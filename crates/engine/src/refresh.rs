//! Refresh from Files: reads the tags of chosen songs again, for when another program changed
//! them. The reading runs as a job; the library takes the results in `Session::tick`.
//!
//! A refresh replaces what the library says with what the file says. For songs whose edits
//! were kept in TuneBox only (a read-only library folder, or a write that failed) that would
//! undo those edits, so the caller says whether to keep them ([`LibraryOnlyEdits`]).

use crate::EngineError;
use crate::file_check::FileStatus;
use crate::jobs::{JobId, JobKind};
use crate::notices::Notice;
use crate::session::{Origin, Outcome, Session};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tunebox_model::{TrackId, TrackMeta};
use tunebox_tags::ReadTags;

/// What a refresh does with songs that have edits kept in TuneBox only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryOnlyEdits {
    /// Leave those songs as they are.
    Keep,
    /// Refresh them too, replacing the edits with what their files say.
    Replace,
}

/// One song's file, read again.
#[derive(Debug)]
pub(crate) struct ReadAgain {
    id: TrackId,
    location: PathBuf,
    /// The song's details when the refresh started: if they changed while it ran, the
    /// listener edited the song meanwhile and that edit is newer than what was read.
    before: TrackMeta,
    result: Result<ReadTags, String>,
}

/// What a refresh found, for `Session::finish_refresh`.
#[derive(Debug)]
pub(crate) struct RefreshReport {
    files: Vec<ReadAgain>,
    /// Songs left out because their library-only edits were kept.
    kept: usize,
    /// Songs left out because their drive or share is not connected.
    unavailable: usize,
}

impl Session {
    /// How many of `ids` have edits kept in TuneBox only, which a refresh would replace.
    pub fn library_only_edits(&self, ids: &[TrackId]) -> usize {
        ids.iter().filter_map(|id| self.library.track(*id)).filter(|t| t.library_only_edits).count()
    }

    /// Reads the tags of `ids` from their files again and updates the library with them.
    /// Ratings, play counts and playlists are TuneBox's own and are kept. Nothing is written.
    pub fn refresh_from_files(&mut self, ids: &[TrackId], edits: LibraryOnlyEdits) -> Result<JobId, EngineError> {
        self.start_refresh(ids, edits, Origin::Listener)
    }

    pub(crate) fn start_refresh(
        &mut self,
        ids: &[TrackId],
        edits: LibraryOnlyEdits,
        origin: Origin,
    ) -> Result<JobId, EngineError> {
        // While files are renamed, reading them would find them gone.
        for kind in [JobKind::Refresh, JobKind::Rename] {
            if self.jobs.is_running(kind) {
                return Err(EngineError::Busy(kind.noun()));
            }
        }
        let all: Vec<_> = ids.iter().filter_map(|id| self.library.track(*id)).collect();
        // Reading from a share that is not connected could stall the refresh; those wait.
        let (tracks, offline): (Vec<_>, Vec<_>) =
            all.into_iter().partition(|t| !matches!(self.file_status(t.id), FileStatus::Unavailable(_)));
        let unavailable = offline.len();
        let songs: Vec<(TrackId, PathBuf, TrackMeta)> = tracks
            .iter()
            .filter(|t| edits == LibraryOnlyEdits::Replace || !t.library_only_edits)
            .map(|t| (t.id, t.location.clone(), t.meta.clone()))
            .collect();
        let kept = tracks.len().saturating_sub(songs.len());
        let title = match songs.len() {
            1 => "Refreshing 1 song from its file".to_owned(),
            n => format!("Refreshing {n} songs from their files"),
        };
        Ok(self.jobs.spawn(JobKind::Refresh, title, move |ctx| {
            let total = songs.len();
            let mut files = Vec::with_capacity(total);
            for (done, (id, location, before)) in songs.into_iter().enumerate() {
                if ctx.cancel_flag().load(Ordering::Relaxed) {
                    break;
                }
                let name = location.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                ctx.progress(done as f32 / total.max(1) as f32, name);
                let result = tunebox_tags::read(&location).map_err(|e| e.to_string());
                files.push(ReadAgain { id, location, before, result });
            }
            Outcome::Refreshed(RefreshReport { files, kept, unavailable }, origin)
        }))
    }

    pub(crate) fn finish_refresh(&mut self, report: RefreshReport, origin: Origin) {
        let (mut refreshed, mut changed) = (0, 0);
        let mut failures: Vec<(PathBuf, String)> = Vec::new();
        let mut unreadable = Vec::new();
        for file in report.files {
            let Some(track) = self.library.track(file.id) else { continue };
            if track.location != file.location || track.meta != file.before {
                // Moved or edited while the refresh ran: the library's version is newer.
                continue;
            }
            match file.result {
                Ok(tags) => {
                    // Only the details count as a change worth reporting; file facts such as
                    // the size are updated quietly.
                    if tags.meta != track.meta {
                        changed += 1;
                    }
                    if self.library.refresh_from_file(file.id, tags.meta, tags.file).is_ok() {
                        // Just read, so plainly there.
                        self.availability.forget(file.id);
                        refreshed += 1;
                    }
                }
                Err(reason) => {
                    unreadable.push(file.id);
                    failures.push((file.location, reason));
                }
            }
        }
        if refreshed > 0 {
            self.library_changed();
        }
        // Perhaps gone, perhaps on a share that just dropped: find out which.
        self.check_tracks(&unreadable);
        if report.unavailable > 0 && origin == Origin::Listener {
            let text = match report.unavailable {
                1 => "1 song was left as it is: the drive or share it is on is not connected.".to_owned(),
                n => format!("{n} songs were left as they are: the drive or share they are on is not connected."),
            };
            self.notices.push(Notice::info(text));
        }
        match origin {
            Origin::Listener => self.notices.push(Notice::info(summary(refreshed, changed, report.kept))),
            // The watcher reads every song it sees change, TuneBox's own edits included: only
            // songs that really changed are worth a word.
            Origin::Watcher if changed > 0 => {
                let text = match changed {
                    1 => "Updated 1 song changed by another program.".to_owned(),
                    n => format!("Updated {n} songs changed by another program."),
                };
                self.notices.push(Notice::info(text));
            }
            Origin::Watcher => {}
        }
        if let Some((path, reason)) = failures.first() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let text = match failures.len() {
                1 => format!("“{name}” could not be read, so it was left as it is: {reason}"),
                n => format!("{n} songs could not be read and were left as they are. The first, “{name}”: {reason}"),
            };
            self.notices.push(Notice::warning(text));
        }
    }
}

/// The notice after a refresh: how many songs had changed, and how many were kept as edited.
fn summary(refreshed: usize, changed: usize, kept: usize) -> String {
    let mut text = match (refreshed, changed) {
        (0, _) => "No songs were refreshed.".to_owned(),
        (1, 0) => "The song already matched its file.".to_owned(),
        (n, 0) => format!("All {n} songs already matched their files."),
        (1, _) => "Updated the song from its file.".to_owned(),
        (n, c) if c == n => format!("Updated all {n} songs from their files."),
        (n, c) => format!("Updated {c} of {n} songs from their files."),
    };
    match kept {
        0 => {}
        1 => text.push_str(" 1 song with changes kept in TuneBox only was left as it is."),
        k => text.push_str(&format!(" {k} songs with changes kept in TuneBox only were left as they are.")),
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_says_what_changed_and_what_was_kept() {
        assert_eq!(summary(1, 0, 0), "The song already matched its file.");
        assert_eq!(summary(5, 2, 0), "Updated 2 of 5 songs from their files.");
        assert_eq!(
            summary(4, 4, 1),
            "Updated all 4 songs from their files. 1 song with changes kept in TuneBox only was left as it is."
        );
        assert_eq!(
            summary(0, 0, 3),
            "No songs were refreshed. 3 songs with changes kept in TuneBox only were left as they are."
        );
    }
}
