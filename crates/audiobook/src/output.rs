//! One output file in the chosen format: an MP4 (AAC or Apple Lossless, with chapters) or FLAC.

use std::path::{Path, PathBuf};
use tunebox_audio::{AudioError, LosslessFormat, Mp4AudioWriter, Mp4Chapter, Mp4Codec, Mp4Settings, PcmWriter};
use tunebox_model::audiobook::{OutputFormat, OutputSettings, SAMPLE_RATE};

/// A file being written.
pub enum Output {
    Mp4(Mp4AudioWriter),
    Flac(PcmWriter),
}

impl Output {
    /// Starts a file of `frames` frames at `path`. FLAC files hold no chapters.
    pub fn create(
        path: &Path,
        settings: &OutputSettings,
        frames: u64,
        chapters: Vec<Mp4Chapter>,
    ) -> Result<Self, AudioError> {
        let channels = if settings.is_mono() { 1 } else { 2 };
        let codec = match settings.format {
            OutputFormat::Aac => Mp4Codec::Aac { bitrate_kbps: settings.aac_bitrate_kbps },
            OutputFormat::AppleLossless => Mp4Codec::AppleLossless,
            OutputFormat::Flac => {
                return Ok(Output::Flac(PcmWriter::create(path, LosslessFormat::Flac, SAMPLE_RATE, channels)?));
            }
        };
        let settings = Mp4Settings { codec, sample_rate: SAMPLE_RATE, channels, frames, chapters };
        Ok(Output::Mp4(Mp4AudioWriter::create(path, &settings)?))
    }

    pub fn write(&mut self, block: &[Vec<f32>]) -> Result<(), AudioError> {
        match self {
            Output::Mp4(writer) => writer.write(block),
            Output::Flac(writer) => writer.write(block),
        }
    }

    pub fn finish(self) -> Result<PathBuf, AudioError> {
        match self {
            Output::Mp4(writer) => writer.finish(),
            Output::Flac(writer) => writer.finish(),
        }
    }

    pub fn abandon(self) {
        match self {
            Output::Mp4(writer) => writer.abandon(),
            Output::Flac(writer) => writer.abandon(),
        }
    }
}
