//! Rename Files from Tags: renaming, and optionally moving into folders, the files of chosen
//! songs, with names built from their tags by a pattern (`tunebox_model::naming`).
//!
//! It always goes preview first ([`Session::preview_rename`], or [`LibraryView::preview_rename`]
//! on a snapshot), then [`Session::rename_files`] with that preview. The rename is planned again
//! and refused if anything changed, so what happens is exactly what the listener saw. Songs in
//! conflict are renamed never, and only skipped when the listener says so.
//!
//! The moves run as a job (`apply`). When it ends, however it ends, the library is pointed at
//! where each file really is; the songs keep their ratings, plays, playlists and edits. While it
//! runs, the folder watcher, imports and refreshes wait, so the renamed files are never taken
//! for new songs.

mod apply;
mod plan;

pub use plan::{Conflict, Planned, RenameBase, RenameItem, RenamePlan, RenameRequest, SkipReason};

use crate::EngineError;
use crate::file_check::FileStatus;
use crate::jobs::{JobId, JobKind};
use crate::notices::Notice;
use crate::prefs::Preferences;
use crate::session::{Outcome, Session};
use crate::view::LibraryView;
use apply::{ApplyReport, Move};
use plan::{Disk, Folders, Occupant, Song};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tunebox_model::TrackId;
use tunebox_model::naming::NamePattern;
use tunebox_model::organize::CaseRule;

/// How long quitting waits for a rename to stop between two files.
const QUIT_WAIT: Duration = Duration::from_secs(10);

/// What to do with songs whose new name is taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameConflicts {
    /// Rename nothing while any remain.
    Refuse,
    /// Rename the others and leave those as they are.
    Skip,
}

/// A rename in progress, as the session keeps it.
pub(crate) struct RenameRun {
    /// The moves made so far, written by the job as each succeeds.
    log: Arc<Mutex<Vec<Move>>>,
    /// The songs being moved: their details are not edited meanwhile.
    pub(crate) tracks: HashSet<TrackId>,
    unchanged: usize,
    /// Songs left as they were, and why.
    left_alone: Vec<(String, String)>,
}

/// What the job hands back, beside the log.
#[derive(Debug)]
pub(crate) struct RenameReport(ApplyReport);

/// The disk as planning sees it.
struct LiveDisk<'a> {
    prefs: &'a Preferences,
}

impl Disk for LiveDisk<'_> {
    fn writable(&self, path: &Path) -> bool {
        crate::files::may_write(self.prefs, path)
    }

    fn occupant(&self, from: &Path, to: &Path) -> Occupant {
        match std::fs::symlink_metadata(to) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Occupant::Nothing,
            // Cannot tell: as good as taken.
            Err(_) => Occupant::Other,
            Ok(target) if is_same_file(from, to, &target) => Occupant::Itself,
            Ok(_) => Occupant::Other,
        }
    }
}

/// Whether `to`, which exists, is the file at `from` under another name, as on a disk that
/// ignores case.
#[cfg(unix)]
fn is_same_file(from: &Path, _to: &Path, target: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(from).is_ok_and(|source| source.dev() == target.dev() && source.ino() == target.ino())
}

/// Windows has no stable file identity in std; there a name differing only in case is the file
/// itself. Should it not be, the move refuses to replace it anyway.
#[cfg(not(unix))]
fn is_same_file(from: &Path, to: &Path, _target: &std::fs::Metadata) -> bool {
    tunebox_model::organize::differ_only_in_case(from, to)
}

impl LibraryView<'_> {
    /// What renaming would do, without changing anything. `status` says whether each song's
    /// file can be found; songs whose drive is not connected are not looked at.
    pub fn preview_rename(
        &self,
        request: &RenameRequest,
        status: impl Fn(TrackId) -> FileStatus,
    ) -> Result<RenamePlan, EngineError> {
        let pattern = NamePattern::parse(&request.pattern)?;
        let mut seen = HashSet::new();
        let songs: Vec<Song<'_>> = request
            .tracks
            .iter()
            .filter(|id| seen.insert(**id))
            .filter_map(|id| self.library.track(*id))
            .map(|t| Song { id: t.id, location: &t.location, meta: &t.meta, status: status(t.id) })
            .collect();
        let roots = roots(self.prefs);
        let folders = Folders { roots: &roots, media_folder: &self.prefs.media_folder };
        let disk = LiveDisk { prefs: self.prefs };
        let items = plan::plan(
            &songs,
            &pattern,
            request.base,
            &folders,
            CaseRule::native(),
            |path| self.library.track_at(path),
            &disk,
        );
        Ok(RenamePlan { request: request.clone(), items })
    }
}

/// The folders renamed files may be filed in, and the only ones emptied folders are removed
/// from: the library folders and the media folder.
fn roots(prefs: &Preferences) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = prefs.library_folders.iter().map(|f| f.path.clone()).collect();
    roots.push(prefs.media_folder.clone());
    roots
}

impl Session {
    /// What renaming would do, without changing anything.
    pub fn preview_rename(&self, request: &RenameRequest) -> Result<RenamePlan, EngineError> {
        self.view().preview_rename(request, |id| self.file_status(id).clone())
    }

    /// Renames the files as `preview` showed, in the background. Refused if anything changed
    /// since the preview, or if songs are in conflict and `conflicts` says to refuse.
    pub fn rename_files(&mut self, preview: &RenamePlan, conflicts: RenameConflicts) -> Result<JobId, EngineError> {
        for kind in [JobKind::Rename, JobKind::Import, JobKind::Refresh, JobKind::Artwork] {
            if self.jobs.is_running(kind) {
                return Err(EngineError::Busy(kind.noun()));
            }
        }
        let plan = self.preview_rename(&preview.request)?;
        if plan != *preview {
            return Err(EngineError::Rename(
                "The songs or their files changed since the preview. Look at the preview again, then rename.".into(),
            ));
        }
        let in_conflict = plan.conflicts();
        if in_conflict > 0 && conflicts == RenameConflicts::Refuse {
            let what = if in_conflict == 1 {
                "1 song would get a name".to_owned()
            } else {
                format!("{in_conflict} songs would get names")
            };
            return Err(EngineError::Rename(format!(
                "{what} already taken. Change the pattern, or skip the songs in conflict."
            )));
        }
        let moves: Vec<Move> =
            plan.renames().map(|(id, from, to)| Move { id, from: from.to_path_buf(), to: to.to_path_buf() }).collect();
        let left_alone = plan
            .items
            .iter()
            .filter_map(|item| {
                let reason = match &item.planned {
                    Planned::Skipped(reason) => reason.text(),
                    Planned::Conflict { reason, .. } => reason.text(),
                    Planned::Rename { .. } | Planned::Unchanged => return None,
                };
                let title = self.library.track(item.id).map(|t| t.title().to_owned()).unwrap_or_default();
                Some((title, reason))
            })
            .collect();
        let log = Arc::new(Mutex::new(Vec::new()));
        self.renaming = Some(RenameRun {
            log: Arc::clone(&log),
            tracks: moves.iter().map(|m| m.id).collect(),
            unchanged: plan.unchanged(),
            left_alone,
        });
        let prefs = self.prefs.clone();
        let roots = roots(&prefs);
        let title = match moves.len() {
            1 => "Renaming 1 file".to_owned(),
            n => format!("Renaming {n} files"),
        };
        Ok(self.jobs.spawn(JobKind::Rename, title, move |ctx| {
            Outcome::Renamed(RenameReport(apply::apply(&moves, &prefs, &roots, &log, ctx)))
        }))
    }

    /// A cancelled rename stops between two files. Waits for that (within a limit), so the
    /// library saved on quitting records every move made, not where the files were.
    pub(crate) fn finish_rename_before_quitting(&mut self) {
        let deadline = Instant::now() + QUIT_WAIT;
        while self.renaming.is_some() && Instant::now() < deadline {
            for finished in self.jobs.poll() {
                match finished.outcome {
                    Ok(Outcome::Renamed(report)) => self.finish_rename(Some(report)),
                    Err(_) if finished.kind == JobKind::Rename => self.finish_rename(None),
                    // Other work was cancelled too, and is not taken in on the way out.
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        // Still moving a file on a slow disk: record the moves made so far.
        self.finish_rename(None);
    }

    /// Points the library at where each file went. `report` is `None` when the job stopped
    /// unexpectedly; the moves it made are in the log all the same.
    pub(crate) fn finish_rename(&mut self, report: Option<RenameReport>) {
        let Some(run) = self.renaming.take() else { return };
        let moved = std::mem::take(&mut *run.log.lock().unwrap_or_else(PoisonError::into_inner));
        let mut relinked = Vec::new();
        for song in moved {
            let still_there = self.library.track(song.id).is_some_and(|t| t.location == song.from);
            if still_there && self.library.relocate(song.id, song.to).is_ok() {
                self.availability.forget(song.id);
                relinked.push(song.id);
            }
        }
        if !relinked.is_empty() {
            self.library_changed();
            // A look at the old place may still be on its way; this one comes after it.
            self.check_tracks(&relinked);
        }
        let report = report.map(|r| r.0).unwrap_or_default();
        self.notices.push(Notice::info(summary(relinked.len(), run.unchanged, &report)));
        if report.cancelled {
            self.notices.push(Notice::info("Renaming was stopped. The files renamed until then are in the library."));
        }
        if let Some((first, reason)) = run.left_alone.first() {
            let text = match run.left_alone.len() {
                1 => format!("“{first}” was left as it is: {reason}."),
                n => format!("{n} songs were left as they are. The first, “{first}”: {reason}."),
            };
            self.notices.push(Notice::info(text));
        }
        if let Some((path, reason)) = report.failures.first() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let text = match report.failures.len() {
                1 => format!("“{name}” could not be renamed, so it is where it was: {reason}."),
                n => format!("{n} files could not be renamed and are where they were. The first, “{name}”: {reason}."),
            };
            self.notices.push(Notice::warning(text));
        }
        if report.sidecars_left > 0 {
            self.notices.push(Notice::warning(format!(
                "{} lyrics or other files that go with the songs stayed in the old folder: their new name was taken, or they could not be moved.",
                report.sidecars_left
            )));
        }
    }
}

/// The notice after a rename.
fn summary(renamed: usize, unchanged: usize, report: &ApplyReport) -> String {
    let mut text = match renamed {
        0 => "No files were renamed.".to_owned(),
        1 => "Renamed 1 file.".to_owned(),
        n => format!("Renamed {n} files."),
    };
    match unchanged {
        0 => {}
        1 => text.push_str(" 1 was already named so."),
        n => text.push_str(&format!(" {n} were already named so.")),
    }
    match report.sidecars_moved {
        0 => {}
        1 => text.push_str(" 1 lyrics or cue file went with its song."),
        n => text.push_str(&format!(" {n} lyrics and cue files went with their songs.")),
    }
    match report.folders_removed {
        0 => {}
        1 => text.push_str(" Removed 1 folder left empty."),
        n => text.push_str(&format!(" Removed {n} folders left empty.")),
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_says_what_happened() {
        let report = ApplyReport { sidecars_moved: 2, folders_removed: 1, ..ApplyReport::default() };
        assert_eq!(
            summary(12, 3, &report),
            "Renamed 12 files. 3 were already named so. 2 lyrics and cue files went with their songs. Removed 1 folder left empty."
        );
        assert_eq!(summary(0, 0, &ApplyReport::default()), "No files were renamed.");
    }
}
