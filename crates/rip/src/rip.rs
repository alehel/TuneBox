//! Ripping: reading tracks off a disc into tagged lossless files in the media folder.

use crate::drive::CdDrive;
use crate::info::DiscInfo;
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
            progress(RipProgress { track: number, title: meta.title.clone(), index, count: request.tracks.len(), fraction })
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
    let mut write_error = None;
    progress(0.0);
    let read = drive.read_track(&request.toc, number, &mut |chunk, fraction| {
        if let Err(e) = writer.write(&cd_audio_to_planar(chunk)) {
            write_error = Some(e.to_string());
            return false;
        }
        progress(fraction);
        !cancel.load(Ordering::Relaxed)
    });
    if let Err(e) = read {
        writer.abandon();
        return Err(e.to_string());
    }
    if let Some(e) = write_error {
        writer.abandon();
        return Err(e);
    }
    if cancel.load(Ordering::Relaxed) {
        writer.abandon();
        return Ok(None);
    }
    writer.finish().map(Some).map_err(|e| e.to_string())
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
        RipRequest { tracks: vec![1, 2], toc, info, format: LosslessFormat::Flac, media_folder: media.to_path_buf() }
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
