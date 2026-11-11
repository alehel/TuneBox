//! Chapters, stored as the tracks where they start, and the edits that move them.
//!
//! Storing start points rather than lists of tracks keeps every edit small and makes the
//! invariants hold by construction: every track belongs to exactly one chapter, and a chapter
//! can run across discs with no special case. The book's first track always starts a chapter,
//! whether or not it is in the set.
//!
//! Chapters without a typed name are called "Chapter 1", "Chapter 2"… in order and renumbered
//! after every edit. Only chapters with at least one kept track are numbered; a chapter whose
//! every track is removed is shown as left out and written nowhere.

use super::book::{Book, BookError, TrackRef, Trim};
use super::layout::MAX_CHAPTER_NAME_BYTES;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Everything the listener has decided about the book's tracks. This is what undo restores.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookEdits {
    /// The tracks where a chapter starts.
    pub chapter_starts: BTreeSet<TrackRef>,
    /// Typed chapter names, by the track where the chapter starts.
    pub names: BTreeMap<TrackRef, String>,
    /// Tracks left out of the book.
    pub removed: BTreeSet<TrackRef>,
    /// What is cut off the start and end of tracks.
    pub trims: BTreeMap<TrackRef, Trim>,
}

/// A chapter as the book page shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterView {
    /// The track it starts at.
    pub start: TrackRef,
    /// All its tracks, kept and removed, in order.
    pub tracks: Vec<TrackRef>,
    /// Its typed name, or "Chapter N", or "Left out".
    pub name: String,
    /// Whether the name was typed (and so is kept through renumbering).
    pub named: bool,
    /// Its number among the chapters the book will contain; `None` when every track is removed.
    pub number: Option<usize>,
    /// Its length once built: kept tracks, less their trims.
    pub samples: u64,
    /// The first and last disc it has tracks on.
    pub discs: (u16, u16),
}

/// A way to regroup the whole book at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grouping {
    /// One chapter per disc: how every book starts.
    PerDisc,
    /// One chapter per track.
    PerTrack,
    /// Consecutive tracks whose names differ only by a part number ("Chapter 4 (1)",
    /// "Chapter 4 (2)") make one chapter, named without the part.
    FromNames,
}

impl Book {
    /// The chapters, in order, with every track in exactly one.
    pub fn chapters(&self) -> Vec<ChapterView> {
        let mut chapters: Vec<ChapterView> = Vec::new();
        for (index, (track, data)) in self.tracks().enumerate() {
            let starts_chapter = index == 0 || self.edits.chapter_starts.contains(&track);
            if starts_chapter {
                chapters.push(ChapterView {
                    start: track,
                    tracks: Vec::new(),
                    name: String::new(),
                    named: false,
                    number: None,
                    samples: 0,
                    discs: (track.disc, track.disc),
                });
            }
            if let Some(chapter) = chapters.last_mut() {
                chapter.tracks.push(track);
                chapter.discs.1 = track.disc;
                if self.is_kept(track) {
                    chapter.samples += self.trimmed_length(track, data);
                    chapter.number = Some(0);
                }
            }
        }
        let mut number = 0;
        for chapter in &mut chapters {
            if chapter.number.is_some() {
                number += 1;
                chapter.number = Some(number);
            }
            match self.edits.names.get(&chapter.start) {
                Some(name) => {
                    chapter.name = name.clone();
                    chapter.named = true;
                }
                None => chapter.name = chapter.number.map_or_else(|| "Left out".to_owned(), |n| format!("Chapter {n}")),
            }
        }
        chapters
    }

    /// The chapter `track` belongs to.
    pub fn chapter_of(&self, track: TrackRef) -> Option<ChapterView> {
        self.chapters().into_iter().find(|c| c.tracks.contains(&track))
    }

    /// Starts a chapter at each of `tracks`.
    pub fn split(&mut self, tracks: &[TrackRef]) -> Result<(), BookError> {
        self.check_tracks(tracks)?;
        self.edits.chapter_starts.extend(tracks.iter().copied());
        Ok(())
    }

    /// Makes one chapter of the chapters holding `tracks`, and of any between them. With the
    /// tracks all in one chapter, that chapter joins the one before it. The first chapter's
    /// name is kept; the others' are dropped.
    pub fn join(&mut self, tracks: &[TrackRef]) -> Result<(), BookError> {
        self.check_tracks(tracks)?;
        let chapters = self.chapters();
        let holding: Vec<usize> = chapters
            .iter()
            .enumerate()
            .filter(|(_, c)| c.tracks.iter().any(|t| tracks.contains(t)))
            .map(|(i, _)| i)
            .collect();
        let (Some(&first), Some(&last)) = (holding.first(), holding.last()) else { return Ok(()) };
        // One chapter: join it to the one before (the first chapter has none).
        let first = if first == last { first.saturating_sub(1) } else { first };
        for chapter in chapters.iter().take(last + 1).skip(first + 1) {
            self.edits.chapter_starts.remove(&chapter.start);
            self.edits.names.remove(&chapter.start);
        }
        Ok(())
    }

    /// Names the chapter holding `track`. An empty name goes back to "Chapter N".
    pub fn rename(&mut self, track: TrackRef, name: &str) -> Result<(), BookError> {
        self.check_tracks(&[track])?;
        let name = name.trim();
        if name.len() > MAX_CHAPTER_NAME_BYTES {
            return Err(BookError::NameTooLong { bytes: name.len() });
        }
        let Some(chapter) = self.chapter_of(track) else { return Err(BookError::NoSuchTrack(track)) };
        if name.is_empty() {
            self.edits.names.remove(&chapter.start);
        } else {
            self.edits.names.insert(chapter.start, name.to_owned());
        }
        Ok(())
    }

    /// Leaves `tracks` out of the book (`removed`), or puts them back.
    pub fn set_removed(&mut self, tracks: &[TrackRef], removed: bool) -> Result<(), BookError> {
        self.check_tracks(tracks)?;
        for track in tracks {
            if removed {
                self.edits.removed.insert(*track);
            } else {
                self.edits.removed.remove(track);
            }
        }
        Ok(())
    }

    /// Removes `tracks`, or restores them when every one is already removed: what Delete does.
    pub fn toggle_removed(&mut self, tracks: &[TrackRef]) -> Result<(), BookError> {
        let all_removed = tracks.iter().all(|t| !self.is_kept(*t));
        self.set_removed(tracks, !all_removed)
    }

    /// Sets what is cut off `track`. The track must be ripped, and something must be left.
    pub fn set_trim(&mut self, track: TrackRef, trim: Trim) -> Result<(), BookError> {
        let data = self.track(track).ok_or(BookError::NoSuchTrack(track))?;
        if !data.staged {
            return Err(BookError::NotRipped(track));
        }
        if trim.start.saturating_add(trim.end) >= data.samples {
            return Err(BookError::TrimTooLong { track, length: data.samples });
        }
        if trim.is_none() {
            self.edits.trims.remove(&track);
        } else {
            self.edits.trims.insert(track, trim);
        }
        Ok(())
    }

    /// The trim of `track`; no cut when none was set.
    pub fn trim(&self, track: TrackRef) -> Trim {
        self.edits.trims.get(&track).copied().unwrap_or_default()
    }

    /// Replaces every chapter boundary. Typed names stay with the starts that remain;
    /// [`Grouping::FromNames`] names its chapters after the tracks.
    pub fn regroup(&mut self, grouping: Grouping) {
        let starts: BTreeSet<TrackRef> = match grouping {
            Grouping::PerDisc => self
                .tracks()
                .filter(|(r, _)| self.disc(r.disc).and_then(|d| d.tracks.first()).is_some_and(|f| f.number == r.track))
                .map(|(r, _)| r)
                .collect(),
            Grouping::PerTrack => self.track_refs().into_iter().collect(),
            Grouping::FromNames => {
                let mut named = BTreeMap::new();
                let mut previous: Option<String> = None;
                for (track, data) in self.tracks() {
                    let base = base_name(&data.title);
                    let key = base.to_lowercase();
                    if previous.as_ref() != Some(&key) {
                        named.insert(track, base);
                    }
                    previous = Some(key);
                }
                let starts = named.keys().copied().collect();
                self.edits.names.extend(named);
                starts
            }
        };
        self.edits.names.retain(|start, _| starts.contains(start));
        self.edits.chapter_starts = starts;
    }

    fn check_tracks(&self, tracks: &[TrackRef]) -> Result<(), BookError> {
        match tracks.iter().find(|t| self.track(**t).is_none()) {
            Some(missing) => Err(BookError::NoSuchTrack(*missing)),
            None => Ok(()),
        }
    }
}

/// A track name without its part number: "Chapter 4 (2)", "Chapter 4 - Part 2" and
/// "Chapter 4, Pt. 2" are all "Chapter 4".
pub fn base_name(title: &str) -> String {
    let title = title.trim();
    // "Name (2)", "Name (Part 2)", "Name [2]".
    for (open, close) in [('(', ')'), ('[', ']')] {
        if let Some((head, part)) = title.strip_suffix(close).and_then(|t| t.rsplit_once(open))
            && is_part(part)
            && !head.trim().is_empty()
        {
            return head.trim().to_owned();
        }
    }
    // "Name - Part 2", "Name, Part 2", "Name: Pt. 2", "Name Part 2".
    let lower = title.to_lowercase();
    for marker in ["part ", "pt. ", "pt "] {
        // Lowercasing can change byte lengths outside ASCII; then positions would not carry over.
        if lower.len() != title.len() {
            break;
        }
        if let Some(at) = lower.rfind(marker) {
            let (Some(head), Some(tail)) = (title.get(..at), lower.get(at..)) else { continue };
            let head = head.trim_end().trim_end_matches(['-', ',', ':', '–', '—']).trim_end();
            if is_part(tail) && !head.is_empty() && at > 0 {
                return head.to_owned();
            }
        }
    }
    title.to_owned()
}

/// "2", "Part 2", "pt. 2", "part ii".
fn is_part(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    let number = lower
        .strip_prefix("part")
        .or_else(|| lower.strip_prefix("pt."))
        .or_else(|| lower.strip_prefix("pt"))
        .unwrap_or(&lower)
        .trim();
    !number.is_empty()
        && (number.chars().all(|c| c.is_ascii_digit()) || number.chars().all(|c| matches!(c, 'i' | 'v' | 'x')))
}

#[cfg(test)]
mod tests {
    use super::super::book::tests::book;
    use super::*;

    fn t(disc: u16, track: u8) -> TrackRef {
        TrackRef::new(disc, track)
    }

    fn names(book: &Book) -> Vec<String> {
        book.chapters().into_iter().map(|c| c.name).collect()
    }

    #[test]
    fn each_disc_starts_as_a_chapter_and_splits_renumber() {
        let mut book = book(&[&[10, 20, 30], &[40, 50]]);
        assert_eq!(names(&book), ["Chapter 1", "Chapter 2"]);
        book.split(&[t(1, 2), t(2, 2)]).unwrap();
        assert_eq!(names(&book), ["Chapter 1", "Chapter 2", "Chapter 3", "Chapter 4"]);
        let chapters = book.chapters();
        assert_eq!(chapters[1].tracks, vec![t(1, 2), t(1, 3)]);
        assert_eq!(chapters[1].samples, 50 * 44_100);
    }

    #[test]
    fn joining_across_discs_keeps_the_first_name() {
        let mut book = book(&[&[10, 20], &[30, 40]]);
        book.rename(t(1, 1), "Saltmarsh").unwrap();
        book.rename(t(2, 2), "Dropped").unwrap();
        book.join(&[t(1, 2), t(2, 1)]).unwrap();
        let chapters = book.chapters();
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].name, "Saltmarsh");
        assert_eq!(chapters[0].discs, (1, 2));
        assert!(book.edits.names.len() == 1);
    }

    #[test]
    fn joining_one_chapter_joins_it_to_the_one_before() {
        let mut book = book(&[&[10, 20, 30]]);
        book.regroup(Grouping::PerTrack);
        book.join(&[t(1, 3)]).unwrap();
        assert_eq!(book.chapters().len(), 2);
        assert_eq!(book.chapters()[1].tracks, vec![t(1, 2), t(1, 3)]);
        // The first chapter has none before it.
        book.join(&[t(1, 1)]).unwrap();
        assert_eq!(book.chapters().len(), 2);
        // Selecting tracks of the first and last chapters takes in the ones between.
        book.regroup(Grouping::PerTrack);
        book.join(&[t(1, 1), t(1, 3)]).unwrap();
        assert_eq!(book.chapters().len(), 1);
    }

    #[test]
    fn typed_names_survive_renumbering_and_empty_names_go_back() {
        let mut book = book(&[&[10], &[20], &[30]]);
        book.rename(t(2, 1), "The Funeral").unwrap();
        assert_eq!(names(&book), ["Chapter 1", "The Funeral", "Chapter 3"]);
        book.join(&[t(1, 1), t(2, 1)]).unwrap();
        assert_eq!(names(&book), ["Chapter 1", "Chapter 2"]);
        book.rename(t(3, 1), "  ").unwrap();
        assert!(book.edits.names.is_empty());
        let long = "x".repeat(MAX_CHAPTER_NAME_BYTES + 1);
        assert_eq!(book.rename(t(1, 1), &long), Err(BookError::NameTooLong { bytes: long.len() }));
    }

    #[test]
    fn removed_tracks_stay_in_their_chapter_and_empty_chapters_are_left_out() {
        let mut book = book(&[&[10, 5], &[6, 30]]);
        book.toggle_removed(&[t(1, 2), t(2, 1)]).unwrap();
        assert!(!book.is_kept(t(1, 2)));
        // A disc whose first track is removed still starts its chapter; it starts sounding later.
        let chapters = book.chapters();
        assert_eq!(chapters[1].tracks, vec![t(2, 1), t(2, 2)]);
        assert_eq!(chapters[1].samples, 30 * 44_100);
        // A chapter of removed tracks only is not numbered.
        book.split(&[t(1, 2)]).unwrap();
        assert_eq!(names(&book), ["Chapter 1", "Left out", "Chapter 2"]);
        // Delete on removed tracks puts them back.
        book.toggle_removed(&[t(1, 2), t(2, 1)]).unwrap();
        assert!(book.edits.removed.is_empty());
    }

    #[test]
    fn trims_must_leave_something_of_a_ripped_track() {
        let mut book = book(&[&[10]]);
        let all = 10 * 44_100;
        assert!(book.set_trim(t(1, 1), Trim { start: all / 2, end: all / 2 }).is_err());
        book.set_trim(t(1, 1), Trim { start: 0, end: 44_100 }).unwrap();
        assert_eq!(book.chapters()[0].samples, 9 * 44_100);
        book.set_trim(t(1, 1), Trim::default()).unwrap();
        assert!(book.edits.trims.is_empty());
        book.discs[0].tracks[0].staged = false;
        assert_eq!(book.set_trim(t(1, 1), Trim { start: 1, end: 0 }), Err(BookError::NotRipped(t(1, 1))));
        assert_eq!(book.set_trim(t(1, 9), Trim::default()), Err(BookError::NoSuchTrack(t(1, 9))));
    }

    #[test]
    fn regrouping_from_names_joins_parts_across_discs() {
        let mut book = book(&[&[1, 2, 3, 4], &[5, 6]]);
        let titles = ["Opening Credits", "Chapter 1 (1)", "Chapter 1 (2)", "Chapter 2 - Part 1"];
        for (track, title) in book.discs[0].tracks.iter_mut().zip(titles) {
            track.title = title.into();
        }
        book.discs[1].tracks[0].title = "Chapter 2 - Part 2".into();
        book.discs[1].tracks[1].title = "Chapter 3, Pt. 1".into();
        book.regroup(Grouping::FromNames);
        assert_eq!(names(&book), ["Opening Credits", "Chapter 1", "Chapter 2", "Chapter 3"]);
        assert_eq!(book.chapters()[2].discs, (1, 2));
        book.regroup(Grouping::PerDisc);
        assert_eq!(book.chapters().len(), 2);
        assert_eq!(names(&book), ["Opening Credits", "Chapter 2"], "a typed name stays with its start");
    }

    #[test]
    fn base_names_drop_only_part_numbers() {
        for (title, base) in [
            ("Chapter 4 (2)", "Chapter 4"),
            ("Chapter 4 (Part 2)", "Chapter 4"),
            ("Chapter 4 [3]", "Chapter 4"),
            ("Chapter 4 - Part 2", "Chapter 4"),
            ("Chapter 4, Pt. II", "Chapter 4"),
            ("Chapter 4", "Chapter 4"),
            ("Part 2", "Part 2"),
            ("The Party (Live)", "The Party (Live)"),
            ("Departure", "Departure"),
        ] {
            assert_eq!(base_name(title), base, "{title}");
        }
    }
}
