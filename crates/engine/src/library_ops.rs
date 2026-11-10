//! Changing the library: importing, editing, rating, removing, and playlists.

use crate::jobs::{JobId, JobKind};
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use crate::EngineError;
use chrono::Utc;
use std::collections::HashSet;
use std::path::PathBuf;
use tunebox_import::{ImportOptions, ImportReport};
use tunebox_model::{PlaylistId, Rating, SmartRules, Track, TrackId, TrackMeta};

impl Session {
    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.library.track(id)
    }

    /// Adds files and folders to the library in the background ("Add to Library…").
    pub fn import(&mut self, paths: Vec<PathBuf>) -> Result<JobId, EngineError> {
        if self.jobs.is_running(JobKind::Import) {
            return Err(EngineError::Busy("An import"));
        }
        let known: HashSet<PathBuf> = self.library.tracks().map(|t| t.location.clone()).collect();
        let options =
            ImportOptions { media_folder: self.prefs.media_folder.clone(), copy_to_media_folder: self.prefs.copy_to_media_folder };
        Ok(self.jobs.spawn(JobKind::Import, "Adding to Library".into(), move |ctx| {
            ctx.progress(0.0, "Looking for music…");
            let files = tunebox_import::find_audio_files(&paths, ctx.cancel_flag());
            let report = tunebox_import::import(&files, &known, &options, Utc::now(), ctx.cancel_flag(), &mut |p| {
                let name = p.current.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                ctx.progress(p.done as f32 / p.total.max(1) as f32, name);
            });
            Outcome::Imported(report)
        }))
    }

    pub(crate) fn finish_import(&mut self, report: ImportReport) {
        let added = report.tracks.len();
        for track in report.tracks {
            self.library.add_track(track);
        }
        if added > 0 {
            self.library_changed();
        }
        let mut text = match added {
            0 => "No new music was found.".to_owned(),
            1 => "Added 1 song to the library.".to_owned(),
            n => format!("Added {n} songs to the library."),
        };
        if report.already_in_library > 0 {
            text.push_str(&format!(" {} were already in the library.", report.already_in_library));
        }
        self.notices.push(Notice::info(text));
        if let Some((path, reason)) = report.failures.first() {
            let others = report.failures.len() - 1;
            let more = if others > 0 { format!(" (and {others} more)") } else { String::new() };
            self.notices.push(Notice::warning(format!("Could not add {}{more}: {reason}", path.display())));
        }
    }

    /// Edits a track's details, writing them into its file as well. If the file cannot be
    /// written (read-only, missing) the library still takes the edit and a warning says why.
    pub fn edit_track(&mut self, id: TrackId, meta: TrackMeta) -> Result<(), EngineError> {
        let location = self.library.track(id).map(|t| t.location.clone()).ok_or(tunebox_library::LibraryError::NoSuchTrack(id))?;
        if let Err(e) = tunebox_tags::write(&location, &meta) {
            self.notices.push(Notice::warning(format!("The details were saved in the library but not in the file: {e}")));
        }
        self.library.set_meta(id, meta)?;
        self.library_changed();
        Ok(())
    }

    pub fn set_rating(&mut self, ids: &[TrackId], rating: Rating) {
        for id in ids {
            let _ = self.library.set_rating(*id, rating);
        }
        self.library_changed();
    }

    pub fn set_checked(&mut self, ids: &[TrackId], checked: bool) {
        for id in ids {
            let _ = self.library.set_checked(*id, checked);
        }
        self.library_changed();
    }

    /// Removes tracks from the library (and from every playlist). With `delete_files` their
    /// files are deleted from disk too.
    pub fn remove_tracks(&mut self, ids: &[TrackId], delete_files: bool) {
        if self.playback.current().is_some_and(|c| ids.contains(&c)) {
            self.stop();
        }
        let removed = self.library.remove_tracks(ids);
        if delete_files {
            let failed = removed.iter().filter(|t| std::fs::remove_file(&t.location).is_err()).count();
            if failed > 0 {
                self.notices.push(Notice::warning(format!("{failed} files could not be deleted from disk.")));
            }
        }
        self.library_changed();
    }

    // ----- playlists -----

    /// Creates a playlist holding `tracks` (which may be empty).
    pub fn create_playlist(&mut self, name: &str, tracks: &[TrackId]) -> Result<PlaylistId, EngineError> {
        let id = self.library.create_playlist(name)?;
        self.library.add_to_playlist(id, tracks)?;
        self.library_changed();
        Ok(id)
    }

    pub fn create_smart_playlist(&mut self, name: &str, rules: SmartRules) -> Result<PlaylistId, EngineError> {
        let id = self.library.create_smart_playlist(name, rules)?;
        self.library_changed();
        Ok(id)
    }

    pub fn set_smart_rules(&mut self, id: PlaylistId, rules: SmartRules) -> Result<(), EngineError> {
        self.library.set_smart_rules(id, rules)?;
        self.library_changed();
        Ok(())
    }

    pub fn rename_playlist(&mut self, id: PlaylistId, name: &str) -> Result<(), EngineError> {
        self.library.rename_playlist(id, name)?;
        self.library_changed();
        Ok(())
    }

    pub fn delete_playlist(&mut self, id: PlaylistId) -> Result<(), EngineError> {
        self.library.delete_playlist(id)?;
        self.library_changed();
        Ok(())
    }

    pub fn add_to_playlist(&mut self, id: PlaylistId, tracks: &[TrackId]) -> Result<(), EngineError> {
        self.library.add_to_playlist(id, tracks)?;
        self.library_changed();
        Ok(())
    }

    /// Removes entries by their positions in the playlist.
    pub fn remove_from_playlist(&mut self, id: PlaylistId, positions: &[usize]) -> Result<(), EngineError> {
        self.library.remove_from_playlist(id, positions)?;
        self.library_changed();
        Ok(())
    }

    pub fn move_in_playlist(&mut self, id: PlaylistId, positions: &[usize], target: usize) -> Result<(), EngineError> {
        self.library.move_in_playlist(id, positions, target)?;
        self.library_changed();
        Ok(())
    }
}
