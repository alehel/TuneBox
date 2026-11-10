//! The tag fields beyond the everyday ones: credits, classical work and movement, release
//! details, sort names, mood and key, lyrics, encoding. Get Info shows the everyday fields by
//! default and these on request; the listener can pin any of them to always show.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// An additional tag field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TagField {
    Subtitle,
    Lyricist,
    Conductor,
    Arranger,
    Remixer,
    Producer,
    Engineer,
    Performer,
    Work,
    Movement,
    MovementNumber,
    MovementTotal,
    Label,
    CatalogNumber,
    Barcode,
    ReleaseDate,
    OriginalReleaseDate,
    MediaType,
    Copyright,
    Isrc,
    SortName,
    SortArtist,
    SortAlbum,
    SortAlbumArtist,
    SortComposer,
    Mood,
    Key,
    Language,
    Lyrics,
    EncodedBy,
    EncoderSettings,
}

/// What a field's value looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// One line of free text.
    Text,
    /// Several lines of free text.
    Lines,
    /// A whole number.
    Number,
    /// A year, year and month, or full date: `2011`, `2011-04`, `2011-04-02`.
    Date,
}

/// How the fields are grouped on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldGroup {
    Credits,
    Classical,
    Release,
    Sorting,
    Character,
    Lyrics,
    Encoding,
}

impl FieldGroup {
    pub const ALL: [FieldGroup; 7] = [
        FieldGroup::Credits,
        FieldGroup::Classical,
        FieldGroup::Release,
        FieldGroup::Sorting,
        FieldGroup::Character,
        FieldGroup::Lyrics,
        FieldGroup::Encoding,
    ];

    pub fn title(self) -> &'static str {
        match self {
            FieldGroup::Credits => "Credits",
            FieldGroup::Classical => "Classical",
            FieldGroup::Release => "Release",
            FieldGroup::Sorting => "Sorting",
            FieldGroup::Character => "Mood and Key",
            FieldGroup::Lyrics => "Lyrics",
            FieldGroup::Encoding => "Encoding",
        }
    }

    /// One line on what the group is for, where that is not obvious.
    pub fn description(self) -> &'static str {
        match self {
            FieldGroup::Classical => "For works in several movements.",
            FieldGroup::Sorting => "How names sort, when not as written (\"Beatles, The\").",
            _ => "",
        }
    }

    /// The group's fields, in display order.
    pub fn fields(self) -> impl Iterator<Item = TagField> {
        TagField::ALL.into_iter().filter(move |f| f.group() == self)
    }
}

impl TagField {
    pub const ALL: [TagField; 31] = [
        TagField::Subtitle,
        TagField::Lyricist,
        TagField::Conductor,
        TagField::Arranger,
        TagField::Remixer,
        TagField::Producer,
        TagField::Engineer,
        TagField::Performer,
        TagField::Work,
        TagField::Movement,
        TagField::MovementNumber,
        TagField::MovementTotal,
        TagField::Label,
        TagField::CatalogNumber,
        TagField::Barcode,
        TagField::ReleaseDate,
        TagField::OriginalReleaseDate,
        TagField::MediaType,
        TagField::Copyright,
        TagField::Isrc,
        TagField::SortName,
        TagField::SortArtist,
        TagField::SortAlbum,
        TagField::SortAlbumArtist,
        TagField::SortComposer,
        TagField::Mood,
        TagField::Key,
        TagField::Language,
        TagField::Lyrics,
        TagField::EncodedBy,
        TagField::EncoderSettings,
    ];

    /// The name shown beside the field.
    pub fn label(self) -> &'static str {
        match self {
            TagField::Subtitle => "Subtitle",
            TagField::Lyricist => "Lyricist",
            TagField::Conductor => "Conductor",
            TagField::Arranger => "Arranger",
            TagField::Remixer => "Remixer",
            TagField::Producer => "Producer",
            TagField::Engineer => "Engineer",
            TagField::Performer => "Performer",
            TagField::Work => "Work",
            TagField::Movement => "Movement",
            TagField::MovementNumber => "Movement number",
            TagField::MovementTotal => "Movements",
            TagField::Label => "Label",
            TagField::CatalogNumber => "Catalogue number",
            TagField::Barcode => "Barcode",
            TagField::ReleaseDate => "Release date",
            TagField::OriginalReleaseDate => "Original date",
            TagField::MediaType => "Media",
            TagField::Copyright => "Copyright",
            TagField::Isrc => "ISRC",
            TagField::SortName => "Sort name",
            TagField::SortArtist => "Sort artist",
            TagField::SortAlbum => "Sort album",
            TagField::SortAlbumArtist => "Sort album artist",
            TagField::SortComposer => "Sort composer",
            TagField::Mood => "Mood",
            TagField::Key => "Key",
            TagField::Language => "Language",
            TagField::Lyrics => "Lyrics",
            TagField::EncodedBy => "Encoded by",
            TagField::EncoderSettings => "Encoder settings",
        }
    }

    /// The name used on the command line, e.g. `catalogue-number`.
    pub fn key(self) -> &'static str {
        match self {
            TagField::Subtitle => "subtitle",
            TagField::Lyricist => "lyricist",
            TagField::Conductor => "conductor",
            TagField::Arranger => "arranger",
            TagField::Remixer => "remixer",
            TagField::Producer => "producer",
            TagField::Engineer => "engineer",
            TagField::Performer => "performer",
            TagField::Work => "work",
            TagField::Movement => "movement",
            TagField::MovementNumber => "movement-number",
            TagField::MovementTotal => "movement-total",
            TagField::Label => "label",
            TagField::CatalogNumber => "catalogue-number",
            TagField::Barcode => "barcode",
            TagField::ReleaseDate => "release-date",
            TagField::OriginalReleaseDate => "original-date",
            TagField::MediaType => "media",
            TagField::Copyright => "copyright",
            TagField::Isrc => "isrc",
            TagField::SortName => "sort-name",
            TagField::SortArtist => "sort-artist",
            TagField::SortAlbum => "sort-album",
            TagField::SortAlbumArtist => "sort-album-artist",
            TagField::SortComposer => "sort-composer",
            TagField::Mood => "mood",
            TagField::Key => "key",
            TagField::Language => "language",
            TagField::Lyrics => "lyrics",
            TagField::EncodedBy => "encoded-by",
            TagField::EncoderSettings => "encoder-settings",
        }
    }

    pub fn from_key(key: &str) -> Option<TagField> {
        TagField::ALL.into_iter().find(|f| f.key() == key)
    }

    pub fn group(self) -> FieldGroup {
        match self {
            TagField::Subtitle
            | TagField::Lyricist
            | TagField::Conductor
            | TagField::Arranger
            | TagField::Remixer
            | TagField::Producer
            | TagField::Engineer
            | TagField::Performer => FieldGroup::Credits,
            TagField::Work | TagField::Movement | TagField::MovementNumber | TagField::MovementTotal => {
                FieldGroup::Classical
            }
            TagField::Label
            | TagField::CatalogNumber
            | TagField::Barcode
            | TagField::ReleaseDate
            | TagField::OriginalReleaseDate
            | TagField::MediaType
            | TagField::Copyright
            | TagField::Isrc => FieldGroup::Release,
            TagField::SortName
            | TagField::SortArtist
            | TagField::SortAlbum
            | TagField::SortAlbumArtist
            | TagField::SortComposer => FieldGroup::Sorting,
            TagField::Mood | TagField::Key | TagField::Language => FieldGroup::Character,
            TagField::Lyrics => FieldGroup::Lyrics,
            TagField::EncodedBy | TagField::EncoderSettings => FieldGroup::Encoding,
        }
    }

    pub fn kind(self) -> FieldKind {
        match self {
            TagField::Lyrics => FieldKind::Lines,
            TagField::MovementNumber | TagField::MovementTotal => FieldKind::Number,
            TagField::ReleaseDate | TagField::OriginalReleaseDate => FieldKind::Date,
            _ => FieldKind::Text,
        }
    }

    /// Whether `value` suits the field. Empty always does: it clears the field. Tag formats
    /// keep dates and numbers in typed frames, and a malformed one can make a file unreadable
    /// to other players, so those are checked before anything is written.
    pub fn accepts(self, value: &str) -> bool {
        let value = value.trim();
        if value.is_empty() {
            return true;
        }
        match self.kind() {
            FieldKind::Text | FieldKind::Lines => true,
            FieldKind::Number => value.parse::<u32>().is_ok(),
            FieldKind::Date => is_date(value),
        }
    }

    /// What to type, for fields that expect a particular form.
    pub fn hint(self) -> &'static str {
        match self.kind() {
            FieldKind::Date => "YYYY-MM-DD",
            FieldKind::Number => "a whole number",
            FieldKind::Text | FieldKind::Lines => "",
        }
    }
}

/// `2011`, `2011-04` or `2011-04-02`, and a real date.
fn is_date(value: &str) -> bool {
    let mut parts = value.split('-');
    let year = parts.next().unwrap_or_default();
    let month = parts.next().unwrap_or("01");
    let day = parts.next().unwrap_or("01");
    let digits = |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    if parts.next().is_some() || !digits(year, 4) || !digits(month, 2) || !digits(day, 2) {
        return false;
    }
    NaiveDate::parse_from_str(&format!("{year}-{month}-{day}"), "%Y-%m-%d").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_field_is_listed_once_and_in_exactly_one_group() {
        let unique: HashSet<_> = TagField::ALL.iter().collect();
        assert_eq!(unique.len(), TagField::ALL.len());
        // Sorting fields must give the display order: preferences rely on it.
        assert!(TagField::ALL.is_sorted());
        let grouped: usize = FieldGroup::ALL.iter().map(|g| g.fields().count()).sum();
        assert_eq!(grouped, TagField::ALL.len());
    }

    #[test]
    fn dates_and_numbers_are_checked_and_empty_always_clears() {
        let date = TagField::ReleaseDate;
        for good in ["2011", "2011-04", "2011-04-02", " 1999-12-31 ", ""] {
            assert!(date.accepts(good), "{good}");
        }
        for bad in ["11", "2011-4-2", "2011-02-30", "April 2011", "2011-04-02-01", "2011-04-02T10:00"] {
            assert!(!date.accepts(bad), "{bad}");
        }
        assert!(TagField::MovementNumber.accepts("3"));
        assert!(!TagField::MovementNumber.accepts("III"));
        assert!(TagField::Mood.accepts("anything at all"));
    }

    #[test]
    fn keys_round_trip_and_are_unique() {
        let keys: HashSet<_> = TagField::ALL.iter().map(|f| f.key()).collect();
        assert_eq!(keys.len(), TagField::ALL.len());
        for field in TagField::ALL {
            assert_eq!(TagField::from_key(field.key()), Some(field));
        }
    }
}
