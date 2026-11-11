//! The frame a look draws from, read out of the buffer TuneBox fills.

use crate::abi::{
    BANDS, FRAME_BANDS, FRAME_BASS, FRAME_BYTES, FRAME_DT, FRAME_LEVEL, FRAME_PEAKS, FRAME_PLAYING, FRAME_TIME,
    FRAME_WAVEFORM_COUNT, FRAME_WAVEFORMS, MAX_WAVEFORMS, WAVE_SAMPLES,
};

/// What the audio is doing, for one frame. The same measurements built-in looks get.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Energy per band, 0–1, lowest frequencies first, spaced like hearing. Smoothed: rises
    /// quickly, falls gently.
    pub bands: [f32; BANDS],
    /// Per band, the recent maximum, falling slowly (for peak markers).
    pub peaks: [f32; BANDS],
    /// Overall loudness, 0–1.
    pub level: f32,
    /// Loudness of the lowest bands, 0–1: the beat.
    pub bass: f32,
    /// The last few waveforms, oldest first; each is [`WAVE_SAMPLES`] samples scaled to ±1.
    pub waveforms: Vec<[f32; WAVE_SAMPLES]>,
    /// Seconds since the visualizer opened.
    pub time: f32,
    /// Seconds since the previous frame.
    pub dt: f32,
    /// Whether anything is playing. Looks keep moving gently when nothing is.
    pub playing: bool,
}

impl Frame {
    /// Reads a frame from the ABI's bytes (missing bytes read as zero).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let word = |offset: usize| -> u32 {
            bytes.get(offset..offset + 4).and_then(|b| b.try_into().ok()).map_or(0, u32::from_le_bytes)
        };
        let float = |offset: usize| f32::from_bits(word(offset));
        let mut bands = [0.0; BANDS];
        let mut peaks = [0.0; BANDS];
        for i in 0..BANDS {
            bands[i] = float(FRAME_BANDS + i * 4);
            peaks[i] = float(FRAME_PEAKS + i * 4);
        }
        let count = (word(FRAME_WAVEFORM_COUNT) as usize).min(MAX_WAVEFORMS);
        let waveforms = (0..count)
            .map(|w| {
                let mut wave = [0.0; WAVE_SAMPLES];
                for (i, sample) in wave.iter_mut().enumerate() {
                    *sample = float(FRAME_WAVEFORMS + (w * WAVE_SAMPLES + i) * 4);
                }
                wave
            })
            .collect();
        Self {
            bands,
            peaks,
            level: float(FRAME_LEVEL),
            bass: float(FRAME_BASS),
            waveforms,
            time: float(FRAME_TIME),
            dt: float(FRAME_DT),
            playing: word(FRAME_PLAYING) != 0,
        }
    }

    /// The band nearest `position` (0 = lowest frequency, 1 = highest).
    pub fn band_at(&self, position: f32) -> f32 {
        let index = (position.clamp(0.0, 1.0) * (BANDS - 1) as f32).round() as usize;
        self.bands.get(index).copied().unwrap_or(0.0)
    }

    /// The newest waveform (silence when there is none).
    pub fn waveform(&self) -> [f32; WAVE_SAMPLES] {
        self.waveforms.last().copied().unwrap_or([0.0; WAVE_SAMPLES])
    }
}

/// The buffer is exactly one frame.
const _: () = assert!(FRAME_BYTES.is_multiple_of(4));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fields_and_caps_the_waveform_count() {
        let mut bytes = vec![0u8; FRAME_BYTES];
        bytes[FRAME_TIME..FRAME_TIME + 4].copy_from_slice(&2.5f32.to_le_bytes());
        bytes[FRAME_PLAYING..FRAME_PLAYING + 4].copy_from_slice(&1u32.to_le_bytes());
        bytes[FRAME_WAVEFORM_COUNT..FRAME_WAVEFORM_COUNT + 4].copy_from_slice(&99u32.to_le_bytes());
        let last_band = FRAME_BANDS + (BANDS - 1) * 4;
        bytes[last_band..last_band + 4].copy_from_slice(&0.75f32.to_le_bytes());
        let frame = Frame::from_bytes(&bytes);
        assert_eq!(frame.time, 2.5);
        assert!(frame.playing);
        assert_eq!(frame.waveforms.len(), MAX_WAVEFORMS);
        assert_eq!(frame.band_at(1.0), 0.75);
        assert_eq!(Frame::from_bytes(&[]).waveforms.len(), 0);
    }
}
