mod bridge;
mod capture;
mod devices;
mod downmix;
mod event;
mod meter;
mod mix;
mod output;

use std::io::Write;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rtrb::RingBuffer;
use windows::Win32::Foundation::E_INVALIDARG;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::System::Console::{
    ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
    SetConsoleMode,
};
use windows::core::{Error, Result};

use bridge::{RING_CAPACITY, Ready, Source, SourceStats, Stats};
use meter::Meter;
use mix::Controls;

const HELP: &str = "\
Weaver: sends computer sound and your mic into VB-CABLE, so Discord hears both.

Usage:
  weaver                  start mixing
  weaver devices          list audio devices

Options:
  --mic <name>            use the mic whose name contains <name> (default: Windows' default mic)
  --computer-off          start with computer sound switched off
  --mic-off               start with the mic switched off

While running:
  c  computer sound on/off      [ ]  computer volume down/up
  m  mic on/off                 - =  mic volume down/up
  q  quit";

/// COM is the Windows system every audio API is built on. Each thread that
/// touches audio must switch it on for itself first.
pub fn com_init() -> Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
}

// Read single key presses from the console without waiting for Enter.
// These come from the C runtime that every Rust program on Windows links.
unsafe extern "C" {
    fn _kbhit() -> i32;
    fn _getch() -> i32;
}

struct Options {
    mic: Option<String>,
    computer_on: bool,
    mic_on: bool,
}

enum Command {
    Run(Options),
    Devices,
    Help,
}

fn parse_args() -> Result<Command> {
    let mut options = Options {
        mic: None,
        computer_on: true,
        mic_on: true,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "devices" => return Ok(Command::Devices),
            "-h" | "--help" | "help" => return Ok(Command::Help),
            "--mic" => {
                let name = args.next().ok_or_else(|| bad_arg("--mic needs a name"))?;
                options.mic = Some(name);
            }
            "--computer-off" => options.computer_on = false,
            "--mic-off" => options.mic_on = false,
            other => return Err(bad_arg(&format!("unknown option {other:?}; try --help"))),
        }
    }
    Ok(Command::Run(options))
}

fn bad_arg(message: &str) -> Error {
    Error::new(E_INVALIDARG, message)
}

fn main() {
    let result = parse_args().and_then(|command| match command {
        Command::Devices => com_init().and_then(|_| devices::print_all()),
        Command::Help => {
            println!("{HELP}");
            Ok(())
        }
        Command::Run(options) => run(options),
    });
    if let Err(error) = result {
        eprintln!("\nWeaver stopped: {}", error.message());
        std::process::exit(1);
    }
}

/// Start a capture thread and wait until it has opened its device (or failed).
fn spawn_capture(
    source: Source,
    stats: &Arc<Stats>,
    which: fn(&Stats) -> &SourceStats,
) -> Result<(JoinHandle<Result<()>>, rtrb::Consumer<f32>, Ready)> {
    let (producer, consumer) = RingBuffer::<f32>::new(RING_CAPACITY);
    let (ready_tx, ready_rx) = mpsc::channel();
    let handle = thread::spawn({
        let stats = stats.clone();
        move || bridge::capture_loop(source, producer, stats, which, ready_tx)
    });
    match ready_rx.recv() {
        Ok(ready) => Ok((handle, consumer, ready)),
        // The thread dropped its sender without reporting: it failed to start.
        Err(_) => Err(handle
            .join()
            .expect("capture thread panicked")
            .expect_err("capture thread stopped without an error")),
    }
}

fn run(options: Options) -> Result<()> {
    enable_cursor_codes();
    let stats = Arc::new(Stats::default());
    let controls = Arc::new(Controls::new(options.computer_on, options.mic_on));

    // Computer sound first: its rate becomes the rate for everything else.
    let (computer, computer_ring, computer_ready) =
        spawn_capture(Source::Computer, &stats, |s| &s.computer)?;
    let rate = computer_ready.format.sample_rate;

    let mic_source = Source::Mic {
        hint: options.mic,
        sample_rate: rate,
    };
    let (mic, mic_ring, mic_ready) = spawn_capture(mic_source, &stats, |s| &s.mic)?;

    let output = thread::spawn({
        let (stats, controls) = (stats.clone(), controls.clone());
        move || bridge::output_loop(computer_ring, mic_ring, stats, controls, rate)
    });

    println!(
        "Computer sound: {} ({} Hz, {} ch)",
        computer_ready.device_name, rate, computer_ready.format.channels
    );
    println!(
        "Mic:            {} ({} ch)",
        mic_ready.device_name, mic_ready.format.channels
    );
    println!(
        "Sending both into {}. In Discord, pick \"CABLE Output\" as your input device.",
        bridge::CABLE_NAME
    );
    println!(
        "Keys: c computer on/off  [ ] computer vol  |  m mic on/off  - = mic vol  |  q quit\n"
    );

    let mut threads = vec![output, computer, mic];
    let mut meters = [
        Meter::new(&["L", "R"]),
        Meter::new(&["M"]),
        Meter::new(&["L", "R"]),
    ];
    let frames_to_ms = |frames: u64| frames * 1000 / rate as u64;
    let mut first_draw = true;
    loop {
        thread::sleep(Duration::from_millis(50));

        // If any thread stopped, it hit an error: show it and quit.
        if let Some(i) = threads.iter().position(|t| t.is_finished()) {
            println!();
            return threads
                .swap_remove(i)
                .join()
                .expect("audio thread panicked");
        }

        if handle_keys(&controls) {
            println!("\nStopped.");
            return Ok(());
        }

        // Computer sound pausing is normal (loopback goes quiet when nothing
        // plays), so it's labelled as such; on the mic, running dry is a dropout.
        let source_line = |name: &str, on: bool, db: i32, bars: String, s: &SourceStats| {
            let gaps = if name == "Mic" { "gaps" } else { "pauses" };
            format!(
                "{name:<8} {:<3} {db:>+3} dB {bars}| buf {:>2}ms {gaps} {} skip {}ms lost {}ms",
                if on { "ON" } else { "off" },
                frames_to_ms(s.buffered_frames.load(Relaxed) as u64),
                s.gaps.load(Relaxed),
                frames_to_ms(s.skipped_frames.load(Relaxed)),
                frames_to_ms(s.overflow_frames.load(Relaxed)),
            )
        };
        let computer_peak = take_peak(&stats.computer.peak);
        let mic_peak = take_peak(&stats.mic.peak);
        let out_peak = take_peak(&stats.out_peak);

        let lines = [
            source_line(
                "Computer",
                controls.computer_on.load(Relaxed),
                controls.computer_gain_db.load(Relaxed),
                meters[0].render(&computer_peak),
                &stats.computer,
            ),
            source_line(
                "Mic",
                controls.mic_on.load(Relaxed),
                controls.mic_gain_db.load(Relaxed),
                // The mic is one voice: show one bar.
                format!("{:<64}", meters[1].render(&[loudest(mic_peak)])),
                &stats.mic,
            ),
            format!(
                "{:<19}{}| clipped {}ms",
                "To Discord",
                meters[2].render(&out_peak),
                // Two samples (left and right) per frame.
                frames_to_ms(stats.clipped_samples.load(Relaxed) / 2),
            ),
        ];

        let mut screen = String::new();
        if !first_draw {
            screen.push_str("\x1b[3F"); // back to the start of our 3 lines
        }
        first_draw = false;
        for line in lines {
            screen.push_str("\x1b[2K"); // clear the old line
            screen.push_str(&line);
            screen.push('\n');
        }
        print!("{screen}");
        let _ = std::io::stdout().flush();
    }
}

fn loudest(peak: [f32; 2]) -> f32 {
    peak[0].max(peak[1])
}

/// Read and reset a peak the audio threads have been raising.
fn take_peak(shared: &[AtomicU32; 2]) -> [f32; 2] {
    shared
        .each_ref()
        .map(|p| f32::from_bits(p.swap(0, Relaxed)))
}

/// Apply any keys pressed since last time. Returns true when it's time to quit.
fn handle_keys(controls: &Controls) -> bool {
    while unsafe { _kbhit() } != 0 {
        let key = unsafe { _getch() };
        match key as u8 {
            // Arrow and function keys arrive as two codes; skip the second.
            0 | 0xE0 => {
                unsafe { _getch() };
            }
            b'c' | b'C' => Controls::toggle(&controls.computer_on),
            b'm' | b'M' => Controls::toggle(&controls.mic_on),
            b'[' => Controls::nudge(&controls.computer_gain_db, -2),
            b']' => Controls::nudge(&controls.computer_gain_db, 2),
            b'-' | b'_' => Controls::nudge(&controls.mic_gain_db, -2),
            b'=' | b'+' => Controls::nudge(&controls.mic_gain_db, 2),
            b'q' | b'Q' => return true,
            _ => {}
        }
    }
    false
}

/// Let our cursor-movement codes work in older consoles too (Windows
/// Terminal already understands them). Harmless if output is redirected.
fn enable_cursor_codes() {
    unsafe {
        if let Ok(handle) = GetStdHandle(STD_OUTPUT_HANDLE) {
            let mut mode = Default::default();
            if GetConsoleMode(handle, &mut mode).is_ok() {
                let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
        }
    }
}
