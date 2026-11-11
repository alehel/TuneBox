//! Finding the pauses in speech: quiet stretches, judged against the recording's own level.
//!
//! [`LoudnessMeter`] takes audio as it is ripped and keeps one level per 10 ms, so finding
//! pauses costs no second pass over the disc. A pause is at least 0.2 s more than 30 dB below
//! the track's median level, so a quietly recorded book is judged against itself. The
//! thresholds are starting values, to be tuned on real audiobook discs.

use std::ops::Range;

/// The meter's resolution.
const WINDOW_SECONDS: f64 = 0.010;
/// How far below the median level a pause is.
const PAUSE_BELOW_MEDIAN_DB: f32 = 30.0;
/// The shortest pause.
const MIN_PAUSE_SECONDS: f64 = 0.2;
/// Quieter than this is silence, whatever the median.
const SILENCE_DB: f32 = -70.0;

/// Measures loudness every 10 ms.
#[derive(Clone, Debug)]
pub struct LoudnessMeter {
    window: usize,
    sum: f64,
    count: usize,
    levels: Vec<f32>,
}

impl LoudnessMeter {
    pub fn new(sample_rate: u32) -> Self {
        let window = ((f64::from(sample_rate) * WINDOW_SECONDS).round() as usize).max(1);
        Self { window, sum: 0.0, count: 0, levels: Vec::new() }
    }

    /// Takes planar samples (one slice per channel, equal lengths).
    pub fn push(&mut self, block: &[Vec<f32>]) {
        let frames = block.iter().map(Vec::len).min().unwrap_or(0);
        let channels = block.len().max(1) as f64;
        for frame in 0..frames {
            let power: f64 =
                block.iter().filter_map(|c| c.get(frame)).map(|s| f64::from(*s) * f64::from(*s)).sum::<f64>()
                    / channels;
            self.sum += power;
            self.count += 1;
            if self.count == self.window {
                self.close_window();
            }
        }
    }

    /// The pauses found, in samples from the start.
    pub fn pauses(mut self) -> Vec<Range<u64>> {
        if self.count > 0 {
            self.close_window();
        }
        find_pauses(&self.levels, self.window as u64)
    }

    fn close_window(&mut self) {
        let mean = self.sum / self.count.max(1) as f64;
        self.levels.push(decibels(mean));
        self.sum = 0.0;
        self.count = 0;
    }
}

fn decibels(power: f64) -> f32 {
    if power > 0.0 { (10.0 * power.log10()) as f32 } else { f32::NEG_INFINITY }
}

/// The pauses in a series of levels (dB, one per `window` samples), in samples.
pub fn find_pauses(levels: &[f32], window: u64) -> Vec<Range<u64>> {
    let mut sounding: Vec<f32> = levels.iter().copied().filter(|l| *l > SILENCE_DB).collect();
    sounding.sort_by(f32::total_cmp);
    let threshold = match sounding.get(sounding.len() / 2) {
        Some(median) => (median - PAUSE_BELOW_MEDIAN_DB).max(SILENCE_DB),
        None => SILENCE_DB,
    };
    let min_windows = ((MIN_PAUSE_SECONDS / WINDOW_SECONDS).round() as usize).max(1);
    let mut pauses = Vec::new();
    let mut run_start: Option<usize> = None;
    for (index, level) in levels.iter().chain(std::iter::once(&f32::INFINITY)).enumerate() {
        match (run_start, *level <= threshold) {
            (None, true) => run_start = Some(index),
            (Some(start), false) => {
                if index - start >= min_windows {
                    pauses.push(start as u64 * window..index as u64 * window);
                }
                run_start = None;
            }
            _ => {}
        }
    }
    pauses
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Speech-like sound (a tone) at `level` with silences at the given seconds.
    fn speech(seconds: f64, level: f32, gaps: &[(f64, f64)]) -> Vec<Vec<f32>> {
        let frames = (seconds * 44_100.0) as usize;
        let wave: Vec<f32> = (0..frames)
            .map(|i| {
                let t = i as f64 / 44_100.0;
                if gaps.iter().any(|(a, b)| t >= *a && t < *b) {
                    0.0005 * ((i % 7) as f32 - 3.0) / 3.0 * level
                } else {
                    ((t * 220.0 * std::f64::consts::TAU).sin() as f32) * level
                }
            })
            .collect();
        vec![wave.clone(), wave]
    }

    fn pauses_in(audio: &[Vec<f32>]) -> Vec<(f64, f64)> {
        let mut meter = LoudnessMeter::new(44_100);
        // In uneven blocks, as a rip delivers them.
        for chunk in (0..audio[0].len()).step_by(5_000) {
            let end = (chunk + 5_000).min(audio[0].len());
            meter.push(&[audio[0][chunk..end].to_vec(), audio[1][chunk..end].to_vec()]);
        }
        meter.pauses().into_iter().map(|r| (r.start as f64 / 44_100.0, r.end as f64 / 44_100.0)).collect()
    }

    #[test]
    fn finds_pauses_longer_than_a_fifth_of_a_second() {
        let found = pauses_in(&speech(10.0, 0.5, &[(2.0, 2.1), (4.0, 5.5), (8.0, 8.3)]));
        assert_eq!(found.len(), 2, "{found:?}");
        assert!((found[0].0 - 4.0).abs() < 0.02 && (found[0].1 - 5.5).abs() < 0.02, "{found:?}");
        assert!((found[1].0 - 8.0).abs() < 0.02 && (found[1].1 - 8.3).abs() < 0.02, "{found:?}");
    }

    #[test]
    fn a_quiet_recording_is_judged_against_itself() {
        // 40 dB quieter overall: the same pauses are found.
        let loud = pauses_in(&speech(6.0, 0.5, &[(3.0, 4.0)]));
        let quiet = pauses_in(&speech(6.0, 0.005, &[(3.0, 4.0)]));
        assert_eq!(loud.len(), 1);
        assert_eq!(quiet.len(), 1);
    }

    #[test]
    fn silence_at_the_end_is_a_pause_and_empty_input_has_none() {
        let found = pauses_in(&speech(3.0, 0.5, &[(2.5, 3.0)]));
        assert_eq!(found.len(), 1);
        assert!((found[0].1 - 3.0).abs() < 0.02);
        assert!(LoudnessMeter::new(44_100).pauses().is_empty());
    }
}
