//! Lossless file writing, block by block.

use craft_audio::{AudioError, BitDepth, EncodeFormat, EncodeOptions, FileEncoder};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use tunebox_model::AudioFormat;

/// The formats TuneBox can rip to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LosslessFormat {
    #[default]
    Flac,
    Wav,
    Aiff,
}

impl LosslessFormat {
    pub const ALL: [LosslessFormat; 3] = [LosslessFormat::Flac, LosslessFormat::Wav, LosslessFormat::Aiff];

    pub fn audio_format(self) -> AudioFormat {
        match self {
            LosslessFormat::Flac => AudioFormat::Flac,
            LosslessFormat::Wav => AudioFormat::Wav,
            LosslessFormat::Aiff => AudioFormat::Aiff,
        }
    }

    /// The name shown in the import settings, e.g. "FLAC Encoder".
    pub fn encoder_name(self) -> &'static str {
        match self {
            LosslessFormat::Flac => "FLAC Encoder",
            LosslessFormat::Wav => "WAV Encoder",
            LosslessFormat::Aiff => "AIFF Encoder",
        }
    }

    fn encode_format(self) -> EncodeFormat {
        match self {
            LosslessFormat::Flac => EncodeFormat::Flac,
            LosslessFormat::Wav => EncodeFormat::Wav,
            LosslessFormat::Aiff => EncodeFormat::Aiff,
        }
    }
}

/// Writes 16-bit audio to a lossless file. The file appears at its final path only when
/// [`PcmWriter::finish`] succeeds; until then it is written next to it as `*.part`, so a
/// cancelled or failed rip never leaves a truncated track behind.
pub struct PcmWriter {
    encoder: FileEncoder<BufWriter<File>>,
    part_path: PathBuf,
    final_path: PathBuf,
}

impl PcmWriter {
    pub fn create(path: &Path, format: LosslessFormat, sample_rate: u32, channels: u16) -> Result<Self, AudioError> {
        let mut part_name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        part_name.push(".part");
        let part_path = path.with_file_name(part_name);
        let file = BufWriter::new(File::create(&part_path)?);
        let options = EncodeOptions::new(format.encode_format()).with_bit_depth(BitDepth::Int16).with_dither(false);
        let encoder = FileEncoder::new(file, &options, sample_rate, channels)?;
        Ok(Self { encoder, part_path, final_path: path.to_path_buf() })
    }

    /// Appends planar samples (one slice per channel, equal lengths).
    pub fn write(&mut self, channels: &[Vec<f32>]) -> Result<(), AudioError> {
        let frames = channels.first().map_or(0, Vec::len);
        self.encoder.write(channels, frames)
    }

    /// Completes the file and moves it into place.
    pub fn finish(self) -> Result<PathBuf, AudioError> {
        let writer = self.encoder.finish()?;
        let file = writer.into_inner().map_err(|e| AudioError::Io(e.into_error()))?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&self.part_path, &self.final_path)?;
        Ok(self.final_path)
    }

    /// Stops writing and removes the partial file.
    pub fn abandon(self) {
        let part = self.part_path.clone();
        drop(self);
        // The partial file is garbage either way; failing to remove it is not worth reporting.
        let _ = std::fs::remove_file(part);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decoder;

    fn tone(frames: usize) -> Vec<Vec<f32>> {
        let wave: Vec<f32> = (0..frames).map(|i| ((i as f32) * 0.05).sin() * 0.5).collect();
        vec![wave.clone(), wave]
    }

    #[test]
    fn writes_each_format_and_decodes_it_back() {
        let dir = tempfile::tempdir().unwrap();
        for format in LosslessFormat::ALL {
            let path = dir.path().join(format!("tone.{}", format.audio_format().extension()));
            let mut writer = PcmWriter::create(&path, format, 44_100, 2).unwrap();
            writer.write(&tone(22_050)).unwrap();
            writer.write(&tone(22_050)).unwrap();
            assert!(!path.exists(), "{format:?}: nothing at the final path before finish");
            writer.finish().unwrap();

            let mut decoder = Decoder::open(&path).unwrap();
            assert_eq!((decoder.sample_rate(), decoder.channels()), (44_100, 2));
            let mut frames = 0;
            while let Some(block) = decoder.next_block().unwrap() {
                frames += block.frames();
            }
            assert_eq!(frames, 44_100, "{format:?}");
        }
    }

    #[test]
    fn abandon_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.flac");
        let mut writer = PcmWriter::create(&path, LosslessFormat::Flac, 44_100, 2).unwrap();
        writer.write(&tone(1000)).unwrap();
        writer.abandon();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn seeking_lands_near_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.wav");
        let mut writer = PcmWriter::create(&path, LosslessFormat::Wav, 44_100, 2).unwrap();
        writer.write(&tone(44_100 * 3)).unwrap();
        writer.finish().unwrap();
        let mut decoder = Decoder::open(&path).unwrap();
        assert_eq!(decoder.duration().map(|d| d.as_secs()), Some(3));
        let landed = decoder.seek(std::time::Duration::from_secs(2)).unwrap();
        assert!((landed.as_secs_f64() - 2.0).abs() < 0.05);
    }

    #[test]
    fn opening_garbage_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.mp3");
        std::fs::write(&path, [0u8; 64]).unwrap();
        assert!(Decoder::open(&path).is_err());
    }
}
