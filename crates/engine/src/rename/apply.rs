//! Carrying out a rename, on a worker thread: each file is moved (never over another, never to
//! another drive, never in a read-only folder; see `files::move_file`), the files that go with
//! it follow, and folders the moves left empty are removed.
//!
//! Each move is written to a shared log the moment it succeeds, so that the session can point
//! the library at where every file really is, however the job ends: finished, cancelled, or
//! stopped by a panic.

use crate::files::{self, MoveError};
use crate::jobs::JobContext;
use crate::prefs::Preferences;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Mutex, PoisonError};
use tunebox_model::TrackId;

/// Files that belong to a song when they share its file name: lyrics, cue sheets, notes.
/// Other audio files with the same name are other songs, so they never follow.
const SIDECAR_EXTENSIONS: [&str; 5] = ["lrc", "elrc", "txt", "cue", "nfo"];

/// One song's file to move.
#[derive(Clone, Debug)]
pub(crate) struct Move {
    pub id: TrackId,
    pub from: PathBuf,
    pub to: PathBuf,
}

/// What the job did, beside the moves in the log.
#[derive(Debug, Default)]
pub(crate) struct ApplyReport {
    /// Files that could not be renamed, and why. They are where they were.
    pub failures: Vec<(PathBuf, String)>,
    /// Lyrics and other files that followed their song.
    pub sidecars_moved: usize,
    /// Those that stayed, because their new name was taken or the move failed.
    pub sidecars_left: usize,
    pub folders_removed: usize,
    pub cancelled: bool,
}

/// Moves the files, logging each move in `log` as it happens. `roots` are the library folders
/// and the media folder: only folders inside them are ever removed, and never they themselves.
pub(crate) fn apply(
    moves: &[Move],
    prefs: &Preferences,
    roots: &[PathBuf],
    log: &Mutex<Vec<Move>>,
    ctx: &JobContext,
) -> ApplyReport {
    let mut report = ApplyReport::default();
    // Folders that may be empty now: where files left, and where a failed move made folders.
    let mut maybe_empty = BTreeSet::new();
    for (done, song) in moves.iter().enumerate() {
        if ctx.cancel_flag().load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let name = song.from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        ctx.progress(done as f32 / moves.len().max(1) as f32, name);
        // Listed before the song moves, while they still share its name.
        let sidecars = sidecars_of(&song.from);
        match files::move_file(prefs, &song.from, &song.to) {
            Ok(()) => {
                log.lock().unwrap_or_else(PoisonError::into_inner).push(song.clone());
                for sidecar in sidecars {
                    match files::move_file(prefs, &sidecar, &sidecar_target(&sidecar, &song.to)) {
                        Ok(()) => report.sidecars_moved += 1,
                        Err(_) => report.sidecars_left += 1,
                    }
                }
                maybe_empty.extend(song.from.parent().map(Path::to_path_buf));
            }
            Err(error) => {
                report.failures.push((song.from.clone(), error.to_string()));
                if error != MoveError::Protected {
                    maybe_empty.extend(song.to.parent().map(Path::to_path_buf));
                }
            }
        }
    }
    // Deepest first, so a parent is looked at after its emptied children.
    for folder in maybe_empty.iter().rev() {
        report.folders_removed += files::remove_empty_folder(prefs, folder, roots);
    }
    report
}

/// The files beside `song` with its name and a sidecar extension: `01 Song.lrc` for
/// `01 Song.flac`.
fn sidecars_of(song: &Path) -> Vec<PathBuf> {
    let (Some(folder), Some(stem)) = (song.parent(), song.file_stem()) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path != song && path.file_stem() == Some(stem))
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| SIDECAR_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .filter(|path| path.is_file())
        .collect()
}

/// Where a sidecar goes: beside the song's new file, with its new name and its own extension.
fn sidecar_target(sidecar: &Path, song_to: &Path) -> PathBuf {
    match sidecar.extension() {
        Some(extension) => song_to.with_extension(extension),
        None => song_to.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecars_share_the_name_and_are_not_audio() {
        let dir = tempfile::tempdir().unwrap();
        let song = dir.path().join("01 Song.flac");
        for name in ["01 Song.flac", "01 Song.lrc", "01 Song.CUE", "01 Song.mp3", "01 Song 2.lrc", "cover.jpg"] {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        let mut found: Vec<_> =
            sidecars_of(&song).into_iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        found.sort();
        assert_eq!(found, ["01 Song.CUE", "01 Song.lrc"]);
        assert_eq!(
            sidecar_target(&dir.path().join("01 Song.CUE"), Path::new("/m/A/03 Vol. 2.flac")),
            PathBuf::from("/m/A/03 Vol. 2.CUE")
        );
    }
}
