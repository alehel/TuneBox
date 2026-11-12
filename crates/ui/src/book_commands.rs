//! Carrying out the audiobook's commands and actions, and showing its dialogs when the book
//! asks for something: the next disc, or an answer about a disc that may not belong.

use crate::actions::Action;
use crate::app::TuneBoxApp;
use crate::dialogs::{
    BuildAudiobook, ConfirmFinish, Dialog, DiscQuestionDialog, InsertDisc, StartAudiobook, TrimDialog,
};
use crate::state::SourceItem;
use tunebox_engine::commands::CommandId;
use tunebox_engine::model::audiobook::Grouping;
use tunebox_engine::{EngineError, Notice};

impl TuneBoxApp {
    /// Runs one of the Audiobook menu's commands (or Import as Audiobook…).
    pub(crate) fn run_book(&mut self, id: CommandId) {
        let chapters = self.session.audiobook().map(|b| b.chapters()).unwrap_or_default();
        let selected = self.view.book.selected();
        let cursor = self.view.book.cursor();
        let result = match id {
            CommandId::ImportAsAudiobook => {
                self.dialog = Some(Dialog::StartAudiobook(Box::new(StartAudiobook::new(&self.session))));
                Ok(())
            }
            CommandId::RipDisc => self.session.rip_audiobook_disc().map(|_| ()),
            CommandId::FinishRipping => match self.session.finish_audiobook_ripping(false) {
                Err(e @ EngineError::MoreDiscsListed { .. }) => {
                    self.dialog = Some(Dialog::ConfirmFinish(ConfirmFinish { message: e.to_string() }));
                    Ok(())
                }
                other => other,
            },
            CommandId::RipAnotherDisc => self.session.resume_audiobook_ripping(),
            CommandId::BuildAudiobook => match self.session.check_audiobook() {
                Ok(()) => {
                    self.dialog = Some(Dialog::BuildAudiobook(Box::new(BuildAudiobook::new(&self.session))));
                    Ok(())
                }
                // A book with too many chapters for one file can still be built per chapter.
                Err(EngineError::Audiobook(message)) if message.contains("one file holds") => {
                    self.dialog = Some(Dialog::BuildAudiobook(Box::new(BuildAudiobook::new(&self.session))));
                    Ok(())
                }
                Err(e) => Err(e),
            },
            CommandId::SplitChapter => self.session.split_audiobook_chapter(&selected),
            CommandId::JoinChapters => self.session.join_audiobook_chapters(&selected),
            CommandId::RenameChapter => {
                self.view.book.start_renaming(&chapters);
                Ok(())
            }
            CommandId::RemoveFromBook => self.session.toggle_audiobook_tracks(&selected),
            CommandId::TrimTrack => {
                match cursor.and_then(|track| TrimDialog::new(&self.session, track)) {
                    Some(dialog) => self.dialog = Some(Dialog::Trim(Box::new(dialog))),
                    None => self.notify(Notice::info("Only a ripped track can be trimmed: rip its disc first.")),
                }
                Ok(())
            }
            CommandId::AcceptSuggestion => self.session.accept_audiobook_suggestions(&selected),
            CommandId::AcceptAllSuggestions => self.session.accept_all_audiobook_suggestions().map(|_| ()),
            CommandId::PlayAcrossStart => match cursor {
                Some(track) => self.session.preview_audiobook_across(track),
                None => Ok(()),
            },
            CommandId::PlayEnd => match cursor {
                Some(track) => self.session.preview_audiobook_end(track),
                None => Ok(()),
            },
            CommandId::UndoBookEdit => self.session.undo_audiobook().map(|_| ()),
            CommandId::RedoBookEdit => self.session.redo_audiobook().map(|_| ()),
            CommandId::PreviousChapter | CommandId::NextChapter => {
                self.view.book.jump_chapter(&chapters, id == CommandId::NextChapter);
                Ok(())
            }
            CommandId::FoldChapter | CommandId::UnfoldChapter => {
                self.view.book.fold_selected(&chapters, id == CommandId::FoldChapter);
                Ok(())
            }
            CommandId::FoldAllChapters | CommandId::UnfoldAllChapters => {
                self.view.book.fold_all(&chapters, id == CommandId::FoldAllChapters);
                Ok(())
            }
            CommandId::GroupPerDisc => self.session.regroup_audiobook(Grouping::PerDisc),
            CommandId::GroupPerTrack => self.session.regroup_audiobook(Grouping::PerTrack),
            CommandId::GroupFromNames => self.session.regroup_audiobook(Grouping::FromNames),
            CommandId::AudiobookShortcuts => {
                self.dialog = Some(Dialog::BookShortcuts);
                Ok(())
            }
            CommandId::DiscardAudiobook => {
                self.dialog = Some(Dialog::ConfirmDiscard);
                Ok(())
            }
            _ => Ok(()),
        };
        self.report(result);
    }

    /// Space on the book page: stops what plays, or plays the selected track.
    pub(crate) fn play_pause_book(&mut self) {
        if self.session.audiobook_preview().is_some() {
            self.session.stop_audiobook_preview();
            return;
        }
        let Some(track) = self.view.book.cursor() else { return };
        let Some(book) = self.session.audiobook() else { return };
        let trim = book.trim(track);
        let to = book.track(track).map_or(0, |t| t.samples.saturating_sub(trim.end));
        let result = self.session.preview_audiobook(track, trim.start, to);
        self.report(result);
    }

    /// ⌘A on the book page: every track.
    pub(crate) fn select_all_book(&mut self) {
        let Some(book) = self.session.audiobook() else { return };
        let chapters = book.chapters();
        if let (Some(first), Some(last)) =
            (chapters.first().and_then(|c| c.tracks.first()), chapters.last().and_then(|c| c.tracks.last()))
        {
            self.view.book.select_only(*first);
            self.view.book.extend_to(&chapters, *last);
        }
    }

    /// Carries out an action of the book page.
    pub(crate) fn apply_book(&mut self, action: Action) {
        let result = match action {
            Action::StartAudiobook(details) => {
                let result = self.session.start_audiobook(*details);
                if result.is_ok() {
                    self.view.select_source(SourceItem::Book);
                }
                result
            }
            Action::SetBookDetails(details) => self.session.set_audiobook_details(*details),
            Action::RenameChapter(track, name) => self.session.rename_audiobook_chapter(track, &name),
            Action::SetBookTracksRemoved(tracks, removed) => {
                self.session.set_audiobook_tracks_removed(&tracks, removed)
            }
            Action::TrimBookTrack(track, trim) => self.session.trim_audiobook_track(track, trim),
            Action::AcceptBookSuggestions(tracks) => self.session.accept_audiobook_suggestions(&tracks),
            Action::PreviewBook { track, from, to } => self.session.preview_audiobook(track, from, to),
            Action::StopBookPreview => {
                self.session.stop_audiobook_preview();
                Ok(())
            }
            Action::BuildAudiobook(output) => {
                self.session.set_audiobook_output(output).and_then(|()| self.session.build_audiobook()).map(|_| ())
            }
            Action::AnswerDiscQuestion(use_it) => self.session.answer_audiobook_question(use_it),
            Action::FinishRipping { anyway } => self.session.finish_audiobook_ripping(anyway),
            Action::DiscardAudiobook => {
                let result = self.session.discard_audiobook();
                if result.is_ok() {
                    self.view.book = Default::default();
                    self.view.select_source(SourceItem::Music);
                }
                result
            }
            _ => return,
        };
        self.report(result);
    }

    /// Opens what the book asks for: the question about a disc wherever the listener is, and
    /// on the book page, "Insert disc N" between discs. A rip finishing brings the page up.
    pub(crate) fn follow_book(&mut self) {
        if self.dialog.is_some() {
            return;
        }
        if let Some(question) = self.session.audiobook_question() {
            self.dialog = Some(Dialog::DiscQuestion(DiscQuestionDialog { question: question.clone() }));
            return;
        }
        let waiting = self.session.audiobook_waiting_for();
        if waiting.is_some() && self.book_rip_just_ended {
            self.view.select_source(SourceItem::Book);
        }
        self.book_rip_just_ended = false;
        if let Some(next) = waiting
            && self.view.source == SourceItem::Book
        {
            self.dialog = Some(Dialog::InsertDisc(InsertDisc { next }));
        }
    }
}
