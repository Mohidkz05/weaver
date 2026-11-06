//! Mixing computer sound and the mic into one signal.
//!
//! Mixing is just adding: two sounds playing at once are two sample streams
//! summed. The catch is that samples must stay between -1.0 and 1.0 (full
//! scale). A loud sound effect plus your voice can add up past that, and
//! anything over is chopped flat ("clipping"), which sounds like harsh
//! crackle. For now we clamp and count it; milestone 6 adds a limiter that
//! turns the level down smoothly instead.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering::Relaxed};

pub const MIN_GAIN_DB: i32 = -40;
pub const MAX_GAIN_DB: i32 = 12;

/// What the user has set. The main thread writes these when keys are pressed;
/// the output thread reads them every few milliseconds.
pub struct Controls {
    pub computer_on: AtomicBool,
    pub mic_on: AtomicBool,
    pub computer_gain_db: AtomicI32,
    pub mic_gain_db: AtomicI32,
}

impl Controls {
    pub fn new(computer_on: bool, mic_on: bool) -> Self {
        Self {
            computer_on: AtomicBool::new(computer_on),
            mic_on: AtomicBool::new(mic_on),
            // Sound effects are often mastered right up to full scale, and would
            // drown out (and clip with) a voice at the same level.
            computer_gain_db: AtomicI32::new(-6),
            mic_gain_db: AtomicI32::new(0),
        }
    }

    pub fn toggle(flag: &AtomicBool) {
        flag.fetch_xor(true, Relaxed);
    }

    pub fn nudge(gain_db: &AtomicI32, by: i32) {
        let _ = gain_db.try_update(Relaxed, Relaxed, |db| {
            Some((db + by).clamp(MIN_GAIN_DB, MAX_GAIN_DB))
        });
    }

    /// The multipliers the mixer should be heading for: (computer, mic).
    pub fn targets(&self) -> [f32; 2] {
        let gain = |on: &AtomicBool, db: &AtomicI32| {
            if on.load(Relaxed) {
                db_to_gain(db.load(Relaxed) as f32)
            } else {
                0.0
            }
        };
        [
            gain(&self.computer_on, &self.computer_gain_db),
            gain(&self.mic_on, &self.mic_gain_db),
        ]
    }
}

/// +6 dB roughly doubles the amplitude, -6 dB halves it.
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// What one block of mixing produced, for the meters.
#[derive(Default, Debug, PartialEq)]
pub struct MixReport {
    /// Loudest left/right sample *before* clamping, so the meter can show CLIP.
    pub peak: [f32; 2],
    /// Samples that went past full scale and were clamped.
    pub clipped: u64,
}

pub struct Mixer {
    /// The gains actually in use right now, which glide toward the targets.
    current: [f32; 2],
}

impl Mixer {
    pub fn new(targets: [f32; 2]) -> Self {
        Self { current: targets }
    }

    /// out = computer * gain + mic * gain, for interleaved stereo blocks of
    /// equal length.
    ///
    /// If a gain jumped straight from 1.0 to 0.0 between two samples, the wave
    /// would drop instantly to zero, and a sudden step like that is heard as a
    /// click. So each gain slides from its old value to the new one across
    /// the block (about 10 ms), which sounds like a quick fade instead.
    pub fn mix(
        &mut self,
        computer: &[f32],
        mic: &[f32],
        out: &mut [f32],
        targets: [f32; 2],
    ) -> MixReport {
        let frames = out.len() / 2;
        let mut report = MixReport::default();
        if frames == 0 {
            return report;
        }
        let step = [
            (targets[0] - self.current[0]) / frames as f32,
            (targets[1] - self.current[1]) / frames as f32,
        ];
        let mut gain = self.current;

        for frame in 0..frames {
            gain[0] += step[0];
            gain[1] += step[1];
            for ch in 0..2 {
                let i = frame * 2 + ch;
                let sample = computer[i] * gain[0] + mic[i] * gain[1];
                report.peak[ch] = report.peak[ch].max(sample.abs());
                if sample.abs() > 1.0 {
                    report.clipped += 1;
                }
                out[i] = sample.clamp(-1.0, 1.0);
            }
        }
        // Land exactly on the target, so rounding never leaves a trace of sound.
        self.current = targets;
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_both_sources_with_their_gains() {
        let mut mixer = Mixer::new([0.5, 1.0]);
        let mut out = [0.0; 4];
        mixer.mix(
            &[0.4, 0.4, 0.4, 0.4],
            &[0.1, 0.2, 0.1, 0.2],
            &mut out,
            [0.5, 1.0],
        );
        assert_eq!(out, [0.3, 0.4, 0.3, 0.4]);
    }

    #[test]
    fn turning_off_fades_instead_of_jumping() {
        let mut mixer = Mixer::new([1.0, 0.0]);
        let ones = [1.0; 200];
        let silent = [0.0; 200];
        let mut out = [0.0; 200];
        mixer.mix(&ones, &silent, &mut out, [0.0, 0.0]);
        // Every step down is small: no click.
        for pair in out.chunks_exact(2).collect::<Vec<_>>().windows(2) {
            assert!((pair[0][0] - pair[1][0]).abs() < 0.011);
        }
        assert!(out[198].abs() < 1e-6);
        // The next block is fully silent.
        mixer.mix(&ones, &silent, &mut out, [0.0, 0.0]);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn clamps_and_counts_overs() {
        let mut mixer = Mixer::new([1.0, 1.0]);
        let mut out = [0.0; 2];
        let report = mixer.mix(&[0.8, -0.8], &[0.5, -0.1], &mut out, [1.0, 1.0]);
        assert_eq!(out, [1.0, -0.90000004]);
        assert_eq!(report.clipped, 1);
        assert!((report.peak[0] - 1.3).abs() < 1e-6);
    }

    #[test]
    fn gain_limits_hold() {
        let controls = Controls::new(true, true);
        for _ in 0..100 {
            Controls::nudge(&controls.mic_gain_db, 2);
        }
        assert_eq!(controls.mic_gain_db.load(Relaxed), MAX_GAIN_DB);
        Controls::toggle(&controls.mic_on);
        assert_eq!(controls.targets()[1], 0.0);
    }
}
