//! A track: one audio file, what it contains, and how it has been listened to.

use crate::{AudioFormat, MediaKind, Rating, TrackId, TrackMeta};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Technical facts about the file, read when it is imported.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    pub format: AudioFormat,
    #[serde(with = "duration_millis")]
    pub duration: Duration,
    pub size_bytes: u64,
    pub bit_rate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
}

/// How a track has been used.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayStats {
    pub play_count: u32,
    pub skip_count: u32,
    pub last_played: Option<DateTime<Utc>>,
    pub last_skipped: Option<DateTime<Utc>>,
    pub rating: Rating,
}

/// One entry in the library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    /// Absolute path of the audio file.
    pub location: PathBuf,
    pub meta: TrackMeta,
    pub file: FileInfo,
    #[serde(default)]
    pub stats: PlayStats,
    pub date_added: DateTime<Utc>,
    /// The checkbox in the track list: unchecked tracks are skipped by playback and sync.
    #[serde(default = "checked_default")]
    pub checked: bool,
    /// Whether the additional tag fields (`TrackMeta::extra`) have been read from the file.
    /// False for songs added before TuneBox read them; the next scan or import that finds the
    /// file reads them (see `TrackMeta::fill_unread_extra`).
    #[serde(default)]
    pub extra_fields_read: bool,
    /// The details were edited in TuneBox but not written to the file: it is in a read-only
    /// library folder, or writing failed. Refreshing from the file would undo those edits, so
    /// it asks first.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub library_only_edits: bool,
    /// Music or an audiobook. Added after version 1 of the library file; absent means music.
    #[serde(default, skip_serializing_if = "MediaKind::is_music")]
    pub kind: MediaKind,
}

fn checked_default() -> bool {
    true
}

impl Track {
    /// The title to show, falling back to the file name for untagged files.
    pub fn title(&self) -> &str {
        let stem = self.location.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        self.meta.display_title(stem)
    }

    /// Records a complete play at `when`.
    pub fn record_play(&mut self, when: DateTime<Utc>) {
        self.stats.play_count = self.stats.play_count.saturating_add(1);
        self.stats.last_played = Some(when);
    }

    /// Records a skip (the listener moved on early) at `when`.
    pub fn record_skip(&mut self, when: DateTime<Utc>) {
        self.stats.skip_count = self.stats.skip_count.saturating_add(1);
        self.stats.last_skipped = Some(when);
    }
}

/// Durations are stored as whole milliseconds, which reads well in the library file.
mod duration_millis {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_millis(u64::deserialize(d)?))
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use chrono::TimeZone;

    /// A small, fully tagged track for tests across the crate.
    pub fn track(id: u64, title: &str, artist: &str, album: &str) -> Track {
        Track {
            id: TrackId(id),
            location: PathBuf::from(format!("/music/{artist}/{album}/{title}.flac")),
            meta: TrackMeta {
                title: title.into(),
                artist: artist.into(),
                album: album.into(),
                genre: "Rock".into(),
                ..TrackMeta::default()
            },
            file: FileInfo {
                format: AudioFormat::Flac,
                duration: Duration::from_secs(200),
                size_bytes: 20_000_000,
                bit_rate_kbps: Some(800),
                sample_rate: Some(44_100),
                channels: Some(2),
            },
            stats: PlayStats::default(),
            date_added: Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).single().unwrap_or_default(),
            checked: true,
            extra_fields_read: true,
            library_only_edits: false,
            kind: MediaKind::Music,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::track;
    use super::*;

    #[test]
    fn title_falls_back_to_file_name() {
        let mut t = track(1, "", "A", "B");
        t.location = PathBuf::from("/x/03 Untitled Jam.mp3");
        assert_eq!(t.title(), "03 Untitled Jam");
    }

    #[test]
    fn serializes_duration_as_millis() {
        let t = track(1, "Song", "A", "B");
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["file"]["duration"], 200_000);
        let back: Track = serde_json::from_value(json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn songs_saved_before_extra_fields_were_read_are_marked_unread() {
        let mut json = serde_json::to_value(track(1, "Song", "A", "B")).unwrap();
        json.as_object_mut().unwrap().remove("extra_fields_read");
        let old: Track = serde_json::from_value(json).unwrap();
        assert!(!old.extra_fields_read);
    }

    #[test]
    fn play_counts_saturate() {
        let mut t = track(1, "Song", "A", "B");
        t.stats.play_count = u32::MAX;
        t.record_play(t.date_added);
        assert_eq!(t.stats.play_count, u32::MAX);
    }
}
