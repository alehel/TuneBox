//! The meter: from the player's recent audio to a smoothed [`Frame`](crate::Frame).

use crate::Frame;
use tunebox_engine::{AudioTap, analysis};

/// How many bands the spectrum is split into.
pub const BANDS: usize = 64;
/// How many past waveforms are kept for trails.
const TRAILS: usize = 6;

/// Smoothed audio measurements, updated once per frame.
#[derive(Clone, Debug)]
pub struct Meter {
    bands: Vec<f32>,
    peaks: Vec<f32>,
    level: f32,
    bass: f32,
    waveforms: Vec<Vec<f32>>,
    time: f32,
    dt: f32,
    playing: bool,
}

impl Default for Meter {
    fn default() -> Self {
        Self {
            bands: vec![0.0; BANDS],
            peaks: vec![0.0; BANDS],
            level: 0.0,
            bass: 0.0,
            waveforms: Vec::new(),
            time: 0.0,
            dt: 0.0,
            playing: false,
        }
    }
}

impl Meter {
    /// Takes in the audio played lately (`None` when nothing plays) and `dt` seconds of time.
    pub fn update(&mut self, tap: Option<&AudioTap>, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        self.dt = dt;
        self.time += dt;
        self.playing = tap.is_some();
        let (target, level, waveform) = match tap {
            Some(tap) => {
                let spectrum = analysis::analyze(&tap.samples, tap.sample_rate, BANDS);
                (spread(&spectrum.bands), spectrum.level, normalised_waveform(&tap.samples))
            }
            None => (vec![0.0; BANDS], 0.0, vec![0.0; 256]),
        };
        // Rise quickly, fall gently: lively without flicker.
        for ((band, peak), target) in self.bands.iter_mut().zip(self.peaks.iter_mut()).zip(&target) {
            let rate = if *target > *band { 18.0 } else { 3.5 };
            *band += (target - *band) * (rate * dt).min(1.0);
            *peak = if *band > *peak { *band } else { (*peak - 0.35 * dt).max(0.0) };
        }
        self.level += (level - self.level) * (6.0 * dt).min(1.0);
        let bass = self.bands.iter().take(6).sum::<f32>() / 6.0;
        self.bass += (bass - self.bass) * (10.0 * dt).min(1.0);
        self.waveforms.push(waveform);
        if self.waveforms.len() > TRAILS {
            self.waveforms.remove(0);
        }
    }

    /// The current measurements, for a look to paint.
    pub fn frame(&self) -> Frame<'_> {
        Frame {
            bands: &self.bands,
            peaks: &self.peaks,
            level: self.level,
            bass: self.bass,
            waveforms: &self.waveforms,
            time: self.time,
            dt: self.dt,
            playing: self.playing,
        }
    }
}

/// Lets each band glow into its neighbours, so single tones become soft hills rather than
/// lone spikes.
fn spread(bands: &[f32]) -> Vec<f32> {
    (0..bands.len())
        .map(|i| {
            bands
                .iter()
                .enumerate()
                .map(|(j, value)| value * 0.72f32.powi((i as i32 - j as i32).abs()))
                .fold(0.0, f32::max)
        })
        .collect()
}

/// 256 points of the latest audio, scaled so the loudest is ±1 (quiet music still draws).
fn normalised_waveform(samples: &[f32]) -> Vec<f32> {
    let tail = samples.get(samples.len().saturating_sub(1024)..).unwrap_or_default();
    let peak = tail.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(0.02);
    (0..256).map(|i| tail.get(i * tail.len() / 256).copied().unwrap_or(0.0) / peak).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreading_turns_a_spike_into_a_hill() {
        let mut bands = vec![0.0; 9];
        bands[4] = 1.0;
        let hill = spread(&bands);
        assert_eq!(hill[4], 1.0);
        assert!(hill[3] > 0.5 && hill[3] < 1.0 && hill[3] == hill[5]);
        assert!(hill[0] < hill[3]);
    }

    #[test]
    fn a_tone_raises_the_bands_and_silence_lets_them_fall() {
        let samples: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let tap = AudioTap { samples, sample_rate: 44_100 };
        let mut meter = Meter::default();
        for _ in 0..30 {
            meter.update(Some(&tap), 1.0 / 60.0);
        }
        let loud = meter.frame().bands.iter().copied().fold(0.0, f32::max);
        assert!(loud > 0.5 && meter.frame().playing);
        for _ in 0..120 {
            meter.update(None, 1.0 / 60.0);
        }
        assert!(meter.frame().bands.iter().copied().fold(0.0, f32::max) < loud / 2.0);
        assert!(!meter.frame().playing);
        assert_eq!(meter.frame().waveforms.len(), TRAILS);
    }
}
