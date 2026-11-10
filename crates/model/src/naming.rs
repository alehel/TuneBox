//! Name patterns: how "Rename Files from Tags…" turns a song's details into a path.
//!
//! A pattern is literal text with fields in braces, and `/` between folders:
//! `{album-artist|artist}/{album}/{track:02} {title}`. Fields use the names the command line
//! uses (`TrackMeta::CORE_KEYS` and the `TagField` keys), so there is one vocabulary.
//!
//! - `{a|b}` takes the first of the fields that is not empty.
//! - `{track:02}` pads a number with zeros to two digits.
//! - `\` separates folders too, for listeners used to Windows paths.
//!
//! Each folder and the file name are made safe with [`organize::component`], so a tag can
//! never add a folder, climb out of one, or use a character some disk refuses. The file's own
//! extension is always kept. Parsing rejects patterns that would go wrong for every song (an
//! unknown field, an empty folder name, a file name without `{title}` or `{track}`), so a
//! mistake shows at once, not as a list of clashes.

use crate::organize::component;
use crate::{FieldKind, TagField, TrackMeta};
use std::fmt;
use std::path::PathBuf;

/// The widest zero padding a field may ask for.
const MAX_PADDING: usize = 9;

/// Ready-made patterns, as the rename dialog offers them: a name, and the pattern.
pub const PRESETS: [(&str, &str); 5] = [
    ("Artist/Album/Track Title", "{album-artist|artist}/{album}/{track:02} {title}"),
    ("Artist/Album/Disc-Track Title", "{album-artist|artist}/{album}/{disc}-{track:02} {title}"),
    ("Artist - Title", "{artist} - {title}"),
    ("Track Title", "{track:02} {title}"),
    ("Track Artist - Title", "{track:02} {artist} - {title}"),
];

/// The fields offered as one-click chips, the everyday ones first.
pub const COMMON_FIELDS: [&str; 9] =
    ["title", "artist", "album-artist", "album", "track", "disc", "year", "genre", "composer"];

/// A parsed pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamePattern {
    /// The folders, then the file name (always at least one).
    segments: Vec<Vec<Part>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    Text(String),
    Field(Field),
}

/// `{album-artist|artist:02}`: the fields to try in order, and the padding.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Field {
    keys: Vec<String>,
    width: Option<usize>,
}

/// Why a pattern cannot be used, in words for the listener.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternError {
    Empty,
    /// A `{` with no `}` after it.
    Unclosed,
    /// A `}` with no `{` before it.
    Unopened,
    /// `{}` or `{a|}`.
    EmptyField,
    UnknownField(String),
    /// Padding that is not a digit from 1 to 9, or padding on a field that is not a number.
    BadPadding(String),
    /// `//`, or `/` at the start or end.
    EmptyFolder,
    /// No `{title}` or `{track}` in the file name: every song in a folder would get one name.
    SameNameForAll,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatternError::Empty => write!(f, "The pattern is empty. Try {{track:02}} {{title}}."),
            PatternError::Unclosed => write!(f, "A “{{” has no “}}” to close it."),
            PatternError::Unopened => write!(f, "A “}}” has no “{{” before it."),
            PatternError::EmptyField => write!(f, "A pair of braces has no field name in it, as in {{}}."),
            PatternError::UnknownField(name) => {
                write!(f, "There is no field called “{name}”. Known fields: {}.", all_keys().join(", "))
            }
            PatternError::BadPadding(field) => write!(
                f,
                "“{{{field}}}” cannot be padded: padding is a digit from 1 to {MAX_PADDING}, as in {{track:02}}, and only numbers are padded."
            ),
            PatternError::EmptyFolder => {
                write!(f, "A folder name is empty: the pattern starts or ends with “/”, or has “//”.")
            }
            PatternError::SameNameForAll => write!(
                f,
                "The file name needs {{title}} or {{track}}, or every song in a folder would get the same name."
            ),
        }
    }
}

impl std::error::Error for PatternError {}

impl NamePattern {
    pub fn parse(text: &str) -> Result<NamePattern, PatternError> {
        if text.trim().is_empty() {
            return Err(PatternError::Empty);
        }
        let mut segments = vec![Vec::new()];
        let mut literal = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            match c {
                '{' => {
                    let mut inside = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some('{') | None => return Err(PatternError::Unclosed),
                            Some(c) => inside.push(c),
                        }
                    }
                    let current = segments.last_mut().ok_or(PatternError::Empty)?;
                    flush(&mut literal, current);
                    current.push(Part::Field(parse_field(&inside)?));
                }
                '}' => return Err(PatternError::Unopened),
                '/' | '\\' => {
                    let current = segments.last_mut().ok_or(PatternError::Empty)?;
                    flush(&mut literal, current);
                    segments.push(Vec::new());
                }
                c => literal.push(c),
            }
        }
        if let Some(current) = segments.last_mut() {
            flush(&mut literal, current);
        }
        if segments.iter().any(|parts| parts.iter().all(|p| matches!(p, Part::Text(t) if t.trim().is_empty()))) {
            return Err(PatternError::EmptyFolder);
        }
        let names_each_song = segments.last().is_some_and(|parts| {
            parts.iter().any(|p| matches!(p, Part::Field(f) if f.keys.iter().any(|k| k == "title" || k == "track")))
        });
        if !names_each_song {
            return Err(PatternError::SameNameForAll);
        }
        Ok(NamePattern { segments })
    }

    /// The path for a song, relative to wherever it is filed. `fallback_title` stands in for an
    /// empty title (the file's current name, usually); `extension` is the file's own, kept as
    /// it is ("" for none).
    pub fn render(&self, meta: &TrackMeta, fallback_title: &str, extension: &str) -> PathBuf {
        let last = self.segments.len().saturating_sub(1);
        let mut path = PathBuf::new();
        for (index, parts) in self.segments.iter().enumerate() {
            let text: String = parts
                .iter()
                .map(|part| match part {
                    Part::Text(text) => text.clone(),
                    Part::Field(field) => field.value(meta, fallback_title),
                })
                .collect();
            let name = component(&text, &fallback_name(parts));
            if index == last && !extension.is_empty() {
                path.push(format!("{name}.{extension}"));
            } else {
                path.push(name);
            }
        }
        path
    }
}

fn flush(literal: &mut String, parts: &mut Vec<Part>) {
    if !literal.is_empty() {
        parts.push(Part::Text(std::mem::take(literal)));
    }
}

fn parse_field(inside: &str) -> Result<Field, PatternError> {
    let (names, padding) = match inside.split_once(':') {
        Some((names, padding)) => (names, Some(padding.trim())),
        None => (inside, None),
    };
    let keys: Vec<String> = names.split('|').map(|k| k.trim().to_ascii_lowercase()).collect();
    if keys.iter().any(String::is_empty) {
        return Err(PatternError::EmptyField);
    }
    if let Some(unknown) = keys.iter().find(|k| !is_field(k)) {
        return Err(PatternError::UnknownField(unknown.clone()));
    }
    let width = match padding {
        None => None,
        Some(padding) => {
            let digits = padding.strip_prefix('0').unwrap_or(padding);
            let width = digits.parse::<usize>().ok().filter(|w| (1..=MAX_PADDING).contains(w));
            match width {
                Some(width) if keys.iter().all(|k| is_number(k)) => Some(width),
                _ => return Err(PatternError::BadPadding(inside.trim().to_owned())),
            }
        }
    };
    Ok(Field { keys, width })
}

impl Field {
    fn value(&self, meta: &TrackMeta, fallback_title: &str) -> String {
        let value = self
            .keys
            .iter()
            .map(|key| match key.as_str() {
                "title" => meta.display_title(fallback_title).to_owned(),
                key => meta.get(key).unwrap_or_default(),
            })
            .find(|v| !v.trim().is_empty())
            .unwrap_or_default();
        match self.width {
            Some(width) if value.chars().all(|c| c.is_ascii_digit()) && !value.is_empty() => {
                format!("{value:0>width$}")
            }
            _ => value,
        }
    }
}

/// What a folder or file name becomes when its fields are all empty: "Unknown Artist" for
/// `{album-artist|artist}`, "Unknown Album" for `{album}`.
fn fallback_name(parts: &[Part]) -> String {
    let label = parts.iter().find_map(|p| match p {
        Part::Field(field) => field.keys.last().map(|k| label(k)),
        Part::Text(_) => None,
    });
    match label {
        Some(label) => format!("Unknown {label}"),
        None => "Unknown".to_owned(),
    }
}

/// Every field name a pattern can use.
pub fn all_keys() -> Vec<&'static str> {
    TrackMeta::CORE_KEYS.into_iter().chain(TagField::ALL.map(TagField::key)).collect()
}

fn is_field(key: &str) -> bool {
    TrackMeta::CORE_KEYS.contains(&key) || TagField::from_key(key).is_some()
}

fn is_number(key: &str) -> bool {
    matches!(key, "year" | "track" | "track-total" | "disc" | "disc-total" | "bpm")
        || TagField::from_key(key).is_some_and(|f| f.kind() == FieldKind::Number)
}

/// A field's name as people say it.
pub fn label(key: &str) -> String {
    let core = match key {
        "title" => "Title",
        "artist" => "Artist",
        "album-artist" => "Album Artist",
        "album" => "Album",
        "genre" => "Genre",
        "composer" => "Composer",
        "grouping" => "Grouping",
        "comments" => "Comments",
        "year" => "Year",
        "track" => "Track",
        "track-total" => "Track Count",
        "disc" => "Disc",
        "disc-total" => "Disc Count",
        "bpm" => "BPM",
        "compilation" => "Compilation",
        other => return TagField::from_key(other).map(|f| f.label().to_owned()).unwrap_or_else(|| other.to_owned()),
    };
    core.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NumberOf;

    fn song() -> TrackMeta {
        let mut meta = TrackMeta {
            title: "Paper Boats".into(),
            artist: "Quiet Harbour feat. Wren".into(),
            album_artist: "Quiet Harbour".into(),
            album: "Lanterns".into(),
            year: Some(1997),
            track: NumberOf::new(3, 12),
            disc: NumberOf::new(1, 2),
            ..TrackMeta::default()
        };
        meta.extra.insert(TagField::CatalogNumber, "HL-001".into());
        meta
    }

    fn render(pattern: &str, meta: &TrackMeta) -> String {
        let path = NamePattern::parse(pattern).unwrap().render(meta, "old name", "flac");
        path.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn fields_fallbacks_padding_and_folders() {
        let meta = song();
        assert_eq!(
            render("{album-artist|artist}/{album}/{track:02} {title}", &meta),
            "Quiet Harbour/Lanterns/03 Paper Boats.flac"
        );
        assert_eq!(render("{disc}-{track:3} {title}", &meta), "1-003 Paper Boats.flac");
        assert_eq!(render("[{catalogue-number}] {year}\\{title}", &meta), "[HL-001] 1997/Paper Boats.flac");
        let mut solo = meta.clone();
        solo.album_artist.clear();
        assert_eq!(render("{album-artist|artist}/{title}", &solo), "Quiet Harbour feat. Wren/Paper Boats.flac");
    }

    #[test]
    fn every_preset_parses() {
        for (name, pattern) in PRESETS {
            assert!(NamePattern::parse(pattern).is_ok(), "{name}");
        }
        for key in COMMON_FIELDS {
            assert!(is_field(key), "{key}");
        }
    }

    #[test]
    fn empty_fields_fall_back_to_readable_names() {
        let meta = TrackMeta::default();
        assert_eq!(
            render("{album-artist|artist}/{album}/{track:02} {title}", &meta),
            "Unknown Artist/Unknown Album/old name.flac",
            "no track number leaves no stray space, and the old name stands in for the title"
        );
        assert_eq!(render("{year}/{title}", &meta), "Unknown Year/old name.flac");
    }

    #[test]
    fn tags_cannot_add_folders_or_climb_out() {
        let meta = TrackMeta {
            title: "../../etc/passwd".into(),
            album: "..".into(),
            artist: "AC/DC: Live?".into(),
            ..TrackMeta::default()
        };
        assert_eq!(render("{artist}/{album}/{title}", &meta), "AC_DC_ Live_/Unknown Album/_.._etc_passwd.flac");
        let path = NamePattern::parse("{album}/{title}").unwrap().render(&meta, "x", "flac");
        assert_eq!(path.components().count(), 2);
        assert!(path.components().all(|c| matches!(c, std::path::Component::Normal(_))));
    }

    #[test]
    fn the_extension_is_kept_as_it_is() {
        let pattern = NamePattern::parse("{title}").unwrap();
        assert_eq!(pattern.render(&song(), "x", "MP3"), PathBuf::from("Paper Boats.MP3"));
        assert_eq!(pattern.render(&song(), "x", ""), PathBuf::from("Paper Boats"));
        let reserved = TrackMeta { title: "con".into(), ..TrackMeta::default() };
        assert_eq!(pattern.render(&reserved, "x", "flac"), PathBuf::from("Unknown Title.flac"));
    }

    #[test]
    fn mistakes_are_explained() {
        let error = |pattern: &str| NamePattern::parse(pattern).unwrap_err();
        assert_eq!(error(""), PatternError::Empty);
        assert_eq!(error("{title"), PatternError::Unclosed);
        assert_eq!(error("{ti{tle}"), PatternError::Unclosed);
        assert_eq!(error("title}"), PatternError::Unopened);
        assert_eq!(error("{}"), PatternError::EmptyField);
        assert_eq!(error("{artist|} {title}"), PatternError::EmptyField);
        assert_eq!(error("{colour} {title}"), PatternError::UnknownField("colour".into()));
        assert!(error("{colour} {title}").to_string().contains("catalogue-number"));
        assert_eq!(error("{title:02}"), PatternError::BadPadding("title:02".into()));
        assert_eq!(error("{track:x} {title}"), PatternError::BadPadding("track:x".into()));
        assert_eq!(error("{track:10}"), PatternError::BadPadding("track:10".into()));
        assert_eq!(error("/{title}"), PatternError::EmptyFolder);
        assert_eq!(error("{artist}//{title}"), PatternError::EmptyFolder);
        assert_eq!(error("{title}/"), PatternError::EmptyFolder);
        assert_eq!(error("{title}/{album}"), PatternError::SameNameForAll);
        assert_eq!(error("{artist} - {album}"), PatternError::SameNameForAll);
        assert!(NamePattern::parse("{TRACK:02} { Title }").is_ok(), "names are forgiving about case and spaces");
    }
}
