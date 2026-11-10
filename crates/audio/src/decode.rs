//! Streaming decode by file path.

use craft_audio::{AudioError, DecodeOptions};
use std::fs::File;
use std::path::Path;
use std::time::Duration;

/// A run of decoded audio: interleaved samples, full scale ±1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    pub samples: Vec<f32>,
    pub channels: u16,
}

impl Block {
    /// Sample frames in the block (one sample per channel each).
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }
}

/// Decodes one audio file from disk without loading it whole (where the format allows).
pub struct Decoder {
    inner: craft_audio::Decoder<'static>,
    sample_rate: u32,
    channels: u16,
    frames: Option<u64>,
}

impl std::fmt::Debug for Decoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoder").field("sample_rate", &self.sample_rate).field("channels", &self.channels).finish()
    }
}

impl Decoder {
    /// Opens `path`; the extension helps pick the format but the content decides.
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let file = File::open(path)?;
        let hint = path.extension().and_then(|e| e.to_str());
        let inner = craft_audio::Decoder::open(file, hint, &DecodeOptions::default())?;
        let info = inner.info();
        let (sample_rate, channels) = (info.sample_rate, info.channels);
        if sample_rate == 0 || channels == 0 {
            return Err(AudioError::Malformed("the file reports no sample rate or no channels".into()));
        }
        let frames = inner.length_known().then_some(info.frames);
        Ok(Self { inner, sample_rate, channels, frames })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// The length, when the file states it.
    pub fn duration(&self) -> Option<Duration> {
        self.frames.map(|f| Duration::from_secs_f64(f as f64 / f64::from(self.sample_rate)))
    }

    /// The next block, or `None` at the end of the file.
    pub fn next_block(&mut self) -> Result<Option<Block>, AudioError> {
        let Some(buffer) = self.inner.next_block()? else { return Ok(None) };
        Ok(Some(Block { samples: buffer.interleaved(), channels: buffer.num_channels() }))
    }

    /// Moves to `position`; returns where decoding will actually resume.
    pub fn seek(&mut self, position: Duration) -> Result<Duration, AudioError> {
        let frame = (position.as_secs_f64() * f64::from(self.sample_rate)) as u64;
        let landed = self.inner.seek(frame)?;
        Ok(Duration::from_secs_f64(landed as f64 / f64::from(self.sample_rate)))
    }
}
