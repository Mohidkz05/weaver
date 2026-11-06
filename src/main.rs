mod bridge;
mod devices;
mod downmix;
mod event;
mod loopback;
mod meter;
mod output;

use std::io::Write;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use rtrb::RingBuffer;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::Result;

use bridge::{RING_CAPACITY, Stats};
use meter::Meter;

/// COM is the Windows system every audio API is built on. Each thread that
/// touches audio must switch it on for itself first.
pub fn com_init() -> Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
}

fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        Some("devices") => com_init().and_then(|_| devices::print_all()),
        _ => run_bridge(),
    };
    if let Err(error) = result {
        eprintln!("\nWeaver stopped: {}", error.message());
        std::process::exit(1);
    }
}

fn run_bridge() -> Result<()> {
    let stats = Arc::new(Stats::default());
    let (producer, consumer) = RingBuffer::<f32>::new(RING_CAPACITY);
    let (ready_tx, ready_rx) = mpsc::channel();

    let capture = thread::spawn({
        let stats = stats.clone();
        move || bridge::capture_loop(producer, stats, ready_tx)
    });

    // Wait for the capture side to report its format, or to fail trying.
    let Ok(ready) = ready_rx.recv() else {
        return capture.join().expect("capture thread panicked");
    };
    let rate = ready.format.sample_rate;

    let output = thread::spawn({
        let stats = stats.clone();
        move || bridge::output_loop(consumer, stats, rate)
    });

    println!(
        "Sending {} ({} Hz, {} ch, downmixed to stereo) into {}.",
        ready.source_name, rate, ready.format.channels, bridge::CABLE_NAME,
    );
    println!("Select \"CABLE Output\" as the mic to hear it. Ctrl+C to stop.\n");

    let mut meter = Meter::new(2);
    let frames_to_ms = |frames: u64| frames * 1000 / rate as u64;
    loop {
        thread::sleep(Duration::from_millis(50));

        // If either thread stopped, it hit an error: show it and quit.
        if output.is_finished() {
            return output.join().expect("output thread panicked");
        }
        if capture.is_finished() {
            return capture.join().expect("capture thread panicked");
        }

        let peaks = stats.peak.each_ref().map(|p| f32::from_bits(p.swap(0, Relaxed)));
        print!(
            "\r{}| buffer {:>3} ms | gaps {} | skipped {} ms | lost {} ms   ",
            meter.render(&peaks),
            frames_to_ms(stats.buffered_frames.load(Relaxed) as u64),
            stats.gaps.load(Relaxed),
            frames_to_ms(stats.skipped_frames.load(Relaxed)),
            frames_to_ms(stats.overflow_frames.load(Relaxed)),
        );
        let _ = std::io::stdout().flush();
    }
}
