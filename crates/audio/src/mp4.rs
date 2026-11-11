//! MP4 audio files (`.m4a`, `.m4b`) in AAC or Apple Lossless, with chapters.
//!
//! craft-audio's encoders produce packets; craft-media's muxer puts them in an MP4 file with a
//! chapter list in both forms players read (a QuickTime chapter track for Apple's players, a
//! Nero list for the others). The file is written next to its final path as `*.part` and moved
//! into place only when complete, as [`crate::PcmWriter`] does.
//!
//! AAC files are gapless: an edit list skips the encoder's priming and ends at the last real
//! sample, which is why the length must be known before writing starts.

use craft_audio::AudioError;
use craft_audio::craft_media::isobmff::{
    self, Brand, Chapter, CodecConfig, Edit, FourCc, Mp4Writer, ReadSeekSource, SampleEntry, TrackConfig, WriteSample,
    WriterOptions,
};
use craft_audio::{aac, alac};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Samples per AAC access unit.
const AAC_FRAME: u32 = 1024;

/// The codec of an MP4 audio file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mp4Codec {
    /// AAC-LC at a constant bit rate.
    Aac { bitrate_kbps: u32 },
    /// Apple Lossless, 16-bit.
    AppleLossless,
}

/// A chapter mark: where it starts, in samples from the start of the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mp4Chapter {
    pub start: u64,
    pub title: String,
}

/// What to write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mp4Settings {
    pub codec: Mp4Codec,
    pub sample_rate: u32,
    pub channels: u16,
    /// Exactly how many frames will be written; [`Mp4AudioWriter::finish`] refuses any other count.
    pub frames: u64,
    pub chapters: Vec<Mp4Chapter>,
}

enum Codec {
    Aac(Box<aac::Encoder>),
    Alac { encoder: Box<alac::Encoder>, pending: Vec<Vec<i32>>, frame_length: usize },
}

/// Writes an MP4 audio file block by block.
pub struct Mp4AudioWriter {
    muxer: Mp4Writer<ReadableBufWriter>,
    track: usize,
    codec: Codec,
    channels: usize,
    expected: u64,
    written: u64,
    part_path: PathBuf,
    final_path: PathBuf,
}

impl Mp4AudioWriter {
    pub fn create(path: &Path, settings: &Mp4Settings) -> Result<Self, AudioError> {
        let channels = usize::from(settings.channels);
        let (codec, entry, edits) = match settings.codec {
            Mp4Codec::Aac { bitrate_kbps } => {
                let config = aac::EncoderConfig::cbr(settings.sample_rate, channels, bitrate_kbps.saturating_mul(1000));
                let encoder = aac::Encoder::new(config)?;
                let entry = SampleEntry::aac(
                    encoder.audio_specific_config(),
                    u32::from(settings.channels),
                    settings.sample_rate,
                );
                // Skip the priming, end at the last real sample: the movie timescale is the
                // sample rate, so both are in samples.
                let edit = Edit {
                    segment_duration: settings.frames,
                    media_time: i64::from(encoder.priming_samples()),
                    media_rate: 0x0001_0000,
                };
                (Codec::Aac(Box::new(encoder)), entry, vec![edit])
            }
            Mp4Codec::AppleLossless => {
                let channels_u8 = u8::try_from(channels).map_err(|_| AudioError::Encode("too many channels".into()))?;
                let config = alac::EncoderConfig::new(settings.sample_rate, channels_u8, 16);
                let encoder = alac::Encoder::new(config)?;
                let frame_length = encoder.encoder_config().frame_length as usize;
                let entry = alac_entry(&encoder, settings);
                let codec =
                    Codec::Alac { encoder: Box::new(encoder), pending: vec![Vec::new(); channels], frame_length };
                (codec, entry, Vec::new())
            }
        };
        let mut options = WriterOptions::new(Brand::Mp4);
        options.movie_timescale = settings.sample_rate;
        options.chapters =
            settings.chapters.iter().map(|c| Chapter { start: c.start, title: c.title.clone() }).collect();

        let part_path = part_path(path);
        // Read as well as write: moving the index to the front reads the media data back.
        let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&part_path)?;
        let result = (|| {
            let mut muxer = Mp4Writer::new(ReadableBufWriter(BufWriter::new(file)), options).map_err(mux_error)?;
            let mut track_config = TrackConfig::new(entry, settings.sample_rate);
            track_config.edits = edits;
            let track = muxer.add_track(track_config).map_err(mux_error)?;
            Ok((muxer, track))
        })();
        let (muxer, track) = match result {
            Ok(ok) => ok,
            Err(e) => {
                let _ = std::fs::remove_file(&part_path);
                return Err(e);
            }
        };
        Ok(Self {
            muxer,
            track,
            codec,
            channels,
            expected: settings.frames,
            written: 0,
            part_path,
            final_path: path.to_path_buf(),
        })
    }

    /// Appends planar samples, one slice per channel, of equal length.
    pub fn write(&mut self, block: &[Vec<f32>]) -> Result<(), AudioError> {
        let frames = block.first().map_or(0, Vec::len);
        if block.len() != self.channels || block.iter().any(|c| c.len() != frames) {
            return Err(AudioError::Encode(format!(
                "a block needs {} channels of equal length, got {}",
                self.channels,
                block.len()
            )));
        }
        self.written += frames as u64;
        if self.written > self.expected {
            return Err(AudioError::Encode(format!("more audio than the {} frames announced", self.expected)));
        }
        match &mut self.codec {
            Codec::Aac(encoder) => {
                let input: Vec<&[f32]> = block.iter().map(Vec::as_slice).collect();
                let units = encoder.encode(&input);
                write_aac_units(&mut self.muxer, self.track, &units)
            }
            Codec::Alac { encoder, pending, frame_length } => {
                for (buffer, channel) in pending.iter_mut().zip(block) {
                    buffer.extend(channel.iter().map(|s| to_i16(*s)));
                }
                while pending.first().is_some_and(|p| p.len() >= *frame_length) {
                    let packet: Vec<Vec<i32>> =
                        pending.iter_mut().map(|p| p.drain(..*frame_length).collect()).collect();
                    write_alac_packet(&mut self.muxer, self.track, encoder, &packet)?;
                }
                Ok(())
            }
        }
    }

    /// Completes the file, puts the index in front ("fast start") and moves it into place. On
    /// failure the partial file is removed.
    pub fn finish(self) -> Result<PathBuf, AudioError> {
        let Self { mut muxer, track, codec, expected, written, part_path, final_path, .. } = self;
        let result = (|| {
            if written != expected {
                return Err(AudioError::Encode(format!("{written} frames written of the {expected} announced")));
            }
            match codec {
                Codec::Aac(mut encoder) => write_aac_units(&mut muxer, track, &encoder.flush())?,
                Codec::Alac { mut encoder, pending, .. } => {
                    if pending.first().is_some_and(|p| !p.is_empty()) {
                        write_alac_packet(&mut muxer, track, &mut encoder, &pending)?;
                    }
                    // The configuration's statistics cover the whole stream only now.
                    let entry = SampleEntry::audio(
                        FourCc(*b"alac"),
                        CodecConfig::Alac { cookie: encoder.cookie() },
                        u32::try_from(pending.len()).unwrap_or(2),
                        f64::from(encoder.encoder_config().sample_rate),
                        16,
                    );
                    muxer.set_sample_entry(track, entry).map_err(mux_error)?;
                }
            }
            let writer = muxer.finish_faststart().map_err(mux_error)?;
            let file = writer.0.into_inner().map_err(|e| AudioError::Io(e.into_error()))?;
            file.sync_all()?;
            std::fs::rename(&part_path, &final_path)?;
            Ok(final_path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&part_path);
        }
        result
    }

    /// Stops writing and removes the partial file.
    pub fn abandon(self) {
        let part = self.part_path.clone();
        drop(self);
        // The partial file is garbage either way; failing to remove it is not worth reporting.
        let _ = std::fs::remove_file(part);
    }
}

/// The chapters of an MP4 file, with their start times.
pub fn read_mp4_chapters(path: &Path) -> Result<Vec<(Duration, String)>, AudioError> {
    let source = ReadSeekSource::new(File::open(path)?)?;
    let file = isobmff::open(source).map_err(mux_error)?;
    let source = ReadSeekSource::new(File::open(path)?)?;
    let chapters = file.read_chapters(&source).map_err(mux_error)?;
    let scale = f64::from(file.timescale.max(1));
    Ok(chapters.into_iter().map(|c| (Duration::from_secs_f64(c.start as f64 / scale), c.title)).collect())
}

fn alac_entry(encoder: &alac::Encoder, settings: &Mp4Settings) -> SampleEntry {
    SampleEntry::audio(
        FourCc(*b"alac"),
        CodecConfig::Alac { cookie: encoder.cookie() },
        u32::from(settings.channels),
        f64::from(settings.sample_rate),
        16,
    )
}

fn write_aac_units(
    muxer: &mut Mp4Writer<ReadableBufWriter>,
    track: usize,
    units: &[Vec<u8>],
) -> Result<(), AudioError> {
    for unit in units {
        let sample = WriteSample { data: unit, duration: AAC_FRAME, composition_offset: 0, is_sync: true };
        muxer.write_sample(track, sample).map_err(mux_error)?;
    }
    Ok(())
}

fn write_alac_packet(
    muxer: &mut Mp4Writer<ReadableBufWriter>,
    track: usize,
    encoder: &mut alac::Encoder,
    packet: &[Vec<i32>],
) -> Result<(), AudioError> {
    let frames = packet.first().map_or(0, Vec::len);
    let channels: Vec<&[i32]> = packet.iter().map(Vec::as_slice).collect();
    let data = encoder.encode(&channels)?;
    let duration = u32::try_from(frames).unwrap_or(u32::MAX);
    muxer
        .write_sample(track, WriteSample { data: &data, duration, composition_offset: 0, is_sync: true })
        .map_err(mux_error)
}

/// A sample in ±1 as 16-bit, as on the disc.
fn to_i16(sample: f32) -> i32 {
    (f64::from(sample) * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i32
}

fn mux_error(error: isobmff::Error) -> AudioError {
    match error {
        isobmff::Error::Io(e) => AudioError::Io(e),
        other => AudioError::Encode(other.to_string()),
    }
}

fn part_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".part");
    path.with_file_name(name)
}

/// A buffered file the muxer can also read back, which moving the index to the front needs.
struct ReadableBufWriter(BufWriter<File>);

impl Write for ReadableBufWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl Seek for ReadableBufWriter {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(position)
    }
}

impl Read for ReadableBufWriter {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        // Buffered writes must reach the file before it is read.
        self.0.flush()?;
        self.0.get_mut().read(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decoder;

    fn tone(frames: usize, channels: usize) -> Vec<Vec<f32>> {
        let wave: Vec<f32> = (0..frames).map(|i| ((i as f32) * 0.03).sin() * 0.4).collect();
        vec![wave; channels]
    }

    fn settings(codec: Mp4Codec, channels: u16, frames: u64) -> Mp4Settings {
        Mp4Settings {
            codec,
            sample_rate: 44_100,
            channels,
            frames,
            chapters: vec![
                Mp4Chapter { start: 0, title: "Opening".into() },
                Mp4Chapter { start: 44_100, title: "The Funeral".into() },
            ],
        }
    }

    fn decode(path: &Path) -> (u16, Vec<f32>) {
        let mut decoder = Decoder::open(path).unwrap();
        let channels = decoder.channels();
        let mut samples = Vec::new();
        while let Some(block) = decoder.next_block().unwrap() {
            samples.extend(block.samples);
        }
        (channels, samples)
    }

    #[test]
    fn apple_lossless_is_sample_exact_and_carries_chapters() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.m4b");
        let frames = 100_000;
        let audio = tone(frames, 2);
        let mut writer = Mp4AudioWriter::create(&path, &settings(Mp4Codec::AppleLossless, 2, frames as u64)).unwrap();
        // Blocks that do not line up with ALAC's packets.
        writer.write(&[audio[0][..30_001].to_vec(), audio[1][..30_001].to_vec()]).unwrap();
        writer.write(&[audio[0][30_001..].to_vec(), audio[1][30_001..].to_vec()]).unwrap();
        assert!(!path.exists());
        writer.finish().unwrap();

        let (channels, decoded) = decode(&path);
        assert_eq!(channels, 2);
        assert_eq!(decoded.len(), frames * 2);
        for (i, sample) in decoded.chunks_exact(2).enumerate() {
            assert_eq!(to_i16(sample[0]), to_i16(audio[0][i]), "frame {i}");
        }
        let chapters = read_mp4_chapters(&path).unwrap();
        assert_eq!(
            chapters,
            vec![(Duration::ZERO, "Opening".to_owned()), (Duration::from_secs(1), "The Funeral".to_owned())]
        );
    }

    #[test]
    fn aac_mono_is_gapless_to_the_sample() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.m4b");
        let frames = 3 * 44_100 + 123;
        let mut writer =
            Mp4AudioWriter::create(&path, &settings(Mp4Codec::Aac { bitrate_kbps: 64 }, 1, frames as u64)).unwrap();
        writer.write(&tone(frames, 1)).unwrap();
        writer.finish().unwrap();
        let (channels, decoded) = decode(&path);
        assert_eq!(channels, 1);
        assert_eq!(decoded.len(), frames, "priming skipped and padding cut by the edit list");
        assert_eq!(read_mp4_chapters(&path).unwrap().len(), 2);
    }

    #[test]
    fn a_wrong_length_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.m4b");
        let mut writer = Mp4AudioWriter::create(&path, &settings(Mp4Codec::AppleLossless, 2, 1000)).unwrap();
        assert!(writer.write(&tone(10, 1)).is_err(), "a missing channel");
        assert!(writer.write(&tone(2000, 2)).is_err(), "more than announced");
        writer.abandon();
        let mut writer = Mp4AudioWriter::create(&path, &settings(Mp4Codec::AppleLossless, 2, 1000)).unwrap();
        writer.write(&tone(10, 2)).unwrap();
        assert!(writer.finish().is_err(), "less than announced");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
