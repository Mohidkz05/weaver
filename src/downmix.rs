//! Folding any speaker layout (mono, 5.1, 7.1, ...) down to stereo.

/// -3 dB: a sound sent to *both* sides at this level is as loud as it was on one speaker.
const HALF_POWER: f32 = std::f32::consts::FRAC_1_SQRT_2;

// Windows' speaker bits (SPEAKER_* in ksmedia.h). Channels in a stream
// appear in the order of these bits, lowest first.
const FRONT_LEFT: u32 = 0x1;
const FRONT_RIGHT: u32 = 0x2;
const LOW_FREQUENCY: u32 = 0x8;
const BACK_LEFT: u32 = 0x10;
const BACK_RIGHT: u32 = 0x20;
const FRONT_LEFT_OF_CENTER: u32 = 0x40;
const FRONT_RIGHT_OF_CENTER: u32 = 0x80;
const SIDE_LEFT: u32 = 0x200;
const SIDE_RIGHT: u32 = 0x400;
const TOP_FRONT_LEFT: u32 = 0x1000;
const TOP_FRONT_RIGHT: u32 = 0x4000;
const TOP_BACK_LEFT: u32 = 0x8000;
const TOP_BACK_RIGHT: u32 = 0x20000;

pub struct Downmix {
    /// For each input channel: how much goes to (left, right).
    gains: Vec<(f32, f32)>,
}

impl Downmix {
    pub fn new(channels: usize, channel_mask: u32) -> Self {
        if channels == 1 {
            return Self {
                gains: vec![(1.0, 1.0)],
            };
        }

        // Walk the set bits of the mask in order: the n-th set bit is channel n.
        let mut speakers = (0..32)
            .map(|bit| 1u32 << bit)
            .filter(|s| channel_mask & s != 0);
        let gains = (0..channels)
            .map(|_| match speakers.next() {
                Some(FRONT_LEFT | FRONT_LEFT_OF_CENTER) => (1.0, 0.0),
                Some(FRONT_RIGHT | FRONT_RIGHT_OF_CENTER) => (0.0, 1.0),
                Some(BACK_LEFT | SIDE_LEFT | TOP_FRONT_LEFT | TOP_BACK_LEFT) => (HALF_POWER, 0.0),
                Some(BACK_RIGHT | SIDE_RIGHT | TOP_FRONT_RIGHT | TOP_BACK_RIGHT) => {
                    (0.0, HALF_POWER)
                }
                // The subwoofer channel is a duplicate of the bass; adding it muddies the mix.
                Some(LOW_FREQUENCY) => (0.0, 0.0),
                // Centre, back centre, top centre, or a channel Windows didn't label.
                _ => (HALF_POWER, HALF_POWER),
            })
            .collect();
        Self { gains }
    }

    /// Mix one block of interleaved input into interleaved stereo, calling
    /// `out(left, right)` for each frame.
    pub fn process(&self, input: &[f32], mut out: impl FnMut(f32, f32)) {
        for frame in input.chunks_exact(self.gains.len()) {
            let (mut left, mut right) = (0.0, 0.0);
            for (&sample, &(to_left, to_right)) in frame.iter().zip(&self.gains) {
                left += sample * to_left;
                right += sample * to_right;
            }
            out(left, right);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_passes_through() {
        let mix = Downmix::new(2, 0x3);
        let mut got = vec![];
        mix.process(&[0.1, 0.2, 0.3, 0.4], |l, r| got.push((l, r)));
        assert_eq!(got, vec![(0.1, 0.2), (0.3, 0.4)]);
    }

    #[test]
    fn surround_7_1_folds_down() {
        // Order: FL FR FC LFE BL BR SL SR
        let mix = Downmix::new(8, 0x63F);
        let mut got = vec![];
        mix.process(&[1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], |l, r| {
            got.push((l, r))
        });
        let (l, r) = got[0];
        assert_eq!(l, 1.0); // front left only; the subwoofer is dropped
        assert!((r - HALF_POWER).abs() < 1e-6); // side right, at -3 dB
    }
}
