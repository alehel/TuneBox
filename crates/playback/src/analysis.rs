//! Turning recent audio into what a visualizer draws: a log-spaced spectrum and a level.
//!
//! The FFT is a small radix-2 implementation; a visualizer needs a few hundred bins sixty times
//! a second, which this does in microseconds without another dependency.

use std::f32::consts::PI;

/// What one analysis produces.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spectrum {
    /// Energy per band, 0–1, low frequencies first, spaced logarithmically like hearing.
    pub bands: Vec<f32>,
    /// Overall loudness, 0–1.
    pub level: f32,
}

/// The lowest and highest frequencies the bands cover.
const LOW_HZ: f32 = 40.0;
const HIGH_HZ: f32 = 16_000.0;
/// Decibels below full scale that map to an empty band.
const FLOOR_DB: f32 = -70.0;

/// Analyses mono `samples` (any length; the most recent power of two up to 4096 is used) taken
/// at `sample_rate` into `band_count` bands.
pub fn analyze(samples: &[f32], sample_rate: u32, band_count: usize) -> Spectrum {
    let size = largest_power_of_two(samples.len().min(4096));
    if size < 64 || sample_rate == 0 || band_count == 0 {
        return Spectrum { bands: vec![0.0; band_count], level: 0.0 };
    }
    let tail = samples.get(samples.len() - size..).unwrap_or_default();
    // A Hann window keeps a steady tone from smearing across the spectrum.
    let mut re: Vec<f32> = tail
        .iter()
        .enumerate()
        .map(|(i, s)| s * (0.5 - 0.5 * (2.0 * PI * i as f32 / (size - 1) as f32).cos()))
        .collect();
    let mut im = vec![0.0; size];
    fft(&mut re, &mut im);

    let bin_hz = sample_rate as f32 / size as f32;
    let magnitudes: Vec<f32> =
        re.iter().zip(&im).take(size / 2).map(|(r, i)| (r * r + i * i).sqrt() * 4.0 / size as f32).collect();
    let high = HIGH_HZ.min(sample_rate as f32 / 2.0);
    let bands = (0..band_count)
        .map(|b| {
            let from = LOW_HZ * (high / LOW_HZ).powf(b as f32 / band_count as f32);
            let to = LOW_HZ * (high / LOW_HZ).powf((b + 1) as f32 / band_count as f32);
            let first = (from / bin_hz).floor() as usize;
            let last = ((to / bin_hz).ceil() as usize).max(first + 1);
            let peak = magnitudes
                .get(first..last.min(magnitudes.len()))
                .unwrap_or_default()
                .iter()
                .copied()
                .fold(0.0, f32::max);
            to_unit(peak)
        })
        .collect();
    let rms = (tail.iter().map(|s| s * s).sum::<f32>() / size as f32).sqrt();
    Spectrum { bands, level: to_unit(rms * std::f32::consts::SQRT_2) }
}

/// Amplitude to 0–1 on a decibel scale.
fn to_unit(amplitude: f32) -> f32 {
    if amplitude <= 0.0 || !amplitude.is_finite() {
        return 0.0;
    }
    ((20.0 * amplitude.log10() - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

fn largest_power_of_two(n: usize) -> usize {
    if n == 0 { 0 } else { 1 << (usize::BITS - 1 - n.leading_zeros()) }
}

/// In-place iterative radix-2 FFT. `re` and `im` have the same power-of-two length.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * PI / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (w_re, w_im) = ((angle * k as f32).cos(), (angle * k as f32).sin());
                let (a, b) = (start + k, start + k + len / 2);
                // `a` and `b` are below `n` by construction of the loops.
                let (t_re, t_im) = (re[b] * w_re - im[b] * w_im, re[b] * w_im + im[b] * w_re);
                re[b] = re[a] - t_re;
                im[b] = im[a] - t_im;
                re[a] += t_re;
                im[a] += t_im;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, rate: u32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (2.0 * PI * hz * i as f32 / rate as f32).sin() * 0.5).collect()
    }

    #[test]
    fn a_low_tone_lights_low_bands_and_a_high_tone_high_bands() {
        let low = analyze(&tone(100.0, 44_100, 4096), 44_100, 32);
        let high = analyze(&tone(8_000.0, 44_100, 4096), 44_100, 32);
        let loudest =
            |s: &Spectrum| s.bands.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i).unwrap();
        assert!(loudest(&low) < 8, "{}", loudest(&low));
        assert!(loudest(&high) > 24, "{}", loudest(&high));
        assert!(low.level > 0.8);
    }

    #[test]
    fn silence_and_too_little_audio_are_empty() {
        assert_eq!(analyze(&[0.0; 2048], 44_100, 8).level, 0.0);
        assert_eq!(analyze(&[0.5; 10], 44_100, 8).bands, vec![0.0; 8]);
    }

    #[test]
    fn fft_of_an_impulse_is_flat() {
        let mut re = vec![0.0; 8];
        re[0] = 1.0;
        let mut im = vec![0.0; 8];
        fft(&mut re, &mut im);
        assert!(re.iter().all(|v| (v - 1.0).abs() < 1e-6) && im.iter().all(|v| v.abs() < 1e-6));
    }
}
