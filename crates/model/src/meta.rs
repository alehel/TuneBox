//! Descriptive metadata: what a track is, as opposed to the file that holds it.

use serde::{Deserialize, Serialize};

/// "Track 3 of 12", "Disc 1 of 2". Either part may be unknown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumberOf {
    pub number: Option<u32>,
    pub total: Option<u32>,
}

impl NumberOf {
    pub const fn new(number: u32, total: u32) -> Self {
        Self { number: Some(number), total: Some(total) }
    }

    /// "3 of 12", "3", or "" when nothing is known.
    pub fn label(&self) -> String {
        match (self.number, self.total) {
            (Some(n), Some(t)) => format!("{n} of {t}"),
            (Some(n), None) => n.to_string(),
            (None, Some(t)) => format!("of {t}"),
            (None, None) => String::new(),
        }
    }
}

/// The tag fields TuneBox reads, shows, edits and writes back.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub genre: String,
    pub composer: String,
    pub grouping: String,
    pub comments: String,
    pub year: Option<u32>,
    pub track: NumberOf,
    pub disc: NumberOf,
    pub bpm: Option<u32>,
    /// Part of a compilation ("Various Artists").
    pub compilation: bool,
}

impl TrackMeta {
    /// The artist an album is filed under: the album artist, else the track artist, else
    /// "Compilations" for compilations.
    pub fn filing_artist(&self) -> &str {
        if self.compilation {
            "Compilations"
        } else if !self.album_artist.is_empty() {
            &self.album_artist
        } else {
            &self.artist
        }
    }

    /// The title to show: the tag, or `fallback` (usually the file name) when it is empty.
    pub fn display_title<'a>(&'a self, fallback: &'a str) -> &'a str {
        if self.title.is_empty() { fallback } else { &self.title }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_of_labels() {
        assert_eq!(NumberOf::new(3, 12).label(), "3 of 12");
        assert_eq!(NumberOf { number: Some(3), total: None }.label(), "3");
        assert_eq!(NumberOf::default().label(), "");
    }

    #[test]
    fn filing_artist_prefers_album_artist_then_artist() {
        let mut meta = TrackMeta { artist: "Solo".into(), ..TrackMeta::default() };
        assert_eq!(meta.filing_artist(), "Solo");
        meta.album_artist = "Band".into();
        assert_eq!(meta.filing_artist(), "Band");
        meta.compilation = true;
        assert_eq!(meta.filing_artist(), "Compilations");
    }
}
