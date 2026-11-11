//! Watching how long a plug-in takes to draw.
//!
//! Fuel stops a single runaway frame, but fuel is counted in instructions, not time: on a slow
//! computer a plug-in can stay within its fuel and still take most of every frame. The pace
//! keeps a running average of draw times and says when it has been too slow for long enough
//! that the interface would stutter.

use std::time::Duration;

/// The average draw time above which a plug-in is turned off.
pub const SLOW_FRAME: Duration = Duration::from_millis(8);
/// Frames left out of the average: the first frames pay for caches warming up.
const WARM_UP_FRAMES: u32 = 5;
/// Frames to watch before judging.
const SETTLE_FRAMES: u32 = 30;

/// A running average of draw times.
#[derive(Clone, Debug, Default)]
pub struct Pace {
    frames: u32,
    average_ms: f32,
}

impl Pace {
    /// Records one frame's draw time; `false` once the plug-in has been too slow for too long.
    pub fn record(&mut self, took: Duration) -> bool {
        let ms = took.as_secs_f32() * 1000.0;
        self.frames = self.frames.saturating_add(1);
        if self.frames <= WARM_UP_FRAMES {
            return true;
        }
        self.average_ms = if self.frames == WARM_UP_FRAMES + 1 { ms } else { self.average_ms * 0.9 + ms * 0.1 };
        self.frames < SETTLE_FRAMES || self.average_ms <= SLOW_FRAME.as_secs_f32() * 1000.0
    }

    /// The average draw time so far.
    pub fn average(&self) -> Duration {
        Duration::from_secs_f32(self.average_ms.max(0.0) / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_start_is_forgiven_but_steady_slowness_is_not() {
        let mut pace = Pace::default();
        assert!(pace.record(Duration::from_millis(200)));
        for _ in 0..60 {
            assert!(pace.record(Duration::from_millis(1)));
        }
        let mut slow = Pace::default();
        let verdicts: Vec<bool> = (0..60).map(|_| slow.record(Duration::from_millis(20))).collect();
        assert!(verdicts[..(SETTLE_FRAMES as usize - 1)].iter().all(|ok| *ok));
        assert!(!verdicts[59]);
        assert!(slow.average() > SLOW_FRAME);
    }
}
