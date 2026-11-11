//! The audiobook in progress: starting one from the disc in the drive, editing it, ripping its
//! discs one after another, and building it into the library.
//!
//! There is at most one book at a time. It is saved after every change (`project`), so quitting
//! loses nothing. Edits are notes applied when the book is built, and every one can be undone
//! (in this session: the history is not saved).
//!
//! Between discs, the book waits for the next one ([`Session::audiobook_waiting_for`]): the
//! frontend shows "Insert disc N" with That Was the Last Disc as the only other way on.
//!
//! - `project`: the book's folder on disk.
//! - `discs`: a disc going in, being ripped, coming out.
//! - `building`: building the book into the library.

mod building;
mod discs;
mod project;

pub use discs::DiscQuestion;

use crate::EngineError;
use crate::cd::CdState;
use crate::disc_cover::DiscCover;
use crate::jobs::JobKind;
use crate::notices::Notice;
use crate::session::Session;
use std::path::PathBuf;
use tunebox_model::audiobook::{
    Book, BookDetails, BookDisc, BookEdits, BookError, BookTrack, Grouping, History, OutputSettings, Suggestion,
    TrackRef, Trim,
};
use tunebox_tags::Artwork;

/// How many edits can be undone.
const UNDO_LIMIT: usize = 500;

/// The book in progress and what goes with it.
pub(crate) struct AudiobookState {
    pub(crate) book: Book,
    pub(crate) folder: PathBuf,
    pub(crate) cover: Option<Artwork>,
    pub(crate) history: History<BookEdits>,
    /// A disc that needs the listener's word before it joins the book.
    pub(crate) question: Option<DiscQuestion>,
}

impl AudiobookState {
    fn new(book: Book, folder: PathBuf, cover: Option<Artwork>) -> Self {
        Self { book, folder, cover, history: History::new(UNDO_LIMIT), question: None }
    }
}

impl From<BookError> for EngineError {
    fn from(error: BookError) -> Self {
        EngineError::Audiobook(error.to_string())
    }
}

impl Session {
    /// The audiobook in progress, if any.
    pub fn audiobook(&self) -> Option<&Book> {
        self.audiobook.as_ref().map(|a| &a.book)
    }

    /// The book's cover, if it has one.
    pub fn audiobook_cover(&self) -> Option<&Artwork> {
        self.audiobook.as_ref()?.cover.as_ref()
    }

    /// The disc the book waits for: ripping is not finished and every kept track of the last
    /// disc is ripped. The frontend asks for it ("Insert disc 3"), offering only That Was the
    /// Last Disc besides.
    pub fn audiobook_waiting_for(&self) -> Option<u16> {
        let book = self.audiobook()?;
        if book.ripping_done || self.is_busy(JobKind::RipBookDisc) {
            return None;
        }
        let last = u16::try_from(book.discs.len()).ok()?;
        let disc = book.disc(last)?;
        let ripped = disc.tracks.iter().all(|t| t.staged || !book.is_kept(TrackRef::new(last, t.number)));
        ripped.then(|| book.next_disc_number())
    }

    /// The question about the disc in the drive, if it needs the listener's word.
    pub fn audiobook_question(&self) -> Option<&DiscQuestion> {
        self.audiobook.as_ref()?.question.as_ref()
    }

    /// The suggested edits (likely announcements, proposed cuts).
    pub fn audiobook_suggestions(&self) -> Vec<Suggestion> {
        self.audiobook().map(Book::suggestions).unwrap_or_default()
    }

    /// Starts an audiobook from the disc in the drive, which becomes its disc 1, as one
    /// chapter. `details` are what the listener typed; any left empty come from the disc's
    /// names (MusicBrainz's, when looked up).
    pub fn start_audiobook(&mut self, details: BookDetails) -> Result<(), EngineError> {
        if self.audiobook.is_some() {
            return Err(EngineError::Audiobook(
                "An audiobook is already in progress. Build it or discard it before starting another.".into(),
            ));
        }
        if self.jobs.is_running(JobKind::Rip) {
            return Err(EngineError::Busy("Importing the CD"));
        }
        let cd = self.cd.as_ref().ok_or(EngineError::NoDisc)?;
        let info = &cd.info;
        let mut details = details;
        let fill = |value: &mut String, from: &str| {
            if value.trim().is_empty() {
                *value = from.to_owned();
            }
        };
        fill(&mut details.title, &info.album);
        fill(&mut details.author, &info.artist);
        details.year = details.year.or(info.year);
        details.release_id = details.release_id.or_else(|| info.release_id.clone());
        details.release_group_id = details.release_group_id.or_else(|| info.release_group_id.clone());
        let cover = match &cd.cover {
            DiscCover::Found { artwork, .. } => Some(artwork.clone()),
            _ => None,
        };
        let mut book = Book::new(details, info.release_media.clone());
        book.add_disc(book_disc(cd, &book));
        let books = project::books_folder(&self.paths.library_file);
        let folder = project::new_folder(&books, chrono::Utc::now())
            .map_err(|e| EngineError::Audiobook(format!("Could not make the audiobook's folder: {e}")))?;
        let state = AudiobookState::new(book, folder, cover);
        project::save(&state.folder, &state.book, state.cover.as_ref(), true)
            .map_err(|e| EngineError::Audiobook(format!("Could not save the audiobook: {e}")))?;
        self.audiobook = Some(state);
        self.content_changed();
        Ok(())
    }

    /// Replaces the book's details (title, author, narrator…).
    pub fn set_audiobook_details(&mut self, details: BookDetails) -> Result<(), EngineError> {
        let state = self.audiobook.as_mut().ok_or(EngineError::NoAudiobook)?;
        state.book.details = details;
        self.save_audiobook(false);
        Ok(())
    }

    /// Chooses how the book is built.
    pub fn set_audiobook_output(&mut self, output: OutputSettings) -> Result<(), EngineError> {
        output.check()?;
        let state = self.audiobook.as_mut().ok_or(EngineError::NoAudiobook)?;
        state.book.output = output;
        self.save_audiobook(false);
        Ok(())
    }

    /// Replaces the book's cover, or removes it.
    pub fn set_audiobook_cover(&mut self, cover: Option<Artwork>) -> Result<(), EngineError> {
        let state = self.audiobook.as_mut().ok_or(EngineError::NoAudiobook)?;
        state.cover = cover;
        self.save_audiobook(true);
        Ok(())
    }

    /// Starts a chapter at each of `tracks`.
    pub fn split_audiobook_chapter(&mut self, tracks: &[TrackRef]) -> Result<(), EngineError> {
        self.edit_audiobook(|book| book.split(tracks))
    }

    /// Makes one chapter of the chapters holding `tracks` (with one chapter, joins it to the one
    /// before).
    pub fn join_audiobook_chapters(&mut self, tracks: &[TrackRef]) -> Result<(), EngineError> {
        self.edit_audiobook(|book| book.join(tracks))
    }

    /// Names the chapter holding `track`; an empty name goes back to "Chapter N".
    pub fn rename_audiobook_chapter(&mut self, track: TrackRef, name: &str) -> Result<(), EngineError> {
        self.edit_audiobook(|book| book.rename(track, name))
    }

    /// Removes `tracks` from the book, or restores them when all are removed (Delete).
    pub fn toggle_audiobook_tracks(&mut self, tracks: &[TrackRef]) -> Result<(), EngineError> {
        self.edit_audiobook(|book| book.toggle_removed(tracks))
    }

    /// Removes `tracks` from the book (`removed`), or puts them back.
    pub fn set_audiobook_tracks_removed(&mut self, tracks: &[TrackRef], removed: bool) -> Result<(), EngineError> {
        self.edit_audiobook(|book| book.set_removed(tracks, removed))
    }

    /// Sets what is cut off the start and end of `track`.
    pub fn trim_audiobook_track(&mut self, track: TrackRef, trim: Trim) -> Result<(), EngineError> {
        self.edit_audiobook(|book| book.set_trim(track, trim))
    }

    /// Regroups the whole book: per disc, per track, or from the track names.
    pub fn regroup_audiobook(&mut self, grouping: Grouping) -> Result<(), EngineError> {
        self.edit_audiobook(|book| {
            book.regroup(grouping);
            Ok(())
        })
    }

    /// Accepts the suggestions for `tracks` (removing announcements, making proposed cuts).
    pub fn accept_audiobook_suggestions(&mut self, tracks: &[TrackRef]) -> Result<(), EngineError> {
        self.edit_audiobook(|book| {
            for suggestion in book.suggestions().into_iter().filter(|s| tracks.contains(&s.track())) {
                apply_suggestion(book, suggestion)?;
            }
            Ok(())
        })
    }

    /// Accepts every suggestion in the book. Returns how many there were.
    pub fn accept_all_audiobook_suggestions(&mut self) -> Result<usize, EngineError> {
        let mut count = 0;
        self.edit_audiobook(|book| {
            let suggestions = book.suggestions();
            count = suggestions.len();
            suggestions.into_iter().try_for_each(|s| apply_suggestion(book, s))
        })?;
        Ok(count)
    }

    /// Undoes the last edit of the book. `false` when there is nothing to undo.
    pub fn undo_audiobook(&mut self) -> Result<bool, EngineError> {
        let state = self.audiobook.as_mut().ok_or(EngineError::NoAudiobook)?;
        let Some(previous) = state.history.undo(state.book.edits.clone()) else { return Ok(false) };
        state.book.edits = previous;
        self.save_audiobook(false);
        Ok(true)
    }

    /// Redoes the edit undone last. `false` when there is nothing to redo.
    pub fn redo_audiobook(&mut self) -> Result<bool, EngineError> {
        let state = self.audiobook.as_mut().ok_or(EngineError::NoAudiobook)?;
        let Some(next) = state.history.redo(state.book.edits.clone()) else { return Ok(false) };
        state.book.edits = next;
        self.save_audiobook(false);
        Ok(true)
    }

    /// Throws the book away: its ripped discs, edits and cover. Nothing in the library or the
    /// media folder is touched.
    pub fn discard_audiobook(&mut self) -> Result<(), EngineError> {
        if self.jobs.is_running(JobKind::RipBookDisc) || self.jobs.is_running(JobKind::BuildBook) {
            return Err(EngineError::Audiobook("Stop the rip or the build first, then discard the audiobook.".into()));
        }
        let state = self.audiobook.take().ok_or(EngineError::NoAudiobook)?;
        if let Err(e) = project::delete(&state.folder) {
            self.notices.push(Notice::warning(format!(
                "The audiobook was discarded, but its ripped discs could not all be deleted ({e}). They are in {}.",
                state.folder.display()
            )));
        }
        self.content_changed();
        Ok(())
    }

    /// Applies `edit` to the book, recording what it changed for undo and saving.
    fn edit_audiobook(&mut self, edit: impl FnOnce(&mut Book) -> Result<(), BookError>) -> Result<(), EngineError> {
        let state = self.audiobook.as_mut().ok_or(EngineError::NoAudiobook)?;
        let before = state.book.edits.clone();
        let mut book = state.book.clone();
        edit(&mut book)?;
        if book.edits != before {
            state.history.record(before);
        }
        state.book = book;
        self.save_audiobook(false);
        Ok(())
    }

    /// Saves the book; a failure is a notice, since the book in memory is still whole.
    pub(crate) fn save_audiobook(&mut self, cover_changed: bool) {
        self.content_changed();
        let Some(state) = self.audiobook.as_ref() else { return };
        if let Err(e) = project::save(&state.folder, &state.book, state.cover.as_ref(), cover_changed) {
            self.notices.push(Notice::error(format!("Could not save the audiobook: {e}. It is kept until you quit.")));
        }
    }

    /// Opens the book in progress, if there is one, when the session starts.
    pub(crate) fn load_audiobook(&mut self) {
        match project::load(&project::books_folder(&self.paths.library_file)) {
            Ok(Some(loaded)) => {
                self.audiobook = Some(AudiobookState::new(loaded.book, loaded.folder, loaded.cover));
            }
            Ok(None) => {}
            Err(damaged) => {
                let moved = damaged.moved_to.map(|p| format!(" It was moved to {}.", p.display())).unwrap_or_default();
                self.notices.push(Notice::error(format!(
                    "The audiobook in progress could not be read ({}).{moved} Its ripped discs are kept in {}.",
                    damaged.reason,
                    damaged.folder.display()
                )));
            }
        }
    }
}

fn apply_suggestion(book: &mut Book, suggestion: Suggestion) -> Result<(), BookError> {
    match suggestion {
        Suggestion::Remove(track) => book.set_removed(&[track], true),
        Suggestion::Trim { track, trim } => book.set_trim(track, trim),
    }
}

/// The disc in the drive as the book's next disc: its tracks, their lengths, and their names
/// (MusicBrainz's for this disc of the release when known, else the disc's).
pub(crate) fn book_disc(cd: &CdState, book: &Book) -> BookDisc {
    let position = u32::from(book.next_disc_number());
    let titles = book.release_medium(position).map(|m| m.track_titles.clone()).unwrap_or_default();
    let tracks = cd
        .toc
        .audio_tracks()
        .enumerate()
        .map(|(index, track)| {
            let sectors = cd.toc.sectors(track.number).unwrap_or(0);
            let title = titles
                .get(index)
                .cloned()
                .or_else(|| cd.info.tracks.get(index).map(|(title, _)| title.clone()))
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| format!("Track {:02}", track.number));
            BookTrack {
                number: track.number,
                title,
                samples: u64::from(sectors) * u64::from(tunebox_rip::SAMPLES_PER_SECTOR),
                ..BookTrack::default()
            }
        })
        .collect();
    BookDisc { disc_id: cd.disc_id.clone(), tracks }
}
