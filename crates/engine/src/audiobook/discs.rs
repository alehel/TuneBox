//! A disc of the book going in, being ripped, and coming out.
//!
//! While the book waits for its next disc, a disc that is read is checked
//! ([`Book::check_disc`]): the next one joins the book as one more chapter and waits for Rip
//! Disc; one already ripped is ejected with a word about which disc is wanted; one that may not
//! belong waits for the listener's answer ([`DiscQuestion`]). Ripping a disc stages every one of
//! its tracks not yet ripped (removed ones too, so a removal can be undone after the disc is put
//! away), then ejects it.

use super::{book_disc, project};
use crate::EngineError;
use crate::jobs::{JobId, JobKind};
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use std::sync::Arc;
use tunebox_model::audiobook::{DiscCheck, Pause, TrackRef};
use tunebox_rip::{StageReport, StageRequest};

/// A disc that may not belong to the book, waiting for the listener's word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscQuestion {
    /// MusicBrainz says it is disc `says` of the release; the book expects disc `expected`.
    OtherDisc { says: u32, expected: u16 },
    /// It is not a disc of the release the book came from (or MusicBrainz does not know it).
    Unrelated { expected: u16 },
}

impl DiscQuestion {
    /// The question, as the listener reads it.
    pub fn text(&self) -> String {
        match self {
            DiscQuestion::OtherDisc { says, expected } => format!(
                "MusicBrainz says this is disc {says} of the book, but disc {expected} comes next. Use it as disc \
                 {expected} anyway, or eject it and insert disc {expected}?"
            ),
            DiscQuestion::Unrelated { expected } => format!(
                "This disc is not one MusicBrainz lists for this book. Use it as disc {expected} anyway, or eject it?"
            ),
        }
    }
}

impl Session {
    /// Called when a newly read disc is in the drive: what it is to the book in progress.
    pub(crate) fn audiobook_disc_arrived(&mut self) {
        let Some(disc_id) = self.cd.as_ref().map(|cd| cd.disc_id.clone()) else { return };
        let Some(book) = self.audiobook().filter(|b| !b.ripping_done) else { return };
        let check = book.check_disc(&disc_id);
        let waiting = self.audiobook_waiting_for();
        if let Some(state) = self.audiobook.as_mut() {
            state.question = None;
        }
        match (check, waiting) {
            (DiscCheck::Next { .. }, Some(_)) => self.add_disc_to_audiobook(),
            (DiscCheck::OtherDisc { says, expected }, Some(_)) => self.ask(DiscQuestion::OtherDisc { says, expected }),
            (DiscCheck::Unrelated, Some(expected)) => self.ask(DiscQuestion::Unrelated { expected }),
            (DiscCheck::InBook { disc, missing }, waiting) if missing.is_empty() => {
                let wanted = waiting.map_or_else(String::new, |next| format!(" Insert disc {next}."));
                self.notices
                    .push(Notice::info(format!("This is disc {disc} of the book, which is already ripped.{wanted}")));
                self.eject_disc();
            }
            // The disc waiting for Rip Disc, or one with tracks still to rip: Rip Disc rips them.
            (DiscCheck::InBook { .. }, _) => {}
            (_, None) => {
                if let Some(pending) = self.audiobook().and_then(pending_disc) {
                    self.notices.push(Notice::info(format!(
                        "Disc {pending} of the book is not ripped yet. Put it back in and choose Rip Disc, or remove its \
                         tracks from the book."
                    )));
                }
            }
        }
    }

    /// Answers the question about the disc in the drive: use it as the next disc, or eject it.
    pub fn answer_audiobook_question(&mut self, use_it: bool) -> Result<(), EngineError> {
        let state = self.audiobook.as_mut().ok_or(EngineError::NoAudiobook)?;
        if state.question.take().is_none() {
            return Err(EngineError::Audiobook("There is no question about the disc to answer.".into()));
        }
        if use_it {
            self.add_disc_to_audiobook();
        } else {
            self.eject_disc();
        }
        self.content_changed();
        Ok(())
    }

    /// Whether Rip Disc can run: the disc in the drive is in the book with tracks not ripped, and
    /// nothing else is ripping.
    pub fn can_rip_audiobook_disc(&self) -> bool {
        let (Some(book), Some(cd)) = (self.audiobook(), self.cd.as_ref()) else { return false };
        let rippable = matches!(book.check_disc(&cd.disc_id), DiscCheck::InBook { missing, .. } if !missing.is_empty());
        rippable && !self.jobs.is_running(JobKind::RipBookDisc) && !self.jobs.is_running(JobKind::Rip)
    }

    /// Rips the disc in the drive into the book: every track of it not ripped yet. Ejects it
    /// when done.
    pub fn rip_audiobook_disc(&mut self) -> Result<JobId, EngineError> {
        if self.jobs.is_running(JobKind::RipBookDisc) || self.jobs.is_running(JobKind::Rip) {
            return Err(EngineError::Busy("Ripping a disc"));
        }
        let state = self.audiobook.as_ref().ok_or(EngineError::NoAudiobook)?;
        let cd = self.cd.as_ref().ok_or(EngineError::NoDisc)?;
        let DiscCheck::InBook { disc, missing } = state.book.check_disc(&cd.disc_id) else {
            let wanted = self
                .audiobook_waiting_for()
                .or_else(|| pending_disc(&state.book))
                .map_or_else(String::new, |n| format!(" Insert disc {n}."));
            return Err(EngineError::Audiobook(format!("The disc in the drive is not part of the book yet.{wanted}")));
        };
        if missing.is_empty() {
            return Err(EngineError::Audiobook(format!("Disc {disc} is already ripped.")));
        }
        let request = StageRequest {
            toc: cd.toc.clone(),
            tracks: missing.iter().map(|t| t.track).collect(),
            folder: project::disc_folder(&state.folder, disc),
            read_offset: self.prefs.read_offset(cd.drive.identity().key()),
        };
        let drive = Arc::clone(&cd.drive);
        let title = format!("Ripping disc {disc} of “{}”", state.book.details.title);
        Ok(self.jobs.spawn(JobKind::RipBookDisc, title, move |ctx| {
            let report = tunebox_rip::stage(drive.as_ref(), &request, ctx.cancel_flag(), &mut |p| {
                ctx.progress(p.overall(), format!("Ripping disc {disc}, track {} of {}", p.index + 1, p.count));
            });
            // A stopped rip leaves the disc in, to carry on.
            let ejected =
                if report.cancelled { Ok(false) } else { drive.eject().map(|()| true).map_err(|e| e.to_string()) };
            Outcome::BookDiscRipped { disc, report, ejected }
        }))
    }

    pub(crate) fn finish_audiobook_rip(&mut self, disc: u16, report: StageReport, ejected: Result<bool, String>) {
        let Some(state) = self.audiobook.as_mut() else { return };
        for staged in &report.staged {
            if let Some(track) = state.book.track_mut(TrackRef::new(disc, staged.number)) {
                track.staged = true;
                track.failure = None;
                track.pauses = staged.pauses.iter().map(|r| Pause { start: r.start, end: r.end }).collect();
            }
        }
        for (number, reason) in &report.failures {
            if let Some(track) = state.book.track_mut(TrackRef::new(disc, *number)) {
                track.failure = Some(reason.clone());
            }
        }
        self.save_audiobook(false);
        for (number, reason) in &report.failures {
            self.notices.push(Notice::warning(format!(
                "Track {disc}:{number} could not be ripped ({reason}). Clean disc {disc} and insert it again, or remove \
                 the track from the book."
            )));
        }
        if report.cancelled {
            self.notices.push(Notice::info(format!("Ripping disc {disc} was stopped. Choose Rip Disc to carry on.")));
        }
        match ejected {
            Ok(true) => {
                self.cd = None;
                self.content_changed();
            }
            Ok(false) => {}
            Err(reason) => self.notices.push(Notice::info(format!(
                "Disc {disc} is ripped, but the drive did not open ({reason}). Take the disc out by hand."
            ))),
        }
    }

    /// Ends the ripping: the book has all its discs. When MusicBrainz lists more discs for the
    /// release than were ripped, this is refused with the counts unless `anyway`. A last disc
    /// that was put in but not ripped at all is left out of the book.
    pub fn finish_audiobook_ripping(&mut self, anyway: bool) -> Result<(), EngineError> {
        if self.jobs.is_running(JobKind::RipBookDisc) {
            return Err(EngineError::Audiobook("A disc is being ripped. Wait for it, or stop it first.".into()));
        }
        let state = self.audiobook.as_mut().ok_or(EngineError::NoAudiobook)?;
        let book = &mut state.book;
        if let Some(last) = book.discs.last()
            && book.discs.len() > 1
            && last.tracks.iter().all(|t| !t.staged)
        {
            let number = book.next_disc_number().saturating_sub(1);
            book.discs.pop();
            book.edits.chapter_starts.retain(|t| t.disc != number);
            book.edits.names.retain(|t, _| t.disc != number);
            book.edits.removed.retain(|t| t.disc != number);
            book.edits.trims.retain(|t, _| t.disc != number);
        }
        if let Some(pending) = pending_disc(book) {
            return Err(EngineError::Audiobook(format!(
                "Disc {pending} is not fully ripped. Rip it, or remove its unripped tracks from the book."
            )));
        }
        let ripped = book.discs.len();
        if let Some(listed) = book.release_disc_count()
            && listed > ripped
            && !anyway
        {
            return Err(EngineError::MoreDiscsListed { listed, ripped });
        }
        book.ripping_done = true;
        state.history.clear();
        state.question = None;
        self.save_audiobook(false);
        Ok(())
    }

    /// Rip Another Disc: takes back That Was the Last Disc, for a disc found afterwards. The book
    /// waits for its next disc again.
    pub fn resume_audiobook_ripping(&mut self) -> Result<(), EngineError> {
        if self.jobs.is_running(JobKind::BuildBook) {
            return Err(EngineError::Audiobook("The book is being built. Wait for it, or stop it first.".into()));
        }
        let state = self.audiobook.as_mut().ok_or(EngineError::NoAudiobook)?;
        state.book.ripping_done = false;
        self.save_audiobook(false);
        Ok(())
    }

    /// Adds the disc in the drive to the book, as its next disc and one more chapter.
    fn add_disc_to_audiobook(&mut self) {
        let (Some(cd), Some(state)) = (self.cd.as_ref(), self.audiobook.as_mut()) else { return };
        let disc = book_disc(cd, &state.book);
        state.book.add_disc(disc);
        self.save_audiobook(false);
    }

    /// The disc a question was about is gone: so is the question.
    pub(crate) fn forget_audiobook_question(&mut self) {
        if let Some(state) = self.audiobook.as_mut() {
            state.question = None;
        }
    }

    fn ask(&mut self, question: DiscQuestion) {
        if let Some(state) = self.audiobook.as_mut() {
            state.question = Some(question);
        }
        self.content_changed();
    }
}

/// A disc whose kept tracks are not all ripped, if any.
fn pending_disc(book: &tunebox_model::audiobook::Book) -> Option<u16> {
    let unripped = book.unripped_kept();
    unripped.first().map(|t| t.disc)
}
