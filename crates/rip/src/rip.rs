//! Ripping: reading tracks off a disc into tagged lossless files in the media folder.
//!
//! Each track is read as the sector range [`shifted_read`] plans for the drive's read offset,
//! so the file holds the track's audio sample for sample, whichever tracks are chosen and in
//! whatever order.

use crate::drive::{CdDrive, DriveError};
use crate::info::DiscInfo;
use crate::offset::shifted_read;
use crate::toc::DiscToc;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tunebox_audio::{LosslessFormat, PcmWriter};
use tunebox_library::NewTrack;
use tunebox_model::{TrackMeta, organize};

/// What to rip, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RipRequest {
    pub toc: DiscToc,
    /// Disc track numbers, in the order to rip them.
    pub tracks: Vec<u8>,
    pub info: DiscInfo,
    pub format: LosslessFormat,
    pub media_folder: PathBuf,
    /// The drive's read offset in samples, corrected while reading (0 reads as the drive does).
    pub read_offset: i32,
}

/// Where a rip has got to.
#[derive(Clone, Debug, PartialEq)]
pub struct RipProgress {
    /// The disc track being read.
    pub track: u8,
    pub title: String,
    /// Position among the requested tracks, from 0.
    pub index: usize,
    pub count: usize,
    /// How much of this track has been read, 0–1.
    pub fraction: f32,
}

impl RipProgress {
    /// Progress through the whole request, 0–1.
    pub fn overall(&self) -> f32 {
        if self.count == 0 { 1.0 } else { (self.index as f32 + self.fraction) / self.count as f32 }
    }
}

/// What a rip produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RipReport {
    pub tracks: Vec<NewTrack>,
    pub failures: Vec<(u8, String)>,
    pub cancelled: bool,
}

/// Rips `request.tracks` from `drive`. Each track is complete or absent: a failed or cancelled
/// track leaves no file behind.
pub fn rip(
    drive: &dyn CdDrive,
    request: &RipRequest,
    now: DateTime<Utc>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(RipProgress),
) -> RipReport {
    let mut report = RipReport::default();
    for (index, &number) in request.tracks.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let meta = request.info.track_meta(&request.toc, number);
        let mut report_progress = |fraction| {
            progress(RipProgress {
                track: number,
                title: meta.title.clone(),
                index,
                count: request.tracks.len(),
                fraction,
            })
        };
        match rip_track(drive, request, number, &meta, cancel, &mut report_progress) {
            Ok(Some(path)) => match finish_track(&path, &meta, now) {
                Ok(track) => report.tracks.push(track),
                Err(e) => report.failures.push((number, e)),
            },
            Ok(None) => report.cancelled = true,
            Err(e) => {
                log::warn!("could not rip track {number}: {e}");
                report.failures.push((number, e));
            }
        }
    }
    report
}

/// Reads one track into its file. `Ok(None)` when cancelled part-way.
fn rip_track(
    drive: &dyn CdDrive,
    request: &RipRequest,
    number: u8,
    meta: &TrackMeta,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<Option<PathBuf>, String> {
    let extension = request.format.audio_format().extension();
    let wanted = request.media_folder.join(organize::relative_path(meta, &meta.title, extension));
    let path = tunebox_import::unique_path(&wanted);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut writer = PcmWriter::create(&path, request.format, 44_100, 2).map_err(|e| e.to_string())?;
    progress(0.0);
    let written = write_track(drive, request, number, &mut writer, cancel, progress);
    match written {
        Err(e) => {
            writer.abandon();
            Err(e)
        }
        Ok(()) if cancel.load(Ordering::Relaxed) => {
            writer.abandon();
            Ok(None)
        }
        Ok(()) => writer.finish().map(Some).map_err(|e| e.to_string()),
    }
}

/// Writes track `number`, offset-corrected, to `writer`: silence where the shift reaches into
/// the lead-in, the drive's read trimmed to the track, then silence where it reaches the lead-out.
fn write_track(
    drive: &dyn CdDrive,
    request: &RipRequest,
    number: u8,
    writer: &mut PcmWriter,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<(), String> {
    let toc = &request.toc;
    let no_track = || DriveError::NoSuchTrack(number).to_string();
    let start = toc.track(number).ok_or_else(no_track)?.start_lba;
    let end = toc.end_lba(number).ok_or_else(no_track)?;
    let plan = shifted_read(start..end, 0..toc.audio_leadout(), request.read_offset);
    let total = plan.total().max(1) as f32;
    write_silence(writer, plan.silence_before)?;
    if plan.sectors > 0 {
        // Byte positions within the read of the samples to keep.
        let keep = plan.skip * 4..(plan.skip + plan.keep) * 4;
        let mut position: u64 = 0;
        let mut write_error = None;
        drive
            .read_sectors(toc, plan.first_sector, plan.sectors, &mut |chunk, _| {
                let chunk_start = position;
                position += chunk.len() as u64;
                let from = keep.start.clamp(chunk_start, position) - chunk_start;
                let to = keep.end.clamp(chunk_start, position) - chunk_start;
                let wanted = chunk.get(from as usize..to as usize).unwrap_or_default();
                if wanted.is_empty() {
                    // Nothing of the track in this chunk yet (the surplus before it).
                } else if let Err(e) = writer.write(&cd_audio_to_planar(wanted)) {
                    write_error = Some(e.to_string());
                    return false;
                }
                let done = plan.silence_before + position.saturating_sub(keep.start).min(plan.keep * 4) / 4;
                progress(done as f32 / total);
                position < keep.end && !cancel.load(Ordering::Relaxed)
            })
            .map_err(|e| e.to_string())?;
        if let Some(e) = write_error {
            return Err(e);
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
    }
    write_silence(writer, plan.silence_after)?;
    progress(1.0);
    Ok(())
}

/// Writes `samples` stereo samples of silence, a second at a time.
fn write_silence(writer: &mut PcmWriter, samples: u64) -> Result<(), String> {
    let mut left = samples;
    while left > 0 {
        let count = left.min(44_100);
        let block = vec![0.0; count as usize];
        writer.write(&[block.clone(), block]).map_err(|e| e.to_string())?;
        left -= count;
    }
    Ok(())
}

/// Tags the new file and describes it for the library.
fn finish_track(path: &Path, meta: &TrackMeta, now: DateTime<Utc>) -> Result<NewTrack, String> {
    tunebox_tags::write(path, meta).map_err(|e| e.to_string())?;
    let file = tunebox_tags::read(path).map_err(|e| e.to_string())?.file;
    Ok(NewTrack { location: path.to_path_buf(), meta: meta.clone(), file, date_added: now })
}

/// 16-bit little-endian stereo bytes to planar `f32`. A trailing partial frame is dropped.
fn cd_audio_to_planar(bytes: &[u8]) -> Vec<Vec<f32>> {
    let frames = bytes.len() / 4;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for frame in bytes.chunks_exact(4) {
        // `chunks_exact(4)` guarantees four bytes, so indexes 0..=3 are in range.
        let sample = |lo: usize| f32::from(i16::from_le_bytes([frame[lo], frame[lo + 1]])) / 32_768.0;
        left.push(sample(0));
        right.push(sample(2));
    }
    vec![left, right]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VirtualDrive;
    use tunebox_model::AudioFormat;

    fn request(drive: &VirtualDrive, media: &Path) -> RipRequest {
        let toc = drive.read_toc().unwrap();
        let mut info = DiscInfo::placeholder(&toc);
        info.album = "Test Pressing".into();
        info.artist = "Signal Path".into();
        info.tracks[0].0 = "Sine Language".into();
        RipRequest {
            tracks: vec![1, 2],
            toc,
            info,
            format: LosslessFormat::Flac,
            media_folder: media.to_path_buf(),
            read_offset: 0,
        }
    }

    #[test]
    fn rips_tagged_lossless_tracks_into_the_media_folder() {
        let media = tempfile::tempdir().unwrap();
        let drive = VirtualDrive::new("Virtual", vec![2, 1, 3]);
        let mut fractions = Vec::new();
        let report = rip(&drive, &request(&drive, media.path()), Utc::now(), &AtomicBool::new(false), &mut |p| {
            fractions.push(p.overall())
        });
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.tracks.len(), 2);
        let first = &report.tracks[0];
        assert_eq!(first.location, media.path().join("Signal Path/Test Pressing/01 Sine Language.flac"));
        assert_eq!(first.file.format, AudioFormat::Flac);
        assert_eq!(first.file.duration.as_secs(), 2);
        let tags = tunebox_tags::read(&first.location).unwrap();
        assert_eq!(tags.meta.album, "Test Pressing");
        assert_eq!(tags.meta.track.total, Some(3));
        assert!(fractions.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(fractions.last().copied(), Some(1.0));
    }

    #[test]
    fn cancelling_leaves_no_partial_files() {
        let media = tempfile::tempdir().unwrap();
        let drive = VirtualDrive::new("Virtual", vec![5]);
        let cancel = AtomicBool::new(false);
        let mut req = request(&drive, media.path());
        req.tracks = vec![1];
        let report = rip(&drive, &req, Utc::now(), &cancel, &mut |p| {
            if p.fraction > 0.3 {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert!(report.cancelled);
        assert!(report.tracks.is_empty());
        let leftovers: Vec<_> = walk(media.path());
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    /// Rips `tracks` from `drive`, correcting `correction` samples, and returns each file's
    /// samples (interleaved stereo, 16-bit) by disc track number.
    fn rip_samples(drive: &VirtualDrive, tracks: &[u8], correction: i32) -> Vec<(u8, Vec<i16>)> {
        let media = tempfile::tempdir().unwrap();
        let mut req = request(drive, media.path());
        req.tracks = tracks.to_vec();
        req.read_offset = correction;
        let report = rip(drive, &req, Utc::now(), &AtomicBool::new(false), &mut |_| {});
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        tracks.iter().copied().zip(report.tracks.iter().map(|t| decode(&t.location))).collect()
    }

    fn decode(path: &Path) -> Vec<i16> {
        let mut decoder = tunebox_audio::Decoder::open(path).unwrap();
        let mut samples = Vec::new();
        while let Some(block) = decoder.next_block().unwrap() {
            samples.extend(block.samples.iter().map(|s| (s * 32_768.0).round() as i16));
        }
        samples
    }

    /// Track `number`'s true audio, as an exact drive reads it.
    fn true_track(drive: &VirtualDrive, number: u8) -> Vec<i16> {
        let toc = drive.read_toc().unwrap();
        let start = i64::from(toc.track(number).unwrap().start_lba) * 588;
        let length = toc.sectors(number).unwrap() as usize * 588;
        drive.true_audio(start, length).chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
    }

    #[test]
    fn corrects_a_drive_that_reads_early() {
        // Track 1 starts the disc, as on most real CDs; tracks are 1 s, 1 s and 2 s.
        let exact = VirtualDrive::new("Virtual", vec![1, 1, 2]).with_pregap(0);
        let drive = exact.clone().with_read_offset(667);
        // A partial selection, out of order.
        for (number, samples) in rip_samples(&drive, &[3, 1], 667) {
            let truth = true_track(&exact, number);
            assert_eq!(samples.len(), truth.len(), "track {number}");
            if number == 3 {
                // The last 667 samples lie in the lead-out, which the drive cannot read: silence.
                let (audio, padding) = samples.split_at(samples.len() - 667 * 2);
                assert_eq!(audio, &truth[..audio.len()]);
                assert!(padding.iter().all(|s| *s == 0));
                assert!(truth[audio.len()..].iter().any(|s| *s != 0), "the padding replaces real audio");
            } else {
                assert_eq!(samples, truth, "track {number}");
            }
        }
        // Without the correction the files are shifted.
        for (number, samples) in rip_samples(&drive, &[1, 3], 0) {
            assert_ne!(samples, true_track(&exact, number), "track {number}");
        }
    }

    #[test]
    fn corrects_a_drive_that_reads_late() {
        let exact = VirtualDrive::new("Virtual", vec![1, 1, 1]).with_pregap(0);
        let drive = exact.clone().with_read_offset(-1164);
        for (number, samples) in rip_samples(&drive, &[2, 1, 3], -1164) {
            let truth = true_track(&exact, number);
            assert_eq!(samples.len(), truth.len(), "track {number}");
            if number == 1 {
                // The first 1164 samples would come from the lead-in: silence.
                let (padding, audio) = samples.split_at(1164 * 2);
                assert!(padding.iter().all(|s| *s == 0));
                assert_eq!(audio, &truth[1164 * 2..]);
            } else {
                assert_eq!(samples, truth, "track {number}");
            }
        }
        for (number, samples) in rip_samples(&drive, &[2], 0) {
            assert_ne!(samples, true_track(&exact, number), "track {number}");
        }
    }

    #[test]
    fn a_readable_pregap_needs_no_padding() {
        // With the usual 2-second pregap, reading late into it is still real (silent) audio.
        let exact = VirtualDrive::new("Virtual", vec![1, 1]);
        let drive = exact.clone().with_read_offset(-1164);
        for (number, samples) in rip_samples(&drive, &[1, 2], -1164) {
            assert_eq!(samples, true_track(&exact, number), "track {number}");
        }
    }

    #[test]
    fn an_exact_drive_with_no_correction_rips_the_true_audio() {
        let drive = VirtualDrive::new("Virtual", vec![1, 2]);
        for (number, samples) in rip_samples(&drive, &[2, 1], 0) {
            assert_eq!(samples, true_track(&drive, number), "track {number}");
        }
    }

    #[test]
    fn converts_cd_samples() {
        let bytes = [0x00, 0x80, 0xFF, 0x7F, 0x01];
        let planar = cd_audio_to_planar(&bytes);
        assert_eq!(planar, vec![vec![-1.0], vec![32_767.0 / 32_768.0]]);
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            if entry.path().is_dir() {
                files.extend(walk(&entry.path()));
            } else {
                files.push(entry.path());
            }
        }
        files
    }
}
