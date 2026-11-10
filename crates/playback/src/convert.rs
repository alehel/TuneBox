//! Converts decoded audio to the output device's sample rate and channel count.
//!
//! Rate conversion is linear interpolation: cheap and artefact-free enough for playback when
//! the device runs at a different rate, which `CpalOutput` avoids whenever the device accepts
//! the file's own rate (then this is a straight copy). Channel mapping duplicates mono, folds
//! to mono by averaging, and keeps the front pair when a device has fewer channels.

/// Streaming rate and channel converter for interleaved `f32`.
#[derive(Clone, Debug)]
pub struct Converter {
    in_channels: usize,
    out_channels: usize,
    /// Input frames per output frame.
    step: f64,
    /// Read position, in input frames, relative to the start of the next input block. Between
    /// -1 and 0 it interpolates from `last` (the final frame of the previous block).
    position: f64,
    last: Vec<f32>,
}

impl Converter {
    pub fn new(in_rate: u32, in_channels: u16, out_rate: u32, out_channels: u16) -> Self {
        let in_channels = usize::from(in_channels.max(1));
        Self {
            in_channels,
            out_channels: usize::from(out_channels.max(1)),
            step: f64::from(in_rate.max(1)) / f64::from(out_rate.max(1)),
            position: 0.0,
            last: vec![0.0; in_channels],
        }
    }

    /// Converts one block, appending to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let frames = input.len() / self.in_channels;
        if frames == 0 {
            return;
        }
        if self.step == 1.0 && self.position == 0.0 {
            for frame in input.chunks_exact(self.in_channels) {
                self.map_channels(frame, out);
            }
        } else {
            self.resample(input, frames, out);
        }
        let tail = (frames - 1) * self.in_channels;
        self.last.clear();
        self.last.extend_from_slice(input.get(tail..tail + self.in_channels).unwrap_or_default());
    }

    fn resample(&mut self, input: &[f32], frames: usize, out: &mut Vec<f32>) {
        let mut mixed = vec![0.0; self.in_channels];
        while self.position < (frames - 1) as f64 {
            let index = self.position.floor();
            let fraction = (self.position - index) as f32;
            for (channel, sample) in mixed.iter_mut().enumerate() {
                let at = |i: f64| -> f32 {
                    if i < 0.0 {
                        self.last.get(channel).copied().unwrap_or(0.0)
                    } else {
                        input.get(i as usize * self.in_channels + channel).copied().unwrap_or(0.0)
                    }
                };
                let (a, b) = (at(index), at(index + 1.0));
                *sample = a + (b - a) * fraction;
            }
            self.map_channels(&mixed, out);
            self.position += self.step;
        }
        self.position -= frames as f64;
    }

    fn map_channels(&self, frame: &[f32], out: &mut Vec<f32>) {
        let get = |i: usize| frame.get(i).copied().unwrap_or(0.0);
        match (self.in_channels, self.out_channels) {
            (a, b) if a == b => out.extend_from_slice(frame),
            (1, n) => out.extend(std::iter::repeat_n(get(0), n)),
            (_, 1) => out.push((get(0) + get(1)) * 0.5),
            (_, n) => out.extend((0..n).map(|i| if i < self.in_channels { get(i) } else { 0.0 })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_format_is_a_copy() {
        let mut c = Converter::new(44_100, 2, 44_100, 2);
        let mut out = Vec::new();
        c.process(&[0.1, 0.2, 0.3, 0.4], &mut out);
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn mono_is_duplicated_and_stereo_folded() {
        let mut out = Vec::new();
        Converter::new(48_000, 1, 48_000, 2).process(&[0.5, -0.5], &mut out);
        assert_eq!(out, [0.5, 0.5, -0.5, -0.5]);
        out.clear();
        Converter::new(48_000, 2, 48_000, 1).process(&[0.2, 0.4], &mut out);
        assert!((out[0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn upsampling_produces_the_right_number_of_frames_across_blocks() {
        let mut c = Converter::new(24_000, 1, 48_000, 1);
        let mut out = Vec::new();
        for _ in 0..10 {
            c.process(&[0.0; 2400], &mut out);
        }
        // 1 s of input (less the one frame held back for interpolation) at twice the rate.
        let expected = 48_000;
        assert!((out.len() as i64 - expected).unsigned_abs() <= 2, "{}", out.len());
    }

    #[test]
    fn interpolates_between_samples() {
        let mut c = Converter::new(1, 1, 2, 1);
        let mut out = Vec::new();
        c.process(&[0.0, 1.0, 0.0], &mut out);
        assert_eq!(out, [0.0, 0.5, 1.0, 0.5]);
    }
}
