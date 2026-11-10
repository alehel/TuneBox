//! Locate…: pointing a missing song at its file, wherever it went. The song keeps everything
//! TuneBox knows about it (ratings, plays, playlists, edits); only where its file is, and the
//! file's own facts, change.
//!
//! Songs rarely go missing alone: an album folder was renamed, a library moved to a new drive.
//! So the move the listener just pointed out (`/old/Music/…` became `/new/Music/…`) is applied
//! to the other missing songs, in the background, and each is relinked only if a file of the
//! same size and format is waiting at its new place.

use crate::EngineError;
use crate::file_check::FileStatus;
use crate::jobs::JobKind;
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use std::path::{Path, PathBuf};
use tunebox_model::{AudioFormat, TrackId};

/// A missing song that may have moved the same way, and where it would be now.
#[derive(Debug)]
pub(crate) struct Candidate {
    id: TrackId,
    new_location: PathBuf,
    size_bytes: u64,
    format: AudioFormat,
}

impl Session {
    /// Links a song to `file`, its file found somewhere else, then looks for other missing songs
    /// that moved the same way. Nothing is written to any music file.
    pub fn locate(&mut self, id: TrackId, file: PathBuf) -> Result<(), EngineError> {
        let track = self.library.track(id).ok_or(tunebox_library::LibraryError::NoSuchTrack(id))?;
        let title = track.title().to_owned();
        let old_location = track.location.clone();
        if let Some(other) = self.library.track_at(&file).filter(|other| *other != id) {
            let other_title = self.library.track(other).map(|t| t.title().to_owned()).unwrap_or_default();
            return Err(EngineError::Locate(format!(
                "That file already belongs to “{other_title}” in the library. Choose the file of “{title}”."
            )));
        }
        let tags = tunebox_tags::read(&file)
            .map_err(|e| EngineError::Locate(format!("That file cannot be used for “{title}”: {e}")))?;
        self.library.relocate(id, file.clone())?;
        // The library's details stay (they may hold the listener's edits); the file's facts are
        // the new file's.
        self.library.set_file(id, tags.file)?;
        self.availability.forget(id);
        self.library_changed();
        self.notices.push(Notice::info(format!("“{title}” is linked to its file again.")));
        self.look_for_others_moved_alike(&old_location, &file);
        Ok(())
    }

    /// Starts looking for the other missing songs at the places the same move would put them.
    fn look_for_others_moved_alike(&mut self, old: &Path, new: &Path) {
        let Some((from, to)) = moved_prefix(old, new) else { return };
        let candidates: Vec<Candidate> = self
            .library
            .tracks()
            .filter(|t| *self.file_status(t.id) == FileStatus::Missing)
            .filter_map(|t| {
                let rest = t.location.strip_prefix(&from).ok()?;
                let new_location = to.join(rest);
                let place_free = self.library.track_at(&new_location).is_none();
                place_free.then_some(Candidate {
                    id: t.id,
                    new_location,
                    size_bytes: t.file.size_bytes,
                    format: t.file.format,
                })
            })
            .collect();
        if candidates.is_empty() || self.jobs.is_running(JobKind::Locate) {
            return;
        }
        self.jobs.spawn(JobKind::Locate, "Looking for other missing songs".into(), move |ctx| {
            let total = candidates.len();
            let mut found = Vec::new();
            for (done, candidate) in candidates.into_iter().enumerate() {
                ctx.progress(done as f32 / total.max(1) as f32, "");
                // The same size and kind of file: the song itself, not another that happens to
                // have its name.
                let same_size =
                    std::fs::metadata(&candidate.new_location).is_ok_and(|m| m.len() == candidate.size_bytes);
                let same_kind = candidate
                    .new_location
                    .extension()
                    .and_then(|e| e.to_str())
                    .and_then(AudioFormat::from_extension)
                    .is_some_and(|f| f == candidate.format);
                if same_size && same_kind {
                    found.push((candidate.id, candidate.new_location));
                }
            }
            Outcome::LocatedOthers(found)
        });
    }

    pub(crate) fn finish_locating_others(&mut self, found: Vec<(TrackId, PathBuf)>) {
        let mut relinked = 0;
        for (id, location) in found {
            // Still missing, and the place still free: the library may have moved on meanwhile.
            let still_missing = *self.file_status(id) == FileStatus::Missing;
            if still_missing
                && self.library.track_at(&location).is_none()
                && self.library.relocate(id, location).is_ok()
            {
                self.availability.forget(id);
                relinked += 1;
            }
        }
        if relinked > 0 {
            self.library_changed();
            let text = match relinked {
                1 => "Found 1 other missing song in the same place, and linked it too.".to_owned(),
                n => format!("Found {n} other missing songs in the same place, and linked them too."),
            };
            self.notices.push(Notice::info(text));
        }
    }
}

/// How a path moved, as the folders that changed: `/old/Music/A/1.flac` → `/new/Music/A/1.flac`
/// gives (`/old`, `/new`); the parts both paths end with are what stayed the same. A file that
/// was renamed too gives its old and new folders. `None` when nothing is left to map from.
fn moved_prefix(old: &Path, new: &Path) -> Option<(PathBuf, PathBuf)> {
    let old_parts: Vec<_> = old.components().collect();
    let new_parts: Vec<_> = new.components().collect();
    let same_ending = old_parts.iter().rev().zip(new_parts.iter().rev()).take_while(|(a, b)| a == b).count();
    let (old_keep, new_keep) = if same_ending == 0 {
        (old_parts.len().checked_sub(1)?, new_parts.len().checked_sub(1)?)
    } else {
        (old_parts.len() - same_ending, new_parts.len() - same_ending)
    };
    let from: PathBuf = old_parts.iter().take(old_keep).collect();
    let to: PathBuf = new_parts.iter().take(new_keep).collect();
    // Mapping the whole disk would send every missing song looking; that is not a move.
    (from.parent().is_some() && from != to).then_some((from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(old: &str, new: &str) -> Option<(PathBuf, PathBuf)> {
        moved_prefix(Path::new(old), Path::new(new))
    }

    #[test]
    fn the_move_is_what_changed_before_the_shared_ending() {
        assert_eq!(
            moved("/old/Music/Band/Album/01.flac", "/new disk/Music/Band/Album/01.flac"),
            Some(("/old".into(), "/new disk".into()))
        );
        assert_eq!(
            moved("/m/Band/Album/01.flac", "/m/Band/Album (1997)/01.flac"),
            Some(("/m/Band/Album".into(), "/m/Band/Album (1997)".into()))
        );
        // Renamed as well: the folders are the move.
        assert_eq!(moved("/m/A/01 Song.flac", "/n/B/Song.flac"), Some(("/m/A".into(), "/n/B".into())));
    }

    #[test]
    fn nothing_to_map_from_is_no_move() {
        assert_eq!(moved("/a.flac", "/b/a.flac"), None, "from the top of the disk");
        assert_eq!(moved("/m/a.flac", "/m/a.flac"), None, "the same place");
    }
}
