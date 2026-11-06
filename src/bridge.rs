//! The bridge: computer sound -> VB-CABLE, on two threads joined by a ring buffer.
//!
//!   capture thread                      output thread
//!   (paced by the headset's clock)      (paced by VB-CABLE's clock)
//!   loopback -> downmix -> [ ring buffer ] -> CABLE Input -> "CABLE Output" mic
//!
//! Each device ticks on its own clock, so the two sides never run in perfect
//! lockstep. The ring buffer absorbs the difference.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::mpsc::Sender;

use rtrb::{Consumer, Producer};
use windows::Win32::Foundation::E_FAIL;
use windows::core::{Error, Result};

use crate::downmix::Downmix;
use crate::loopback::{Format, LoopbackCapture};
use crate::output::{CHANNELS, Output};
use crate::{com_init, devices};

/// Part of the name VB-CABLE gives its playback side.
pub const CABLE_NAME: &str = "CABLE Input";

/// Ring buffer size in samples: 1 second of stereo, even at 192 kHz.
pub const RING_CAPACITY: usize = 192_000 * CHANNELS;

/// Audio held back before playing, so small timing hiccups don't cause gaps.
const TARGET_MS: u32 = 30;
/// If the buffer grows past this (clock drift, a stall), skip back down to the
/// target, so the delay to Discord can't creep up over a long call.
const MAX_MS: u32 = 90;

/// Numbers the threads publish for the main thread to display.
/// Atomics let several threads read and write a number without a lock.
#[derive(Default)]
pub struct Stats {
    /// Loudest sample since the main thread last looked, as f32 bits.
    pub peak: [AtomicU32; 2],
    pub buffered_frames: AtomicUsize,
    /// Times the buffer ran dry while audio was flowing.
    pub gaps: AtomicU64,
    /// Frames thrown away to keep the delay down.
    pub skipped_frames: AtomicU64,
    /// Frames lost because the buffer was full (the output side stalled).
    pub overflow_frames: AtomicU64,
}

pub struct Ready {
    pub source_name: String,
    pub format: Format,
}

pub fn capture_loop(
    mut producer: Producer<f32>,
    stats: Arc<Stats>,
    ready: Sender<Ready>,
) -> Result<()> {
    com_init()?;
    let device = devices::default_output()?;
    let source_name = devices::friendly_name(&device)?;
    if source_name.contains("CABLE") {
        // We'd capture our own output and feed it back in forever.
        return Err(Error::new(
            E_FAIL,
            format!(
                "Windows' default output is {source_name}. Set your headset as the \
                 default playback device in Sound settings, then try again."
            ),
        ));
    }

    let mut capture = LoopbackCapture::new(&device)?;
    let format = capture.format();
    let downmix = Downmix::new(format.channels, format.channel_mask);
    let _ = ready.send(Ready {
        source_name,
        format,
    });
    capture.start()?;

    // The output thread drops its end when it stops; then we stop too.
    while !producer.is_abandoned() {
        capture.wait(100);
        let mut peak = [0.0f32; 2];
        capture.read(|samples| {
            downmix.process(samples, |left, right| {
                peak[0] = peak[0].max(left.abs());
                peak[1] = peak[1].max(right.abs());
                if producer.slots() < CHANNELS {
                    stats.overflow_frames.fetch_add(1, Relaxed);
                    return;
                }
                let _ = producer.push(left);
                let _ = producer.push(right);
            });
        })?;
        // For positive floats, a bigger number also has bigger bits, so
        // comparing the bits as integers finds the max without a lock.
        for (shared, value) in stats.peak.iter().zip(peak) {
            shared.fetch_max(value.to_bits(), Relaxed);
        }
    }
    Ok(())
}

pub fn output_loop(mut consumer: Consumer<f32>, stats: Arc<Stats>, sample_rate: u32) -> Result<()> {
    com_init()?;
    let device = devices::find_output(CABLE_NAME)?.ok_or_else(|| {
        Error::new(
            E_FAIL,
            "VB-CABLE isn't installed (no playback device named \"CABLE Input\").",
        )
    })?;

    let mut output = Output::new(&device, sample_rate)?;
    let ms_to_frames = |ms: u32| (sample_rate * ms / 1000) as usize;
    let (target, max) = (ms_to_frames(TARGET_MS), ms_to_frames(MAX_MS));
    // False while we're waiting for the buffer to refill to `target`.
    let mut primed = false;
    output.start()?;

    while !consumer.is_abandoned() {
        output.wait(200);
        output.write(|out| {
            let wanted = out.len() / CHANNELS;
            let mut available = consumer.slots() / CHANNELS;

            if !primed && available >= target {
                primed = true;
            }
            if primed && available > max {
                let skip = available - target;
                consumer.read_chunk(skip * CHANNELS).unwrap().commit_all();
                stats.skipped_frames.fetch_add(skip as u64, Relaxed);
                available = target;
            }

            let mut written = 0;
            if primed {
                written = wanted.min(available);
                let chunk = consumer.read_chunk(written * CHANNELS).unwrap();
                let (first, second) = chunk.as_slices();
                out[..first.len()].copy_from_slice(first);
                out[first.len()..first.len() + second.len()].copy_from_slice(second);
                chunk.commit_all();

                if written < wanted {
                    // Ran dry. Wait for a full `target` again before resuming,
                    // rather than stuttering sample by sample.
                    primed = false;
                    stats.gaps.fetch_add(1, Relaxed);
                }
            }
            out[written * CHANNELS..].fill(0.0);
            stats
                .buffered_frames
                .store(consumer.slots() / CHANNELS, Relaxed);
        })?;
    }
    Ok(())
}
