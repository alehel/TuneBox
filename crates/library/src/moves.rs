//! Recognising a song that moved. A file the library does not know may be one it knows under an
//! old path: a folder renamed or reorganised outside TuneBox. Relinking it keeps its ratings,
//! play counts and playlists, where adding it again would make a duplicate and leave the old
//! entry pointing at nothing.

use crate::{Library, NewTrack};
use std::collections::HashMap;
use std::time::Duration;
use tunebox_model::{AudioFormat, Track, TrackId};

/// What a moved file still shares with its old self: the same bytes, so the same format, size
/// and length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Fingerprint {
    format: AudioFormat,
    size_bytes: u64,
    duration: Duration,
}

impl Fingerprint {
    fn of(file: &tunebox_model::FileInfo) -> Self {
        Fingerprint { format: file.format, size_bytes: file.size_bytes, duration: file.duration }
    }
}

/// The library's songs by fingerprint, so finding a moved song costs one lookup, not a pass over
/// the whole library for every new file.
#[derive(Debug, Default)]
pub struct MovedSongs {
    by_fingerprint: HashMap<Fingerprint, Vec<TrackId>>,
}

impl MovedSongs {
    pub fn index(library: &Library) -> Self {
        let mut by_fingerprint: HashMap<Fingerprint, Vec<TrackId>> = HashMap::new();
        for track in library.tracks() {
            by_fingerprint.entry(Fingerprint::of(&track.file)).or_default().push(track.id);
        }
        MovedSongs { by_fingerprint }
    }

    /// The song `new` was moved from: one with the same fingerprint and the same title, artist
    /// and album, whose own file `is_gone`. Only those few candidates are asked about. The
    /// caller decides what gone means; a song on a share that is not connected is not gone.
    pub fn find(&self, library: &Library, new: &NewTrack, is_gone: impl Fn(&Track) -> bool) -> Option<TrackId> {
        let candidates = self.by_fingerprint.get(&Fingerprint::of(&new.file))?;
        candidates
            .iter()
            .filter_map(|id| library.track(*id))
            .filter(|old| old.location != new.location && same_song(old, new))
            .find(|old| is_gone(old))
            .map(|old| old.id)
    }
}

fn same_song(old: &Track, new: &NewTrack) -> bool {
    old.meta.title == new.meta.title && old.meta.artist == new.meta.artist && old.meta.album == new.meta.album
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;
    use tunebox_model::{FileInfo, TrackMeta};

    fn song(location: &str, title: &str, size_bytes: u64) -> NewTrack {
        NewTrack {
            location: PathBuf::from(location),
            meta: TrackMeta { title: title.into(), artist: "Quiet Harbour".into(), ..TrackMeta::default() },
            file: FileInfo {
                format: AudioFormat::Flac,
                duration: Duration::from_secs(200),
                size_bytes,
                bit_rate_kbps: None,
                sample_rate: None,
                channels: None,
            },
            date_added: Utc::now(),
            kind: tunebox_model::MediaKind::Music,
        }
    }

    #[test]
    fn a_moved_song_is_found_only_when_its_old_file_is_gone_and_everything_matches() {
        let mut library = Library::default();
        let old = library.add_track(song("/old/a.flac", "First Light", 1_000));
        library.add_track(song("/old/b.flac", "Second Light", 2_000));
        let moved = MovedSongs::index(&library);
        let gone = |old: &Track| old.location.starts_with("/old");

        assert_eq!(moved.find(&library, &song("/new/a.flac", "First Light", 1_000), gone), Some(old));
        assert_eq!(
            moved.find(&library, &song("/new/a.flac", "First Light", 1_000), |_: &Track| false),
            None,
            "still there: a copy"
        );
        assert_eq!(moved.find(&library, &song("/new/a.flac", "First Light", 1_001), gone), None, "different bytes");
        assert_eq!(moved.find(&library, &song("/new/a.flac", "Other Title", 1_000), gone), None, "different song");
    }
}
