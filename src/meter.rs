//! Turning peak levels into bars you can see.

use std::time::Instant;

/// Anything quieter than this is drawn as an empty bar.
const FLOOR_DB: f32 = -60.0;
/// How fast the bar falls after a loud sound, in decibels per second.
/// Rising is instant, so short peaks are never missed.
const FALL_DB_PER_SEC: f32 = 30.0;
const BAR_WIDTH: usize = 24;

pub struct Meter {
    /// What each bar currently shows, in dB, after the fall-off is applied.
    shown_db: Vec<f32>,
    last_render: Instant,
}

impl Meter {
    pub fn new(channels: usize) -> Self {
        Self {
            shown_db: vec![FLOOR_DB; channels],
            last_render: Instant::now(),
        }
    }

    /// Draw one line of bars from the loudest sample per channel (0.0 to 1.0+)
    /// seen since the last call.
    pub fn render(&mut self, peaks: &[f32]) -> String {
        let now = Instant::now();
        let dt = now.duration_since(self.last_render).as_secs_f32();
        self.last_render = now;

        let mut line = String::new();
        for (ch, (shown_db, &peak)) in self.shown_db.iter_mut().zip(peaks).enumerate() {
            let fallen = *shown_db - FALL_DB_PER_SEC * dt;
            let shown = to_db(peak).max(fallen).max(FLOOR_DB);
            *shown_db = shown;

            let filled = (((shown - FLOOR_DB) / -FLOOR_DB) * BAR_WIDTH as f32).round() as usize;
            let filled = filled.min(BAR_WIDTH);
            let clip = if shown >= 0.0 { "CLIP" } else { "    " };
            line.push_str(&format!(
                "{} [{}{}] {:>6.1} dB {} ",
                if ch == 0 { "L" } else { "R" },
                "#".repeat(filled),
                "-".repeat(BAR_WIDTH - filled),
                shown,
                clip,
            ));
        }
        line
    }
}

/// Decibels relative to full scale: 1.0 is 0 dB, 0.5 is about -6 dB.
pub fn to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * amplitude.log10()
    }
}
