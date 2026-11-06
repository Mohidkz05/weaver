//! Playing stereo audio into a device. Here that device is VB-CABLE's
//! "CABLE Input": whatever plays into it comes out of the "CABLE Output" mic.

use std::slice;

use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioClient,
    IAudioRenderClient, IMMDevice,
};
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::core::Result;

use crate::capture::{HNS_PER_SECOND, float_format};
use crate::event::Event;

pub const CHANNELS: usize = 2;

pub struct Output {
    client: IAudioClient,
    render: IAudioRenderClient,
    event: Event,
    buffer_frames: u32,
}

impl Output {
    /// Open `device` for stereo float audio at `sample_rate`. If the device runs
    /// at a different rate, Windows converts for us (AUTOCONVERTPCM), with
    /// proper filtering (SRC_DEFAULT_QUALITY) so it doesn't sound harsh.
    pub fn new(device: &IMMDevice, sample_rate: u32) -> Result<Self> {
        let format = float_format(CHANNELS, sample_rate);
        unsafe {
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                HNS_PER_SECOND / 50, // 20 ms device buffer
                0,
                &format,
                None,
            )?;
            let event = Event::new()?;
            client.SetEventHandle(event.handle())?;
            let render: IAudioRenderClient = client.GetService()?;
            let buffer_frames = client.GetBufferSize()?;

            // Start from a buffer full of silence, so the very first period
            // doesn't play garbage.
            render.GetBuffer(buffer_frames)?;
            render.ReleaseBuffer(buffer_frames, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;

            Ok(Self {
                client,
                render,
                event,
                buffer_frames,
            })
        }
    }

    /// The most frames a single `write` can ask for.
    pub fn buffer_frames(&self) -> usize {
        self.buffer_frames as usize
    }

    pub fn start(&self) -> Result<()> {
        unsafe { self.client.Start() }
    }

    /// Sleep until the device wants more audio.
    pub fn wait(&self, timeout_ms: u32) {
        self.event.wait(timeout_ms);
    }

    /// Ask how much room the device has, and let `fill` write that many
    /// interleaved stereo samples straight into its buffer.
    pub fn write(&mut self, fill: impl FnOnce(&mut [f32])) -> Result<()> {
        unsafe {
            // "Padding" is audio we already gave it that hasn't played yet.
            let free = self.buffer_frames - self.client.GetCurrentPadding()?;
            if free == 0 {
                return Ok(());
            }
            let data = self.render.GetBuffer(free)?;
            let len = free as usize * CHANNELS;
            fill(slice::from_raw_parts_mut(data as *mut f32, len));
            self.render.ReleaseBuffer(free, 0)
        }
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}
