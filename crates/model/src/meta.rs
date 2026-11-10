//! Descriptive metadata: what a track is, as opposed to the file that holds it.

use crate::TagField;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    /// The additional fields, by field. A field that is absent was never read or edited and is
    /// left alone when the file is written; an empty value means "clear it". So a song whose
    /// extra fields TuneBox has not read can be edited without losing them.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<TagField, String>,
}

/// Why `TrackMeta::set` refused a field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldError {
    /// No field has this name.
    Unknown(String),
    /// The value does not suit the field, such as a date that is not a date.
    Invalid { field: TagField, value: String },
}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldError::Unknown(name) => {
                let extra: Vec<&str> = TagField::ALL.iter().map(|t| t.key()).collect();
                write!(
                    f,
                    "unknown field \"{name}\"; known fields are {}, {}",
                    TrackMeta::CORE_KEYS.join(", "),
                    extra.join(", ")
                )
            }
            FieldError::Invalid { field, value } => {
                write!(f, "\"{value}\" is not a valid {}; use the form {}", field.label().to_lowercase(), field.hint())
            }
        }
    }
}

impl TrackMeta {
    /// The names of the everyday fields, as `set` takes them.
    pub const CORE_KEYS: [&'static str; 15] = [
        "title",
        "artist",
        "album-artist",
        "album",
        "genre",
        "composer",
        "grouping",
        "comments",
        "year",
        "track",
        "track-total",
        "disc",
        "disc-total",
        "bpm",
        "compilation",
    ];

    /// An additional field's value ("" when not set).
    pub fn extra(&self, field: TagField) -> &str {
        self.extra.get(&field).map_or("", String::as_str)
    }

    /// A field's value by its name, as `set` takes it; `None` for a name that is no field.
    pub fn get(&self, key: &str) -> Option<String> {
        let number = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
        Some(match key {
            "title" => self.title.clone(),
            "artist" => self.artist.clone(),
            "album-artist" => self.album_artist.clone(),
            "album" => self.album.clone(),
            "genre" => self.genre.clone(),
            "composer" => self.composer.clone(),
            "grouping" => self.grouping.clone(),
            "comments" => self.comments.clone(),
            "year" => number(self.year),
            "track" => number(self.track.number),
            "track-total" => number(self.track.total),
            "disc" => number(self.disc.number),
            "disc-total" => number(self.disc.total),
            "bpm" => number(self.bpm),
            "compilation" => if self.compilation { "yes" } else { "" }.to_owned(),
            other => self.extra(TagField::from_key(other)?).to_owned(),
        })
    }

    /// Sets a field by its name (an everyday field from `CORE_KEYS` or a `TagField` key), as
    /// the command line does. Numbers that do not parse, and "", clear the field.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), FieldError> {
        let value = value.trim();
        let text = value.to_owned();
        let number = value.parse::<u32>().ok();
        match key {
            "title" => self.title = text,
            "artist" => self.artist = text,
            "album-artist" => self.album_artist = text,
            "album" => self.album = text,
            "genre" => self.genre = text,
            "composer" => self.composer = text,
            "grouping" => self.grouping = text,
            "comments" => self.comments = text,
            "year" => self.year = number,
            "track" => self.track.number = number,
            "track-total" => self.track.total = number,
            "disc" => self.disc.number = number,
            "disc-total" => self.disc.total = number,
            "bpm" => self.bpm = number,
            "compilation" => self.compilation = matches!(value, "1" | "yes" | "true"),
            other => {
                let field = TagField::from_key(other).ok_or_else(|| FieldError::Unknown(other.to_owned()))?;
                if !field.accepts(&text) {
                    return Err(FieldError::Invalid { field, value: text });
                }
                self.extra.insert(field, text);
            }
        }
        Ok(())
    }

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
    use crate::FieldKind;

    #[test]
    fn number_of_labels() {
        assert_eq!(NumberOf::new(3, 12).label(), "3 of 12");
        assert_eq!(NumberOf { number: Some(3), total: None }.label(), "3");
        assert_eq!(NumberOf::default().label(), "");
    }

    #[test]
    fn set_by_name_reaches_everyday_and_extra_fields() {
        let mut meta = TrackMeta::default();
        meta.set("album-artist", " Band ").unwrap();
        meta.set("track", "4").unwrap();
        meta.set("catalogue-number", "HL-001").unwrap();
        assert_eq!(meta.album_artist, "Band");
        assert_eq!(meta.track.number, Some(4));
        assert_eq!(meta.extra(TagField::CatalogNumber), "HL-001");
        assert!(meta.set("colour", "blue").unwrap_err().to_string().contains("catalogue-number"));
        assert!(matches!(meta.set("release-date", "spring"), Err(FieldError::Invalid { .. })));
        for key in TrackMeta::CORE_KEYS {
            assert!(meta.set(key, "1").is_ok(), "{key}");
        }
        for key in TrackMeta::CORE_KEYS.into_iter().chain(TagField::ALL.map(TagField::key)) {
            let value = match (key, TagField::from_key(key).map(TagField::kind)) {
                ("compilation", _) => "yes",
                (_, Some(FieldKind::Date)) => "2011-04-02",
                _ => "1",
            };
            meta.set(key, value).unwrap();
            assert_eq!(meta.get(key).as_deref(), Some(value), "{key}");
        }
        assert_eq!(meta.get("colour"), None);
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
