//! Ripping a disc of an audiobook into its staging folder: lossless FLAC, untagged and not in
//! the library, with each track's pauses measured on the way.
//!
//! The staged files are the book's raw material: the build reads them back, joins them into
//! chapters and encodes them. Each file is complete or absent, as with an ordinary rip.

use crate::drive::CdDrive;
use crate::rip::read_track;
use crate::toc::DiscToc;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tunebox_audio::{LosslessFormat, LoudnessMeter, PcmWriter};

/// What to stage, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageRequest {
    pub toc: DiscToc,
    /// Disc track numbers, in the order to rip them.
    pub tracks: Vec<u8>,
    /// The disc's staging folder.
    pub folder: PathBuf,
    /// The drive's read offset in samples.
    pub read_offset: i32,
}

/// A track ripped into the staging folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedTrack {
    pub number: u8,
    pub path: PathBuf,
    /// Its quiet stretches, in samples from its start.
    pub pauses: Vec<Range<u64>>,
}

/// Where staging has got to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StageProgress {
    /// The disc track being read.
    pub track: u8,
    /// Position among the requested tracks, from 0.
    pub index: usize,
    pub count: usize,
    /// How much of this track has been read, 0–1.
    pub fraction: f32,
}

impl StageProgress {
    /// Progress through the whole disc, 0–1.
    pub fn overall(&self) -> f32 {
        if self.count == 0 { 1.0 } else { (self.index as f32 + self.fraction) / self.count as f32 }
    }
}

/// What staging produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StageReport {
    pub staged: Vec<StagedTrack>,
    pub failures: Vec<(u8, String)>,
    pub cancelled: bool,
}

/// The staged file of track `number` in a disc's folder.
pub fn staged_path(folder: &Path, number: u8) -> PathBuf {
    folder.join(format!("track-{number:02}.flac"))
}

/// Rips `request.tracks` into `request.folder`, replacing any earlier staged file of the same
/// track. A failed or cancelled track leaves no file behind.
pub fn stage(
    drive: &dyn CdDrive,
    request: &StageRequest,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(StageProgress),
) -> StageReport {
    let mut report = StageReport::default();
    if let Err(e) = std::fs::create_dir_all(&request.folder) {
        report.failures = request.tracks.iter().map(|n| (*n, e.to_string())).collect();
        return report;
    }
    for (index, &number) in request.tracks.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let count = request.tracks.len();
        let mut track_progress = |fraction| progress(StageProgress { track: number, index, count, fraction });
        match stage_track(drive, request, number, cancel, &mut track_progress) {
            Ok(Some(staged)) => report.staged.push(staged),
            Ok(None) => report.cancelled = true,
            Err(e) => {
                log::warn!("could not stage track {number}: {e}");
                report.failures.push((number, e));
            }
        }
    }
    report
}

/// Rips one track. `Ok(None)` when cancelled part-way.
fn stage_track(
    drive: &dyn CdDrive,
    request: &StageRequest,
    number: u8,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<Option<StagedTrack>, String> {
    let path = staged_path(&request.folder, number);
    let mut writer = PcmWriter::create(&path, LosslessFormat::Flac, 44_100, 2).map_err(|e| e.to_string())?;
    let mut meter = LoudnessMeter::new(44_100);
    progress(0.0);
    let mut sink = |block: &[Vec<f32>]| {
        meter.push(block);
        writer.write(block).map_err(|e| e.to_string())
    };
    let read = read_track(drive, &request.toc, number, request.read_offset, cancel, progress, &mut sink);
    match read {
        Err(e) => {
            writer.abandon();
            Err(e)
        }
        Ok(()) if cancel.load(Ordering::Relaxed) => {
            writer.abandon();
            Ok(None)
        }
        Ok(()) => {
            let path = writer.finish().map_err(|e| e.to_string())?;
            Ok(Some(StagedTrack { number, path, pauses: meter.pauses() }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VirtualDrive;

    fn request(drive: &VirtualDrive, folder: &Path, tracks: Vec<u8>) -> StageRequest {
        StageRequest { toc: drive.read_toc().unwrap(), tracks, folder: folder.to_path_buf(), read_offset: 0 }
    }

    #[test]
    fn stages_untagged_flac_with_the_tracks_exact_length() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("disc-01");
        let drive = VirtualDrive::new("Virtual", vec![2, 1]);
        let mut fractions = Vec::new();
        let report = stage(&drive, &request(&drive, &folder, vec![1, 2]), &AtomicBool::new(false), &mut |p| {
            fractions.push(p.overall())
        });
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(
            report.staged.iter().map(|t| t.path.clone()).collect::<Vec<_>>(),
            [folder.join("track-01.flac"), folder.join("track-02.flac")]
        );
        let mut decoder = tunebox_audio::Decoder::open(&report.staged[0].path).unwrap();
        let mut frames = 0;
        while let Some(block) = decoder.next_block().unwrap() {
            frames += block.frames();
        }
        assert_eq!(frames, 2 * 44_100);
        assert!(tunebox_tags::read(&report.staged[0].path).unwrap().meta.title.is_empty(), "no tags");
        assert_eq!(fractions.last().copied(), Some(1.0));
    }

    #[test]
    fn a_spoken_disc_has_pauses_and_ends_with_an_announcement() {
        let dir = tempfile::tempdir().unwrap();
        let drive = VirtualDrive::new("Virtual", vec![15, 20]).spoken();
        let report = stage(&drive, &request(&drive, dir.path(), vec![1, 2]), &AtomicBool::new(false), &mut |_| {});
        let seconds = |r: &Range<u64>| (r.start as f64 / 44_100.0, r.end as f64 / 44_100.0);
        let first: Vec<_> = report.staged[0].pauses.iter().map(seconds).collect();
        assert!(first.len() >= 2, "pauses between sentences: {first:?}");
        // The last track: a long pause before the announcement, 7.4 s from the end.
        let last: Vec<_> = report.staged[1].pauses.iter().map(seconds).collect();
        assert!(last.iter().any(|(a, b)| (a - 12.6).abs() < 0.1 && (b - 14.2).abs() < 0.1), "{last:?}");
    }

    #[test]
    fn cancelling_leaves_no_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let drive = VirtualDrive::new("Virtual", vec![4]);
        let cancel = AtomicBool::new(false);
        let report = stage(&drive, &request(&drive, dir.path(), vec![1]), &cancel, &mut |p| {
            if p.fraction > 0.3 {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert!(report.cancelled);
        assert!(report.staged.is_empty());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_missing_track_is_a_failure_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let drive = VirtualDrive::new("Virtual", vec![1]);
        let report = stage(&drive, &request(&drive, dir.path(), vec![7]), &AtomicBool::new(false), &mut |_| {});
        assert_eq!(report.failures.len(), 1);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
