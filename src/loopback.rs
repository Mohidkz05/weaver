//! Copying whatever Windows is playing, without rerouting it ("loopback").

use std::slice;

use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient, IMMDevice, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE,
};
use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::{CLSCTX_ALL, CoTaskMemFree};
use windows::core::{Error, HRESULT, Result};

use crate::event::Event;

/// Windows measures audio time in units of 100 nanoseconds.
pub const HNS_PER_SECOND: i64 = 10_000_000;

/// What the captured stream looks like.
#[derive(Clone, Copy, Debug)]
pub struct Format {
    pub channels: usize,
    pub sample_rate: u32,
    /// Which speaker each channel feeds (front left, centre, ...), one bit per speaker.
    pub channel_mask: u32,
}

pub struct LoopbackCapture {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: Event,
    format: Format,
    /// Reused when Windows says a packet is silent, so we never allocate in the loop.
    silence: Vec<f32>,
}

impl LoopbackCapture {
    /// Capture a copy of everything playing on `device` (a speaker/headphone device).
    pub fn new(device: &IMMDevice) -> Result<Self> {
        unsafe {
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

            // The format Windows mixes everything into before sending it to the device.
            let format_ptr = client.GetMixFormat()?;
            let format = read_format(format_ptr);
            let init = client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                HNS_PER_SECOND / 10, // room for 100 ms if our thread is ever late
                0,
                format_ptr,
                None,
            );
            CoTaskMemFree(Some(format_ptr as *const _));
            init?;
            let format = format?;

            let event = Event::new()?;
            client.SetEventHandle(event.handle())?;
            let capture: IAudioCaptureClient = client.GetService()?;
            Ok(Self {
                client,
                capture,
                event,
                format,
                silence: Vec::new(),
            })
        }
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn start(&self) -> Result<()> {
        unsafe { self.client.Start() }
    }

    /// Sleep until Windows has new audio for us (or `timeout_ms` passes,
    /// which is normal: when nothing is playing, loopback delivers nothing).
    pub fn wait(&self, timeout_ms: u32) {
        self.event.wait(timeout_ms);
    }

    /// Hand every waiting packet to `on_samples`, as interleaved f32 samples
    /// (left, right, left, right, ... for stereo). Returns once the queue is empty.
    pub fn read(&mut self, mut on_samples: impl FnMut(&[f32])) -> Result<()> {
        unsafe {
            while self.capture.GetNextPacketSize()? > 0 {
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;

                let len = frames as usize * self.format.channels;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                    self.silence.resize(len, 0.0);
                    on_samples(&self.silence[..len]);
                } else {
                    on_samples(slice::from_raw_parts(data as *const f32, len));
                }

                // Tell Windows we're done, so it can reuse that memory.
                self.capture.ReleaseBuffer(frames)?;
            }
        }
        Ok(())
    }
}

impl Drop for LoopbackCapture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

/// Check the mix format is 32-bit float (what Windows uses in shared mode) and
/// pull out the layout.
unsafe fn read_format(ptr: *const WAVEFORMATEX) -> Result<Format> {
    let format = unsafe { ptr.read_unaligned() };
    let tag = format.wFormatTag as u32;
    let channels = format.nChannels as usize;

    let (is_float, channel_mask) = if tag == WAVE_FORMAT_EXTENSIBLE {
        let ext = unsafe { (ptr as *const WAVEFORMATEXTENSIBLE).read_unaligned() };
        // Copy fields out first: `==` borrows, and borrowing inside a
        // packed struct could point at a misaligned address.
        let sub_format = ext.SubFormat;
        let mask = ext.dwChannelMask;
        (sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, mask)
    } else {
        (tag == WAVE_FORMAT_IEEE_FLOAT, default_mask(channels))
    };

    if !is_float || format.wBitsPerSample != 32 {
        let bits = format.wBitsPerSample;
        return Err(Error::new(
            HRESULT(0x8889_0008u32 as i32), // AUDCLNT_E_UNSUPPORTED_FORMAT
            format!("mix format is {bits}-bit tag {tag:#x}, expected 32-bit float"),
        ));
    }
    Ok(Format {
        channels,
        sample_rate: format.nSamplesPerSec,
        channel_mask,
    })
}

/// The usual speaker layout for a channel count, when Windows doesn't say.
fn default_mask(channels: usize) -> u32 {
    match channels {
        1 => 0x4,   // centre
        2 => 0x3,   // front left, front right
        6 => 0x3F,  // 5.1
        8 => 0x63F, // 7.1
        _ => 0,
    }
}
