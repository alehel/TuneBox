//! The frame TuneBox hands a plug-in, encoded as the ABI's bytes.

use tunebox_visualizer_sdk::abi::{
    BANDS, FRAME_BANDS, FRAME_BASS, FRAME_BYTES, FRAME_DT, FRAME_LEVEL, FRAME_PEAKS, FRAME_PLAYING, FRAME_TIME,
    FRAME_WAVEFORM_COUNT, FRAME_WAVEFORMS, MAX_WAVEFORMS, WAVE_SAMPLES,
};

/// What the audio is doing, for one frame: the measurements built-in looks get
/// (`tunebox_visualizer::Frame`), without the visualizer crate's egui.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameData<'a> {
    pub bands: &'a [f32],
    pub peaks: &'a [f32],
    pub level: f32,
    pub bass: f32,
    /// Oldest first.
    pub waveforms: &'a [Vec<f32>],
    pub time: f32,
    pub dt: f32,
    pub playing: bool,
}

/// The frame as the ABI lays it out. Extra bands, samples and waveforms are left out (the
/// newest waveforms are kept), missing ones are zero, and values that are not numbers become 0.
pub fn encode(frame: &FrameData) -> Vec<u8> {
    let mut bytes = vec![0u8; FRAME_BYTES];
    let mut put = |offset: usize, word: u32| {
        if let Some(slot) = bytes.get_mut(offset..offset + 4) {
            slot.copy_from_slice(&word.to_le_bytes());
        }
    };
    let number = |value: f32| if value.is_finite() { value.to_bits() } else { 0 };
    put(FRAME_TIME, number(frame.time));
    put(FRAME_DT, number(frame.dt));
    put(FRAME_LEVEL, number(frame.level));
    put(FRAME_BASS, number(frame.bass));
    put(FRAME_PLAYING, u32::from(frame.playing));
    for (i, value) in frame.bands.iter().take(BANDS).enumerate() {
        put(FRAME_BANDS + i * 4, number(*value));
    }
    for (i, value) in frame.peaks.iter().take(BANDS).enumerate() {
        put(FRAME_PEAKS + i * 4, number(*value));
    }
    let newest = frame.waveforms.get(frame.waveforms.len().saturating_sub(MAX_WAVEFORMS)..).unwrap_or_default();
    put(FRAME_WAVEFORM_COUNT, newest.len() as u32);
    for (w, wave) in newest.iter().enumerate() {
        for (i, value) in wave.iter().take(WAVE_SAMPLES).enumerate() {
            put(FRAME_WAVEFORMS + (w * WAVE_SAMPLES + i) * 4, number(*value));
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunebox_visualizer_sdk::Frame;

    #[test]
    fn round_trips_through_the_sdk() {
        let bands: Vec<f32> = (0..BANDS).map(|i| i as f32 / BANDS as f32).collect();
        let waves: Vec<Vec<f32>> = (0..8).map(|w| vec![w as f32 / 10.0; WAVE_SAMPLES]).collect();
        let data = FrameData {
            bands: &bands,
            peaks: &[0.5, f32::NAN],
            level: 0.25,
            bass: 0.75,
            waveforms: &waves,
            time: 3.0,
            dt: 0.02,
            playing: true,
        };
        let frame = Frame::from_bytes(&encode(&data));
        assert_eq!(frame.bands.to_vec(), bands);
        assert_eq!(&frame.peaks[..3], &[0.5, 0.0, 0.0]);
        assert_eq!((frame.level, frame.bass, frame.time, frame.dt, frame.playing), (0.25, 0.75, 3.0, 0.02, true));
        // The six newest waveforms, oldest first.
        assert_eq!(frame.waveforms.len(), MAX_WAVEFORMS);
        assert_eq!(frame.waveforms[0][0], 0.2);
        assert_eq!(frame.waveform()[0], 0.7);
        let empty = Frame::from_bytes(&encode(&FrameData::default()));
        assert!(empty.waveforms.is_empty() && !empty.playing);
    }
}
