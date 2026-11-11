//! Audiobooks ripped from several CDs: the book in progress and every decision about it.
//!
//! A [`Book`] holds the book's details, its discs in book order with their tracks, and the
//! listener's edits ([`BookEdits`]): where chapters start and what they are called, which
//! tracks are left out, and what is trimmed off the start or end of a track. Nothing here
//! touches audio: the edits are notes, applied when the book is built, so every one of them can
//! be undone ([`History`]) until then.
//!
//! - [`book`]: the book, its discs and tracks, and how tracks are addressed ([`TrackRef`]).
//! - [`chapters`]: chapters as start points, and the edits that move them.
//! - [`layout`]: what the build writes: chapters with their trimmed parts, file names, tags.
//! - [`suggest`]: likely announcements ("Please insert disc 2") and proposed trims.
//! - [`next_disc`]: what a newly inserted disc is to the book.
//! - [`history`]: undo and redo.
//!
//! Lengths and positions are in samples per channel at the CD's 44.1 kHz ([`SAMPLE_RATE`]).

pub mod book;
pub mod chapters;
pub mod history;
pub mod layout;
pub mod next_disc;
pub mod suggest;

pub use book::{
    Book, BookDetails, BookDisc, BookError, BookTrack, OutputFormat, OutputLayout, OutputSettings, Pause,
    ReleaseMedium, TrackRef, Trim,
};
pub use chapters::{BookEdits, ChapterView, Grouping};
pub use history::History;
pub use layout::{MAX_CHAPTER_NAME_BYTES, MAX_CHAPTERS_PER_FILE, OutputChapter, Part};
pub use next_disc::DiscCheck;
pub use suggest::Suggestion;

/// Samples per second per channel on an audio CD.
pub const SAMPLE_RATE: u32 = 44_100;

/// A length in samples as seconds, for messages and display.
pub fn seconds(samples: u64) -> f64 {
    samples as f64 / f64::from(SAMPLE_RATE)
}

/// Seconds as samples, rounded to the nearest sample; negative and non-finite values are 0.
pub fn samples(seconds: f64) -> u64 {
    if seconds.is_finite() && seconds > 0.0 { (seconds * f64::from(SAMPLE_RATE)).round() as u64 } else { 0 }
}
