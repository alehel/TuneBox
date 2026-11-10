//! The columns a track list can show, what each displays, and how each sorts.
//!
//! Sorting follows the conventions listeners expect: text ignores case and a leading "The ", and
//! ties fall through to album order (album, disc, track number) so albums stay together.

use crate::{Track, format_bytes, format_clock};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// A track list column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Column {
    Name,
    Time,
    Artist,
    AlbumArtist,
    Album,
    Genre,
    Rating,
    Plays,
    Year,
    TrackNumber,
    DiscNumber,
    Composer,
    Kind,
    BitRate,
    SampleRate,
    Size,
    DateAdded,
    LastPlayed,
    Skips,
    Comments,
}

/// Ascending or descending.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

impl SortOrder {
    pub fn flipped(self) -> Self {
        match self {
            SortOrder::Ascending => SortOrder::Descending,
            SortOrder::Descending => SortOrder::Ascending,
        }
    }
}

impl Column {
    /// Every column, in the order the View Options list them.
    pub const ALL: [Column; 20] = [
        Column::Name,
        Column::Time,
        Column::Artist,
        Column::AlbumArtist,
        Column::Album,
        Column::Genre,
        Column::Rating,
        Column::Plays,
        Column::Year,
        Column::TrackNumber,
        Column::DiscNumber,
        Column::Composer,
        Column::Kind,
        Column::BitRate,
        Column::SampleRate,
        Column::Size,
        Column::DateAdded,
        Column::LastPlayed,
        Column::Skips,
        Column::Comments,
    ];

    /// The columns a new library shows.
    pub const DEFAULT_VISIBLE: [Column; 7] =
        [Column::Name, Column::Time, Column::Artist, Column::Album, Column::Genre, Column::Rating, Column::Plays];

    /// The header text.
    pub fn title(self) -> &'static str {
        match self {
            Column::Name => "Name",
            Column::Time => "Time",
            Column::Artist => "Artist",
            Column::AlbumArtist => "Album Artist",
            Column::Album => "Album",
            Column::Genre => "Genre",
            Column::Rating => "Rating",
            Column::Plays => "Plays",
            Column::Year => "Year",
            Column::TrackNumber => "Track #",
            Column::DiscNumber => "Disc #",
            Column::Composer => "Composer",
            Column::Kind => "Kind",
            Column::BitRate => "Bit Rate",
            Column::SampleRate => "Sample Rate",
            Column::Size => "Size",
            Column::DateAdded => "Date Added",
            Column::LastPlayed => "Last Played",
            Column::Skips => "Skips",
            Column::Comments => "Comments",
        }
    }

    /// Whether the column holds numbers (drawn right-aligned).
    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            Column::Time
                | Column::Plays
                | Column::Year
                | Column::TrackNumber
                | Column::DiscNumber
                | Column::BitRate
                | Column::SampleRate
                | Column::Size
                | Column::Skips
        )
    }

    /// The direction a first click on the header sorts in: counts, ratings and dates put the
    /// biggest or newest first.
    pub fn natural_order(self) -> SortOrder {
        match self {
            Column::Rating | Column::Plays | Column::Skips | Column::DateAdded | Column::LastPlayed => {
                SortOrder::Descending
            }
            _ => SortOrder::Ascending,
        }
    }

    /// The cell text for `track`. Ratings are drawn as stars by the UI; this is their text form.
    pub fn text(self, track: &Track) -> String {
        let meta = &track.meta;
        let some_or_blank = |value: Option<u32>| value.map(|v| v.to_string()).unwrap_or_default();
        match self {
            Column::Name => track.title().to_owned(),
            Column::Time => format_clock(track.file.duration),
            Column::Artist => meta.artist.clone(),
            Column::AlbumArtist => meta.album_artist.clone(),
            Column::Album => meta.album.clone(),
            Column::Genre => meta.genre.clone(),
            Column::Rating => {
                if track.stats.rating.is_rated() {
                    track.stats.rating.glyphs()
                } else {
                    String::new()
                }
            }
            Column::Plays => {
                if track.stats.play_count > 0 {
                    track.stats.play_count.to_string()
                } else {
                    String::new()
                }
            }
            Column::Year => some_or_blank(meta.year),
            Column::TrackNumber => meta.track.label(),
            Column::DiscNumber => meta.disc.label(),
            Column::Composer => meta.composer.clone(),
            Column::Kind => track.file.format.kind_label().to_owned(),
            Column::BitRate => track.file.bit_rate_kbps.map(|k| format!("{k} kbps")).unwrap_or_default(),
            Column::SampleRate => {
                track.file.sample_rate.map(|r| format!("{:.1} kHz", f64::from(r) / 1000.0)).unwrap_or_default()
            }
            Column::Size => format_bytes(track.file.size_bytes),
            Column::DateAdded => track.date_added.format("%Y-%m-%d %H:%M").to_string(),
            Column::LastPlayed => {
                track.stats.last_played.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
            }
            Column::Skips => {
                if track.stats.skip_count > 0 {
                    track.stats.skip_count.to_string()
                } else {
                    String::new()
                }
            }
            Column::Comments => meta.comments.clone(),
        }
    }

    /// Compares two tracks by this column alone, ascending.
    fn compare_primary(self, a: &Track, b: &Track) -> Ordering {
        let (ma, mb) = (&a.meta, &b.meta);
        match self {
            Column::Name => compare_text(a.title(), b.title()),
            Column::Time => a.file.duration.cmp(&b.file.duration),
            Column::Artist => compare_text(&ma.artist, &mb.artist),
            Column::AlbumArtist => compare_text(ma.filing_artist(), mb.filing_artist()),
            Column::Album => compare_text(&ma.album, &mb.album),
            Column::Genre => compare_text(&ma.genre, &mb.genre),
            Column::Rating => a.stats.rating.cmp(&b.stats.rating),
            Column::Plays => a.stats.play_count.cmp(&b.stats.play_count),
            Column::Year => ma.year.cmp(&mb.year),
            Column::TrackNumber => ma.track.number.cmp(&mb.track.number),
            Column::DiscNumber => ma.disc.number.cmp(&mb.disc.number),
            Column::Composer => compare_text(&ma.composer, &mb.composer),
            Column::Kind => a.file.format.kind_label().cmp(b.file.format.kind_label()),
            Column::BitRate => a.file.bit_rate_kbps.cmp(&b.file.bit_rate_kbps),
            Column::SampleRate => a.file.sample_rate.cmp(&b.file.sample_rate),
            Column::Size => a.file.size_bytes.cmp(&b.file.size_bytes),
            Column::DateAdded => a.date_added.cmp(&b.date_added),
            Column::LastPlayed => a.stats.last_played.cmp(&b.stats.last_played),
            Column::Skips => a.stats.skip_count.cmp(&b.stats.skip_count),
            Column::Comments => compare_text(&ma.comments, &mb.comments),
        }
    }
}

/// Album order: filing artist, album, disc, track number, then title.
fn compare_album_order(a: &Track, b: &Track) -> Ordering {
    let (ma, mb) = (&a.meta, &b.meta);
    compare_text(ma.filing_artist(), mb.filing_artist())
        .then_with(|| compare_text(&ma.album, &mb.album))
        .then_with(|| ma.disc.number.cmp(&mb.disc.number))
        .then_with(|| ma.track.number.cmp(&mb.track.number))
        .then_with(|| compare_text(a.title(), b.title()))
}

/// Compares two tracks for a list sorted by `column` in `order`. Ties keep albums together and
/// are always broken by id, so sorting is total and stable across runs.
pub fn compare_tracks(a: &Track, b: &Track, column: Column, order: SortOrder) -> Ordering {
    let primary = column.compare_primary(a, b);
    let primary = match order {
        SortOrder::Ascending => primary,
        SortOrder::Descending => primary.reverse(),
    };
    primary.then_with(|| compare_album_order(a, b)).then_with(|| a.id.cmp(&b.id))
}

/// Case-insensitive comparison that files "The Beatles" under B and puts blanks last.
fn compare_text(a: &str, b: &str) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    let key = |s: &str| sort_key(s).to_lowercase();
    key(a).cmp(&key(b))
}

/// The part of a name that sorting looks at: without a leading English article.
fn sort_key(s: &str) -> &str {
    let trimmed = s.trim_start();
    for article in ["The ", "the ", "A ", "An "] {
        if let Some(rest) = trimmed.strip_prefix(article)
            && !rest.is_empty()
        {
            return rest;
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::fixtures::track;

    #[test]
    fn artist_sort_ignores_case_and_the() {
        let a = track(1, "x", "The Beatles", "Abbey");
        let b = track(2, "x", "blur", "Parklife");
        assert_eq!(compare_tracks(&a, &b, Column::Artist, SortOrder::Ascending), Ordering::Less);
    }

    #[test]
    fn ties_fall_back_to_album_order() {
        let mut first = track(2, "Zebra", "A", "Album");
        first.meta.track.number = Some(1);
        let mut second = track(1, "Apple", "A", "Album");
        second.meta.track.number = Some(2);
        assert_eq!(compare_tracks(&first, &second, Column::Artist, SortOrder::Ascending), Ordering::Less);
    }

    #[test]
    fn blanks_sort_last() {
        let blank = track(1, "x", "", "a");
        let named = track(2, "x", "Zed", "a");
        assert_eq!(compare_tracks(&named, &blank, Column::Artist, SortOrder::Ascending), Ordering::Less);
    }

    #[test]
    fn descending_reverses_only_the_primary_key() {
        let mut low = track(1, "a", "A", "X");
        low.stats.play_count = 1;
        let mut high = track(2, "b", "A", "X");
        high.stats.play_count = 9;
        assert_eq!(compare_tracks(&high, &low, Column::Plays, SortOrder::Descending), Ordering::Less);
    }
}
