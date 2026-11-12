//! What the audiobook page is showing: which tracks are selected, which chapters are folded,
//! and the chapter name being typed. Pure, so the keyboard's behaviour is tested here.
//!
//! The keyboard moves through *items*: the tracks of unfolded chapters, and folded chapters as
//! a whole. Selecting a folded chapter selects all its tracks, so every command works on it.

use std::collections::BTreeSet;
use tunebox_engine::model::audiobook::{ChapterView, TrackRef};

/// A row of the page's table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookRow {
    /// A chapter's heading, by the track it starts at.
    Chapter(TrackRef),
    Track(TrackRef),
}

/// A chapter name being typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Renaming {
    pub start: TrackRef,
    pub text: String,
}

/// The page's state.
#[derive(Clone, Debug, Default)]
pub struct BookView {
    selection: BTreeSet<TrackRef>,
    /// Where Shift extends from, and where the keyboard is.
    anchor: Option<TrackRef>,
    cursor: Option<TrackRef>,
    /// Folded chapters, by start.
    folded: BTreeSet<TrackRef>,
    pub renaming: Option<Renaming>,
}

impl BookView {
    /// The table's rows: each chapter's heading, then its tracks unless folded.
    pub fn rows(&self, chapters: &[ChapterView]) -> Vec<BookRow> {
        let mut rows = Vec::new();
        for chapter in chapters {
            rows.push(BookRow::Chapter(chapter.start));
            if !self.folded.contains(&chapter.start) {
                rows.extend(chapter.tracks.iter().map(|t| BookRow::Track(*t)));
            }
        }
        rows
    }

    pub fn is_folded(&self, start: TrackRef) -> bool {
        self.folded.contains(&start)
    }

    pub fn is_selected(&self, track: TrackRef) -> bool {
        self.selection.contains(&track)
    }

    /// Whether every track of `chapter` is selected.
    pub fn is_chapter_selected(&self, chapter: &ChapterView) -> bool {
        !chapter.tracks.is_empty() && chapter.tracks.iter().all(|t| self.selection.contains(t))
    }

    /// The selected tracks, in book order.
    pub fn selected(&self) -> Vec<TrackRef> {
        self.selection.iter().copied().collect()
    }

    /// The track the keyboard is on (the last one clicked or moved to).
    pub fn cursor(&self) -> Option<TrackRef> {
        self.cursor.filter(|c| self.selection.contains(c)).or_else(|| self.selection.first().copied())
    }

    /// A plain click on a track.
    pub fn select_only(&mut self, track: TrackRef) {
        self.selection = BTreeSet::from([track]);
        self.anchor = Some(track);
        self.cursor = Some(track);
    }

    /// ⌘/Ctrl-click.
    pub fn toggle(&mut self, track: TrackRef) {
        if !self.selection.remove(&track) {
            self.selection.insert(track);
        }
        self.anchor = Some(track);
        self.cursor = Some(track);
    }

    /// Shift-click: every track from the anchor to `track`.
    pub fn extend_to(&mut self, chapters: &[ChapterView], track: TrackRef) {
        let anchor = self.anchor.unwrap_or(track);
        self.selection = self.span(chapters, anchor, track).into_iter().collect();
        self.cursor = Some(track);
    }

    /// A click on a chapter's heading: all its tracks.
    pub fn select_chapter(&mut self, chapter: &ChapterView) {
        self.selection = chapter.tracks.iter().copied().collect();
        self.anchor = chapter.tracks.first().copied();
        self.cursor = self.anchor;
    }

    /// ↑ (`forward` false) or ↓, with Shift to extend.
    pub fn step(&mut self, chapters: &[ChapterView], forward: bool, extend: bool) {
        let items = self.items(chapters);
        let here = self.cursor().and_then(|c| items.iter().position(|item| item.contains(&c)));
        let next = match (here, forward) {
            (None, true) => 0,
            (None, false) => items.len().saturating_sub(1),
            (Some(i), true) => (i + 1).min(items.len().saturating_sub(1)),
            (Some(i), false) => i.saturating_sub(1),
        };
        let Some(item) = items.get(next) else { return };
        let (first, last) = match (item.first(), item.last()) {
            (Some(f), Some(l)) => (*f, *l),
            _ => return,
        };
        if extend {
            self.selection.extend(item.iter().copied());
            if let Some(anchor) = self.anchor {
                let target = if forward { last } else { first };
                self.selection.extend(self.span(chapters, anchor, target));
            }
        } else {
            self.selection = item.iter().copied().collect();
            self.anchor = Some(first);
        }
        self.cursor = Some(if forward { last } else { first });
    }

    /// ⌥↑ / ⌥↓: to the previous or next chapter's first track.
    pub fn jump_chapter(&mut self, chapters: &[ChapterView], forward: bool) {
        let here = self.cursor().and_then(|c| chapters.iter().position(|ch| ch.tracks.contains(&c)));
        let target = match (here, forward) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(chapters.len().saturating_sub(1)),
            // Back to this chapter's start first, as a text editor goes to the line's start.
            (Some(i), false) => {
                let at_start = chapters.get(i).and_then(|c| c.tracks.first()) == self.cursor().as_ref();
                if at_start { i.saturating_sub(1) } else { i }
            }
        };
        let Some(chapter) = chapters.get(target) else { return };
        if self.folded.contains(&chapter.start) {
            self.select_chapter(chapter);
        } else if let Some(first) = chapter.tracks.first() {
            self.select_only(*first);
        }
    }

    /// ← / →: folds or unfolds the chapters of the selection.
    pub fn fold_selected(&mut self, chapters: &[ChapterView], fold: bool) {
        let touched: Vec<TrackRef> =
            chapters.iter().filter(|c| c.tracks.iter().any(|t| self.selection.contains(t))).map(|c| c.start).collect();
        for start in touched {
            self.set_folded(start, fold);
        }
    }

    /// ⌥← / ⌥→: every chapter.
    pub fn fold_all(&mut self, chapters: &[ChapterView], fold: bool) {
        for chapter in chapters {
            self.set_folded(chapter.start, fold);
        }
    }

    pub fn set_folded(&mut self, start: TrackRef, fold: bool) {
        if fold {
            self.folded.insert(start);
        } else {
            self.folded.remove(&start);
        }
    }

    /// Return: starts typing the name of the chapter at the cursor.
    pub fn start_renaming(&mut self, chapters: &[ChapterView]) {
        let chapter = self.cursor().and_then(|c| chapters.iter().find(|ch| ch.tracks.contains(&c)));
        if let Some(chapter) = chapter {
            self.renaming = Some(Renaming { start: chapter.start, text: chapter.name.clone() });
        }
    }

    /// Tab / ⇧Tab while renaming: the next or previous chapter's name, after the one typed is
    /// saved. Returns the name to save.
    pub fn rename_next(&mut self, chapters: &[ChapterView], forward: bool) -> Option<Renaming> {
        let typed = self.renaming.take()?;
        let here = chapters.iter().position(|c| c.start == typed.start)?;
        let next = if forward { here.checked_add(1) } else { here.checked_sub(1) };
        if let Some(chapter) = next.and_then(|i| chapters.get(i)) {
            self.renaming = Some(Renaming { start: chapter.start, text: chapter.name.clone() });
            if let Some(first) = chapter.tracks.first() {
                self.select_only(*first);
            }
        }
        Some(typed)
    }

    /// Forgets what is no longer in the book (after a disc was left out, or the book replaced).
    pub fn keep_only(&mut self, chapters: &[ChapterView]) {
        let tracks: BTreeSet<TrackRef> = chapters.iter().flat_map(|c| c.tracks.iter().copied()).collect();
        let starts: BTreeSet<TrackRef> = chapters.iter().map(|c| c.start).collect();
        self.selection.retain(|t| tracks.contains(t));
        self.folded.retain(|s| starts.contains(s));
        if self.renaming.as_ref().is_some_and(|r| !starts.contains(&r.start)) {
            self.renaming = None;
        }
    }

    /// The keyboard's items: a folded chapter's tracks together, else each track alone.
    fn items(&self, chapters: &[ChapterView]) -> Vec<Vec<TrackRef>> {
        let mut items = Vec::new();
        for chapter in chapters {
            if self.folded.contains(&chapter.start) {
                items.push(chapter.tracks.clone());
            } else {
                items.extend(chapter.tracks.iter().map(|t| vec![*t]));
            }
        }
        items
    }

    /// Every track from `a` to `b`, whichever comes first.
    fn span(&self, chapters: &[ChapterView], a: TrackRef, b: TrackRef) -> Vec<TrackRef> {
        let (from, to) = (a.min(b), a.max(b));
        chapters.iter().flat_map(|c| c.tracks.iter().copied()).filter(|t| (from..=to).contains(t)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(disc: u16, track: u8) -> TrackRef {
        TrackRef::new(disc, track)
    }

    /// Chapters of these tracks, numbered in order.
    fn chapters(groups: &[&[(u16, u8)]]) -> Vec<ChapterView> {
        groups
            .iter()
            .enumerate()
            .map(|(i, tracks)| {
                let tracks: Vec<TrackRef> = tracks.iter().map(|(d, n)| t(*d, *n)).collect();
                ChapterView {
                    start: tracks[0],
                    discs: (tracks[0].disc, tracks[tracks.len() - 1].disc),
                    tracks,
                    name: format!("Chapter {}", i + 1),
                    named: false,
                    number: Some(i + 1),
                    samples: 0,
                }
            })
            .collect()
    }

    fn book() -> Vec<ChapterView> {
        chapters(&[&[(1, 1), (1, 2)], &[(1, 3), (2, 1)], &[(2, 2)]])
    }

    #[test]
    fn arrows_walk_the_tracks_and_shift_extends() {
        let chapters = book();
        let mut view = BookView::default();
        view.step(&chapters, true, false);
        assert_eq!(view.selected(), [t(1, 1)]);
        view.step(&chapters, true, false);
        view.step(&chapters, true, true);
        view.step(&chapters, true, true);
        assert_eq!(view.selected(), [t(1, 2), t(1, 3), t(2, 1)], "across the disc boundary");
        view.step(&chapters, false, false);
        assert_eq!(view.selected(), [t(1, 3)]);
        for _ in 0..10 {
            view.step(&chapters, true, false);
        }
        assert_eq!(view.selected(), [t(2, 2)], "stops at the end");
    }

    #[test]
    fn a_folded_chapter_is_one_item() {
        let chapters = book();
        let mut view = BookView::default();
        view.fold_all(&chapters, true);
        assert_eq!(
            view.rows(&chapters),
            [BookRow::Chapter(t(1, 1)), BookRow::Chapter(t(1, 3)), BookRow::Chapter(t(2, 2))]
        );
        view.step(&chapters, true, false);
        view.step(&chapters, true, false);
        assert_eq!(view.selected(), [t(1, 3), t(2, 1)], "the whole second chapter");
        view.fold_selected(&chapters, false);
        assert_eq!(view.rows(&chapters).len(), 5);
    }

    #[test]
    fn option_arrows_jump_between_chapter_starts() {
        let chapters = book();
        let mut view = BookView::default();
        view.select_only(t(2, 1));
        view.jump_chapter(&chapters, false);
        assert_eq!(view.selected(), [t(1, 3)], "to this chapter's start first");
        view.jump_chapter(&chapters, false);
        assert_eq!(view.selected(), [t(1, 1)]);
        view.jump_chapter(&chapters, true);
        view.jump_chapter(&chapters, true);
        assert_eq!(view.selected(), [t(2, 2)]);
    }

    #[test]
    fn clicks_select_toggle_and_extend() {
        let chapters = book();
        let mut view = BookView::default();
        view.select_only(t(1, 2));
        view.extend_to(&chapters, t(2, 1));
        assert_eq!(view.selected(), [t(1, 2), t(1, 3), t(2, 1)]);
        view.toggle(t(1, 3));
        assert_eq!(view.selected(), [t(1, 2), t(2, 1)]);
        view.select_chapter(&chapters[2]);
        assert!(view.is_chapter_selected(&chapters[2]));
    }

    #[test]
    fn tab_moves_through_the_names_saving_each() {
        let chapters = book();
        let mut view = BookView::default();
        view.select_only(t(1, 2));
        view.start_renaming(&chapters);
        assert_eq!(view.renaming.as_ref().unwrap().start, t(1, 1));
        view.renaming.as_mut().unwrap().text = "Saltmarsh".into();
        let saved = view.rename_next(&chapters, true).unwrap();
        assert_eq!((saved.start, saved.text.as_str()), (t(1, 1), "Saltmarsh"));
        assert_eq!(view.renaming.as_ref().unwrap().start, t(1, 3));
        assert_eq!(view.selected(), [t(1, 3)]);
        view.rename_next(&chapters, true);
        assert!(view.rename_next(&chapters, true).is_some(), "the last name is saved too");
        assert!(view.renaming.is_none(), "and renaming ends");
    }

    #[test]
    fn what_left_the_book_is_forgotten() {
        let mut view = BookView::default();
        view.select_only(t(2, 2));
        view.fold_all(&book(), true);
        let shorter = chapters(&[&[(1, 1), (1, 2), (1, 3)]]);
        view.keep_only(&shorter);
        assert!(view.selected().is_empty());
        assert!(view.is_folded(t(1, 1)) && !view.is_folded(t(2, 2)));
    }
}
