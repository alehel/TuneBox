//! tunebox-import — "Add to Library…".
//!
//! [`find_audio_files`] walks folders for audio files; [`import`] reads each file's tags and,
//! when asked, copies it into the media folder at its organised place
//! (`Artist/Album/NN Title.ext`). The result is a list of [`NewTrack`]s for the library to add:
//! this crate never touches the library itself, so it can run on a worker thread while the
//! library stays with its single owner.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod files;
mod scan;

pub use files::{copy_into_media_folder, unique_path};
pub use scan::{MAX_DEPTH, find_audio_files, is_audio_file};

use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tunebox_library::NewTrack;

/// How imported files are treated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportOptions {
    /// Where organised copies go.
    pub media_folder: PathBuf,
    /// "Copy files to the media folder when adding to library". When off, tracks point at the
    /// files where they are.
    pub copy_to_media_folder: bool,
}

/// How far an import has got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportProgress {
    pub done: usize,
    pub total: usize,
    pub current: PathBuf,
}

/// What an import produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Tracks ready to add, in the order the files were found.
    pub tracks: Vec<NewTrack>,
    /// Files skipped because the library already has them.
    pub already_in_library: usize,
    /// Files that could not be imported, with the reason.
    pub failures: Vec<(PathBuf, String)>,
    pub cancelled: bool,
}

/// Imports `files`. `known` holds the locations already in the library, which are skipped.
/// `progress` is called before each file; setting `cancel` stops after the current file.
pub fn import(
    files: &[PathBuf],
    known: &HashSet<PathBuf>,
    options: &ImportOptions,
    now: DateTime<Utc>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(ImportProgress),
) -> ImportReport {
    let mut report = ImportReport::default();
    for (done, path) in files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        progress(ImportProgress { done, total: files.len(), current: path.clone() });
        if known.contains(path) {
            report.already_in_library += 1;
            continue;
        }
        match import_one(path, options, now) {
            Ok(track) if known.contains(&track.location) => report.already_in_library += 1,
            Ok(track) => report.tracks.push(track),
            Err(reason) => {
                log::warn!("not importing {}: {reason}", path.display());
                report.failures.push((path.clone(), reason));
            }
        }
    }
    report
}

/// Reads one file and, if asked, files a copy in the media folder.
fn import_one(path: &Path, options: &ImportOptions, now: DateTime<Utc>) -> Result<NewTrack, String> {
    let tags = tunebox_tags::read(path).map_err(|e| e.to_string())?;
    let location = if options.copy_to_media_folder && !path.starts_with(&options.media_folder) {
        copy_into_media_folder(path, &tags.meta, tags.file.format, &options.media_folder).map_err(|e| e.to_string())?
    } else {
        path.to_path_buf()
    };
    Ok(NewTrack { location, meta: tags.meta, file: tags.file, date_added: now })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunebox_audio::{LosslessFormat, PcmWriter};
    use tunebox_model::{NumberOf, TrackMeta};

    fn write_track(path: &Path, title: &str, track: u32) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut writer = PcmWriter::create(path, LosslessFormat::Flac, 44_100, 2).unwrap();
        writer.write(&[vec![0.0; 4410], vec![0.0; 4410]]).unwrap();
        writer.finish().unwrap();
        let meta = TrackMeta {
            title: title.into(),
            artist: "Night Ferry".into(),
            album: "Harbour".into(),
            track: NumberOf::new(track, 2),
            ..TrackMeta::default()
        };
        tunebox_tags::write(path, &meta).unwrap();
    }

    #[test]
    fn imports_and_organises_copies() {
        let source = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        write_track(&source.path().join("a/one.flac"), "Departure", 1);
        write_track(&source.path().join("a/b/two.flac"), "Arrival", 2);
        std::fs::write(source.path().join("a/notes.txt"), "not audio").unwrap();

        let files = find_audio_files(&[source.path().to_path_buf()], &AtomicBool::new(false));
        assert_eq!(files.len(), 2);
        let options = ImportOptions { media_folder: media.path().to_path_buf(), copy_to_media_folder: true };
        let mut calls = 0;
        let report =
            import(&files, &HashSet::new(), &options, Utc::now(), &AtomicBool::new(false), &mut |_| calls += 1);
        assert_eq!(calls, 2);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let mut locations: Vec<_> = report.tracks.iter().map(|t| t.location.clone()).collect();
        locations.sort();
        assert_eq!(
            locations,
            [
                media.path().join("Night Ferry/Harbour/01 Departure.flac"),
                media.path().join("Night Ferry/Harbour/02 Arrival.flac")
            ]
        );
        assert!(locations.iter().all(|p| p.exists()));
    }

    #[test]
    fn skips_known_files_and_reports_bad_ones() {
        let source = tempfile::tempdir().unwrap();
        let good = source.path().join("good.flac");
        write_track(&good, "Good", 1);
        let bad = source.path().join("bad.mp3");
        std::fs::write(&bad, b"garbage").unwrap();
        let options = ImportOptions { media_folder: source.path().join("media"), copy_to_media_folder: false };
        let known: HashSet<PathBuf> = [good.clone()].into();
        let report = import(&[good, bad.clone()], &known, &options, Utc::now(), &AtomicBool::new(false), &mut |_| {});
        assert_eq!(report.already_in_library, 1);
        assert!(report.tracks.is_empty());
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].0, bad);
    }

    #[test]
    fn cancel_stops_the_import() {
        let options = ImportOptions { media_folder: PathBuf::from("/nowhere"), copy_to_media_folder: false };
        let report = import(
            &[PathBuf::from("/x.mp3")],
            &HashSet::new(),
            &options,
            Utc::now(),
            &AtomicBool::new(true),
            &mut |_| {},
        );
        assert!(report.cancelled);
        assert!(report.failures.is_empty());
    }
}
