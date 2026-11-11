//! Suggested edits: tracks that look like announcements ("Please insert disc 2") and cuts that
//! would take an announcement off the end or start of a disc.
//!
//! These are only suggestions: the book page shows them and the listener accepts or ignores
//! each. The thresholds are starting values, to be tuned on real audiobook discs.

use super::book::{Book, Pause, TrackRef, Trim};
use super::samples;

/// A track this short, first or last on its disc, is probably an announcement.
const SHORT_TRACK_SECONDS: f64 = 15.0;
/// A proposed cut looks this far into a track's start or end.
const LOOK_SECONDS: f64 = 20.0;
/// What may follow (or precede) the cut: an announcement is a sentence or two.
const ANNOUNCEMENT_MAX_SECONDS: f64 = 8.0;
/// The pause before an announcement is longer than the pauses between sentences.
const PAUSE_MIN_SECONDS: f64 = 1.0;

/// A suggested edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suggestion {
    /// The track looks like an announcement: leave it out.
    Remove(TrackRef),
    /// The track probably ends (or starts) with an announcement: cut it off.
    Trim { track: TrackRef, trim: Trim },
}

impl Suggestion {
    pub fn track(&self) -> TrackRef {
        match self {
            Suggestion::Remove(track) | Suggestion::Trim { track, .. } => *track,
        }
    }
}

impl Book {
    /// Every suggestion for the kept tracks, in book order. Tracks already trimmed at that end
    /// get no trim suggestion.
    pub fn suggestions(&self) -> Vec<Suggestion> {
        let mut out = Vec::new();
        for (index, disc) in self.discs.iter().enumerate() {
            let number = u16::try_from(index + 1).unwrap_or(u16::MAX);
            let last = disc.tracks.len().saturating_sub(1);
            for (position, track) in disc.tracks.iter().enumerate() {
                let r = TrackRef::new(number, track.number);
                if !self.is_kept(r) {
                    continue;
                }
                let at_edge = position == 0 || position == last;
                let short = track.samples < samples(SHORT_TRACK_SECONDS);
                if (at_edge && short && disc.tracks.len() > 1) || looks_like_announcement(&track.title) {
                    out.push(Suggestion::Remove(r));
                    continue;
                }
                if !track.staged {
                    continue;
                }
                let trim = self.trim(r);
                let mut proposed = trim;
                if position == last && trim.end == 0 {
                    proposed.end = cut_before_end(&track.pauses, track.samples).unwrap_or(0);
                }
                if position == 0 && index > 0 && trim.start == 0 {
                    proposed.start = cut_after_start(&track.pauses, track.samples).unwrap_or(0);
                }
                if proposed != trim && proposed.start.saturating_add(proposed.end) < track.samples {
                    out.push(Suggestion::Trim { track: r, trim: proposed });
                }
            }
        }
        out
    }
}

/// How much to cut off the end: up to the middle of the longest long pause after which only a
/// short stretch of sound is left.
fn cut_before_end(pauses: &[Pause], length: u64) -> Option<u64> {
    let window = length.saturating_sub(samples(LOOK_SECONDS));
    pauses
        .iter()
        .filter(|p| p.start >= window && p.length() >= samples(PAUSE_MIN_SECONDS))
        .filter(|p| {
            let after = length.saturating_sub(p.end);
            after > 0 && after <= samples(ANNOUNCEMENT_MAX_SECONDS)
        })
        .max_by_key(|p| p.length())
        .map(|p| length.saturating_sub(p.middle()))
}

/// How much to cut off the start: up to the middle of the longest long pause before which
/// there is only a short stretch of sound. A disc's first track that opens with "Disc two"
/// is the case this is for; the book's first disc is left alone (its start is the opening).
fn cut_after_start(pauses: &[Pause], length: u64) -> Option<u64> {
    let window = samples(LOOK_SECONDS).min(length);
    pauses
        .iter()
        .filter(|p| p.end <= window && p.length() >= samples(PAUSE_MIN_SECONDS))
        .filter(|p| p.start > 0 && p.start <= samples(ANNOUNCEMENT_MAX_SECONDS))
        .max_by_key(|p| p.length())
        .map(|p| p.middle())
}

/// Whether a track name reads like a disc announcement. English phrases only, to begin with.
pub fn looks_like_announcement(title: &str) -> bool {
    let lower = title.to_lowercase();
    let phrases = [
        "insert disc",
        "insert cd",
        "insert the next",
        "end of disc",
        "end of cd",
        "end of side",
        "continued on disc",
        "continues on disc",
        "continue on disc",
        "continued on cd",
        "turn the disc",
    ];
    if phrases.iter().any(|p| lower.contains(p)) {
        return true;
    }
    // "Disc 2", "CD Two", "Disc two." on their own.
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let numbers = [
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
        "twenty",
    ];
    matches!(words.as_slice(), [kind, number]
        if matches!(*kind, "disc" | "disk" | "cd")
            && (number.chars().all(|c| c.is_ascii_digit()) || numbers.contains(number)))
}

#[cfg(test)]
mod tests {
    use super::super::book::tests::book;
    use super::*;

    fn pause(from: f64, to: f64) -> Pause {
        Pause { start: samples(from), end: samples(to) }
    }

    #[test]
    fn announcement_names_are_recognised() {
        for title in ["Please insert disc 2", "End of Disc One", "Disc Two", "CD 3", "This story continues on disc 4"] {
            assert!(looks_like_announcement(title), "{title}");
        }
        for title in ["Chapter 4 (1)", "The Disc Jockey", "Discovery", "Opening Credits", "Disc"] {
            assert!(!looks_like_announcement(title), "{title}");
        }
    }

    #[test]
    fn short_tracks_at_a_disc_edge_are_suggested_for_removal() {
        let mut book = book(&[&[6, 200, 300, 9], &[200]]);
        let t = TrackRef::new;
        assert_eq!(book.suggestions(), vec![Suggestion::Remove(t(1, 1)), Suggestion::Remove(t(1, 4))]);
        // A removed track is not suggested again; a one-track disc is not an announcement.
        book.set_removed(&[t(1, 1)], true).unwrap();
        assert_eq!(book.suggestions(), vec![Suggestion::Remove(t(1, 4))]);
    }

    #[test]
    fn an_announcement_after_a_long_pause_at_the_end_of_a_disc_is_proposed_as_a_cut() {
        let mut book = book(&[&[60, 100], &[100]]);
        let t = TrackRef::new;
        // Sentences with short pauses, then a long pause and six seconds of announcement.
        book.discs[0].tracks[1].pauses = vec![pause(80.0, 80.4), pause(92.5, 94.0), pause(97.0, 97.3)];
        let expected = 100 * 44_100 - samples(93.25);
        assert_eq!(
            book.suggestions(),
            vec![Suggestion::Trim { track: t(1, 2), trim: Trim { start: 0, end: expected } }]
        );
        // Accepted, it is not proposed again.
        book.set_trim(t(1, 2), Trim { start: 0, end: expected }).unwrap();
        assert!(book.suggestions().is_empty());
    }

    #[test]
    fn a_disc_that_opens_with_its_number_is_proposed_a_cut_at_the_start() {
        let mut book = book(&[&[100], &[100]]);
        book.discs[1].tracks[0].pauses = vec![pause(2.0, 3.5), pause(10.0, 10.3)];
        // The first disc's opening is never cut.
        book.discs[0].tracks[0].pauses = vec![pause(2.0, 3.5)];
        assert_eq!(
            book.suggestions(),
            vec![Suggestion::Trim { track: TrackRef::new(2, 1), trim: Trim { start: samples(2.75), end: 0 } }]
        );
    }

    #[test]
    fn a_track_ending_mid_story_gets_no_proposal() {
        let mut book = book(&[&[100], &[100]]);
        // Only short pauses between sentences near the end.
        book.discs[0].tracks[0].pauses = vec![pause(90.0, 90.5), pause(95.0, 95.4)];
        assert!(book.suggestions().is_empty());
    }
}
