//! Building the book into the library.
//!
//! The build writes into the media folder, so it follows the media folder's rule in
//! `files.rs`: a read-only media folder is refused before anything is written. Once the files
//! are in the library and the library is saved, the staged rip is deleted.

use super::project;
use crate::EngineError;
use crate::jobs::{JobId, JobKind};
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use chrono::Utc;
use std::collections::BTreeMap;
use tunebox_audiobook::{BuildRequest, build, check};
use tunebox_library::NewTrack;

impl Session {
    /// Whether the book can be built as it is; the error says what to do first.
    pub fn check_audiobook(&self) -> Result<(), EngineError> {
        let book = self.audiobook().ok_or(EngineError::NoAudiobook)?;
        check(book).map_err(|e| EngineError::Audiobook(e.to_string()))
    }

    /// Builds the book into the media folder and the library, as chosen in its output settings.
    pub fn build_audiobook(&mut self) -> Result<JobId, EngineError> {
        if self.jobs.is_running(JobKind::BuildBook) {
            return Err(EngineError::Busy("Building the audiobook"));
        }
        if self.jobs.is_running(JobKind::RipBookDisc) {
            return Err(EngineError::Audiobook("A disc is being ripped. Wait for it, then build.".into()));
        }
        if !crate::files::may_write(&self.prefs, &self.prefs.media_folder) {
            return Err(EngineError::Protected(self.prefs.media_folder.clone()));
        }
        self.check_audiobook()?;
        let state = self.audiobook.as_ref().ok_or(EngineError::NoAudiobook)?;
        let staged = state
            .book
            .tracks()
            .filter(|(_, t)| t.staged)
            .map(|(r, _)| (r, project::staged_path(&state.folder, r)))
            .collect::<BTreeMap<_, _>>();
        let request = BuildRequest {
            book: state.book.clone(),
            staged,
            media_folder: self.prefs.media_folder.clone(),
            cover: state.cover.clone(),
        };
        let title = format!("Building “{}”", state.book.details.title);
        Ok(self.jobs.spawn(JobKind::BuildBook, title, move |ctx| {
            let result = build(&request, Utc::now(), ctx.cancel_flag(), &mut |p| {
                ctx.progress(p.fraction, format!("Building chapter {} of {}", p.chapter, p.chapters));
            });
            Outcome::BookBuilt(result.map_err(|e| e.to_string()))
        }))
    }

    pub(crate) fn finish_audiobook_build(&mut self, result: Result<Vec<NewTrack>, String>) {
        let tracks = match result {
            Ok(tracks) => tracks,
            Err(message) => {
                self.notices.push(Notice::error(format!("The audiobook was not built. {message}")));
                return;
            }
        };
        for track in tracks {
            self.library.add_track(track);
        }
        self.library_changed();
        let title = self.audiobook().map(|b| b.details.title.clone()).unwrap_or_default();
        // The library must hold the book before its rip is deleted: if saving fails, the rip is
        // kept and the book can be built again.
        if let Err(e) = self.save() {
            self.notices.push(Notice::error(format!(
                "“{title}” was built, but the library could not be saved ({e}). The ripped discs are kept."
            )));
            return;
        }
        if let Some(state) = self.audiobook.take()
            && let Err(e) = project::delete(&state.folder)
        {
            self.notices.push(Notice::warning(format!(
                "The ripped discs of “{title}” could not all be deleted ({e}). They are in {}.",
                state.folder.display()
            )));
        }
        self.notices.push(Notice::info(format!("“{title}” is in your library, under Audiobooks.")));
        self.content_changed();
    }
}
