//! The bridge: computer sound + mic -> VB-CABLE, on three threads joined by
//! two ring buffers.
//!
//!   computer thread (headset's clock)  loopback -> downmix -> [ ring ] --\
//!                                                                         mix -> CABLE Input
//!   mic thread      (mic's clock)      mic      -> downmix -> [ ring ] --/   (cable's clock)
//!
//! Every device ticks on its own clock, so no two sides run in perfect
//! lockstep. Each ring buffer absorbs the difference for its source, and the
//! output thread, paced by VB-CABLE, pulls from both and adds them together.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::mpsc::Sender;

use rtrb::{Consumer, Producer};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
use windows::core::{Error, Result, w};

use crate::capture::{Capture, Format};
use crate::downmix::Downmix;
use crate::mix::{Controls, Mixer};
use crate::output::{CHANNELS, Output};
use crate::{com_init, devices};

/// Part of the name VB-CABLE gives its playback side.
pub const CABLE_NAME: &str = "CABLE Input";

/// Ring buffer size in samples: 1 second of stereo, even at 192 kHz.
pub const RING_CAPACITY: usize = 192_000 * CHANNELS;

/// What a capture thread records.
pub enum Source {
    /// Everything playing on the default speakers/headphones.
    Computer,
    /// A real microphone, resampled to `sample_rate` to match computer sound.
    Mic {
        hint: Option<String>,
        sample_rate: u32,
    },
}

/// How much audio each ring buffer holds back before playing, and how far it
/// may grow before we skip back down, so the delay can't creep up over a long
/// call. Steady state sits about one device period (10 ms) below the target,
/// because the output takes a chunk as soon as the target is reached. The
/// headset mic delivers in 20 ms packets, so 20 ms left it no margin and it
/// ran dry; 30 ms holds.
struct Cushion {
    target_ms: u32,
    max_ms: u32,
}
const COMPUTER_CUSHION: Cushion = Cushion {
    target_ms: 30,
    max_ms: 90,
};
const MIC_CUSHION: Cushion = Cushion {
    target_ms: 30,
    max_ms: 90,
};

/// Numbers one source's threads publish for the main thread to display.
/// Atomics let several threads read and write a number without a lock.
#[derive(Default)]
pub struct SourceStats {
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

#[derive(Default)]
pub struct Stats {
    pub computer: SourceStats,
    pub mic: SourceStats,
    /// Loudest sample of the final mix, before clamping.
    pub out_peak: [AtomicU32; 2],
    /// Samples clamped because the mix went past full scale.
    pub clipped_samples: AtomicU64,
}

pub struct Ready {
    pub device_name: String,
    pub format: Format,
}

/// Ask Windows to treat this thread as live audio ("Pro Audio" in MMCSS), so
/// it gets the CPU on time even when a game or browser is busy. Without it, a
/// late wake-up empties a buffer and you hear a crackle.
fn boost_thread() {
    let mut task_index = 0u32;
    unsafe {
        let _ = AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task_index);
    }
}

fn open(source: &Source) -> Result<(Capture, String)> {
    match source {
        Source::Computer => {
            let device = devices::default_output()?;
            let name = devices::friendly_name(&device)?;
            if name.contains("CABLE") {
                // We'd capture our own output and feed it back in forever.
                return Err(Error::new(
                    E_FAIL,
                    format!(
                        "Windows' default output is {name}. Set your headset as the \
                         default playback device in Sound settings, then try again."
                    ),
                ));
            }
            Ok((Capture::loopback(&device)?, name))
        }
        Source::Mic { hint, sample_rate } => {
            let (device, name) = devices::pick_mic(hint.as_deref())?;
            Ok((Capture::mic(&device, *sample_rate)?, name))
        }
    }
}

/// Record `source`, fold it to stereo, and push it into the ring buffer.
/// `which` picks this source's half of the shared stats.
pub fn capture_loop(
    source: Source,
    mut producer: Producer<f32>,
    stats: Arc<Stats>,
    which: fn(&Stats) -> &SourceStats,
    ready: Sender<Ready>,
) -> Result<()> {
    com_init()?;
    boost_thread();
    let (mut capture, device_name) = open(&source)?;
    let format = capture.format();
    let downmix = Downmix::new(format.channels, format.channel_mask);
    let _ = ready.send(Ready {
        device_name,
        format,
    });
    capture.start()?;
    let stats = which(&stats);

    // The output thread drops its end when it stops; then we stop too.
    while !producer.is_abandoned() {
        // Loopback stops waking us when nothing is playing, but Windows can
        // still deliver the tail end of a sound after that. A short timeout
        // collects it in small steady pieces; a long one would get it in one
        // lump, big enough to overflow the cushion and get trimmed.
        capture.wait(10);
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
        publish_peak(&stats.peak, peak);
    }
    Ok(())
}

/// For positive floats, a bigger number also has bigger bits, so comparing
/// the bits as integers finds the max without a lock.
fn publish_peak(shared: &[AtomicU32; 2], peak: [f32; 2]) {
    for (shared, value) in shared.iter().zip(peak) {
        shared.fetch_max(value.to_bits(), Relaxed);
    }
}

/// The reading end of one source's ring buffer, with its cushion logic.
struct Cushioned {
    consumer: Consumer<f32>,
    target: usize,
    max: usize,
    /// False while we're waiting for the buffer to refill to `target`.
    primed: bool,
}

impl Cushioned {
    fn new(consumer: Consumer<f32>, cushion: Cushion, sample_rate: u32) -> Self {
        let ms_to_frames = |ms: u32| (sample_rate * ms / 1000) as usize;
        Self {
            consumer,
            target: ms_to_frames(cushion.target_ms),
            max: ms_to_frames(cushion.max_ms),
            primed: false,
        }
    }

    /// Fill `out` (interleaved stereo) with as much buffered audio as allowed,
    /// and silence for the rest.
    fn pull(&mut self, out: &mut [f32], stats: &SourceStats) {
        let wanted = out.len() / CHANNELS;
        let mut available = self.consumer.slots() / CHANNELS;

        if !self.primed && available >= self.target {
            self.primed = true;
        }
        if self.primed && available > self.max {
            let skip = available - self.target;
            self.consumer
                .read_chunk(skip * CHANNELS)
                .unwrap()
                .commit_all();
            stats.skipped_frames.fetch_add(skip as u64, Relaxed);
            available = self.target;
        }

        let mut written = 0;
        if self.primed {
            written = wanted.min(available);
            let chunk = self.consumer.read_chunk(written * CHANNELS).unwrap();
            let (first, second) = chunk.as_slices();
            out[..first.len()].copy_from_slice(first);
            out[first.len()..first.len() + second.len()].copy_from_slice(second);
            chunk.commit_all();

            if written < wanted {
                // Ran dry. Wait for a full `target` again before resuming,
                // rather than stuttering sample by sample.
                self.primed = false;
                stats.gaps.fetch_add(1, Relaxed);
            }
        }
        out[written * CHANNELS..].fill(0.0);
        stats
            .buffered_frames
            .store(self.consumer.slots() / CHANNELS, Relaxed);
    }
}

pub fn output_loop(
    computer: Consumer<f32>,
    mic: Consumer<f32>,
    stats: Arc<Stats>,
    controls: Arc<Controls>,
    sample_rate: u32,
) -> Result<()> {
    com_init()?;
    boost_thread();
    let device = devices::find_output(CABLE_NAME)?.ok_or_else(|| {
        Error::new(
            E_FAIL,
            "VB-CABLE isn't installed (no playback device named \"CABLE Input\").",
        )
    })?;

    let mut output = Output::new(&device, sample_rate)?;
    let mut computer = Cushioned::new(computer, COMPUTER_CUSHION, sample_rate);
    let mut mic = Cushioned::new(mic, MIC_CUSHION, sample_rate);
    // Scratch space for each source's block, allocated once up front: the
    // output thread must never wait on the memory allocator mid-stream.
    let mut computer_block = vec![0.0f32; output.buffer_frames() * CHANNELS];
    let mut mic_block = computer_block.clone();
    let mut mixer = Mixer::new(controls.targets());
    // Audio that piled up while the devices were opening is stale: throw it
    // away so we start with the shortest delay (and honest skip counts).
    for ring in [&mut computer.consumer, &mut mic.consumer] {
        let stale = ring.slots();
        ring.read_chunk(stale).unwrap().commit_all();
    }
    output.start()?;

    while !computer.consumer.is_abandoned() && !mic.consumer.is_abandoned() {
        output.wait(200);
        output.write(|out| {
            let n = out.len();
            // Always drain both rings, even when a source is switched off,
            // or its buffer would fill up and overflow.
            computer.pull(&mut computer_block[..n], &stats.computer);
            mic.pull(&mut mic_block[..n], &stats.mic);

            let report = mixer.mix(
                &computer_block[..n],
                &mic_block[..n],
                out,
                controls.targets(),
            );
            publish_peak(&stats.out_peak, report.peak);
            stats.clipped_samples.fetch_add(report.clipped, Relaxed);
        })?;
    }
    Ok(())
}
