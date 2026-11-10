//! The collection itself and every change that can be made to it.

use crate::LibraryError;
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tunebox_model::{
    FileInfo, PlayStats, Playlist, PlaylistId, PlaylistKind, Rating, SmartRules, TagField, Track, TrackId, TrackMeta,
};

/// A file to add: everything about a track except what the library assigns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTrack {
    pub location: PathBuf,
    pub meta: TrackMeta,
    pub file: FileInfo,
    pub date_added: DateTime<Utc>,
}

/// The music library.
#[derive(Clone, Debug, Default)]
pub struct Library {
    tracks: BTreeMap<TrackId, Track>,
    playlists: Vec<Playlist>,
    next_id: u64,
    /// Which track each file belongs to, so a file is never added twice.
    by_location: HashMap<PathBuf, TrackId>,
}

impl Library {
    /// An empty library with no playlists.
    pub fn new() -> Self {
        Self { next_id: 1, ..Self::default() }
    }

    /// An empty library with the standard smart playlists, as a first launch creates.
    pub fn with_default_playlists() -> Self {
        let mut library = Self::new();
        for (name, rules) in tunebox_model::smart::presets() {
            library.push_playlist(name.to_owned(), PlaylistKind::Smart(rules));
        }
        library
    }

    /// Rebuilds a library from stored parts, repairing what a damaged or hand-edited file could
    /// break: duplicate ids or locations keep the first entry, playlists lose references to
    /// tracks that do not exist, and the id counter moves past every id in use.
    pub fn from_parts(tracks: Vec<Track>, playlists: Vec<Playlist>, next_id: u64) -> Self {
        let mut library = Self { next_id: next_id.max(1), ..Self::default() };
        for track in tracks {
            if library.tracks.contains_key(&track.id) || library.by_location.contains_key(&track.location) {
                continue;
            }
            library.next_id = library.next_id.max(track.id.0.saturating_add(1));
            library.by_location.insert(track.location.clone(), track.id);
            library.tracks.insert(track.id, track);
        }
        let mut seen = std::collections::HashSet::new();
        for mut playlist in playlists {
            if !seen.insert(playlist.id) {
                continue;
            }
            library.next_id = library.next_id.max(playlist.id.0.saturating_add(1));
            if let PlaylistKind::Manual { tracks } = &mut playlist.kind {
                tracks.retain(|id| library.tracks.contains_key(id));
            }
            library.playlists.push(playlist);
        }
        library
    }

    /// The id counter, for the library file.
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    // ----- tracks -----

    /// Every track, in id order.
    pub fn tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.values()
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.get(&id)
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// The track that holds the file at `location`, if any.
    pub fn track_at(&self, location: &Path) -> Option<TrackId> {
        self.by_location.get(location).copied()
    }

    /// Adds a file. A file already in the library is not added twice: its metadata and file
    /// facts are refreshed and its existing id returned, keeping its play history.
    pub fn add_track(&mut self, new: NewTrack) -> TrackId {
        if let Some(id) = self.track_at(&new.location)
            && let Some(existing) = self.tracks.get_mut(&id)
        {
            existing.meta = new.meta;
            existing.file = new.file;
            return id;
        }
        let id = TrackId(self.allocate_id());
        self.by_location.insert(new.location.clone(), id);
        let track = Track {
            id,
            location: new.location,
            meta: new.meta,
            file: new.file,
            stats: PlayStats::default(),
            date_added: new.date_added,
            checked: true,
            // New tracks come from reading the file (or writing it, for CD imports).
            extra_fields_read: true,
        };
        self.tracks.insert(id, track);
        id
    }

    /// Removes tracks from the library (not from disk) and from every playlist. Returns the
    /// removed tracks so the caller can delete their files if asked to.
    pub fn remove_tracks(&mut self, ids: &[TrackId]) -> Vec<Track> {
        let removed: Vec<Track> = ids.iter().filter_map(|id| self.tracks.remove(id)).collect();
        for track in &removed {
            self.by_location.remove(&track.location);
        }
        for playlist in &mut self.playlists {
            if let PlaylistKind::Manual { tracks } = &mut playlist.kind {
                tracks.retain(|id| !ids.contains(id));
            }
        }
        removed
    }

    fn track_mut(&mut self, id: TrackId) -> Result<&mut Track, LibraryError> {
        self.tracks.get_mut(&id).ok_or(LibraryError::NoSuchTrack(id))
    }

    pub fn set_meta(&mut self, id: TrackId, meta: TrackMeta) -> Result<(), LibraryError> {
        self.track_mut(id)?.meta = meta;
        Ok(())
    }

    /// Fills in the additional fields read from the track's file (see
    /// `TrackMeta::fill_unread_extra`) and marks them read.
    pub fn fill_unread_extra(
        &mut self,
        id: TrackId,
        from_file: BTreeMap<TagField, String>,
    ) -> Result<(), LibraryError> {
        let track = self.track_mut(id)?;
        track.meta.fill_unread_extra(from_file);
        track.extra_fields_read = true;
        Ok(())
    }

    pub fn set_rating(&mut self, id: TrackId, rating: Rating) -> Result<(), LibraryError> {
        self.track_mut(id)?.stats.rating = rating;
        Ok(())
    }

    pub fn set_checked(&mut self, id: TrackId, checked: bool) -> Result<(), LibraryError> {
        self.track_mut(id)?.checked = checked;
        Ok(())
    }

    pub fn record_play(&mut self, id: TrackId, when: DateTime<Utc>) -> Result<(), LibraryError> {
        self.track_mut(id)?.record_play(when);
        Ok(())
    }

    pub fn record_skip(&mut self, id: TrackId, when: DateTime<Utc>) -> Result<(), LibraryError> {
        self.track_mut(id)?.record_skip(when);
        Ok(())
    }

    /// Records that a track's file moved (for example when the media folder is reorganised).
    pub fn relocate(&mut self, id: TrackId, location: PathBuf) -> Result<(), LibraryError> {
        let track = self.tracks.get_mut(&id).ok_or(LibraryError::NoSuchTrack(id))?;
        self.by_location.remove(&track.location);
        self.by_location.insert(location.clone(), id);
        track.location = location;
        Ok(())
    }

    // ----- playlists -----

    pub fn playlists(&self) -> &[Playlist] {
        &self.playlists
    }

    pub fn playlist(&self, id: PlaylistId) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.id == id)
    }

    fn playlist_mut(&mut self, id: PlaylistId) -> Result<&mut Playlist, LibraryError> {
        self.playlists.iter_mut().find(|p| p.id == id).ok_or(LibraryError::NoSuchPlaylist(id))
    }

    fn push_playlist(&mut self, name: String, kind: PlaylistKind) -> PlaylistId {
        let id = PlaylistId(self.allocate_id());
        self.playlists.push(Playlist { id, name, kind });
        id
    }

    /// Creates an empty playlist.
    pub fn create_playlist(&mut self, name: &str) -> Result<PlaylistId, LibraryError> {
        let name = checked_name(name)?;
        Ok(self.push_playlist(name, PlaylistKind::Manual { tracks: Vec::new() }))
    }

    /// Creates a smart playlist.
    pub fn create_smart_playlist(&mut self, name: &str, rules: SmartRules) -> Result<PlaylistId, LibraryError> {
        let name = checked_name(name)?;
        Ok(self.push_playlist(name, PlaylistKind::Smart(rules)))
    }

    pub fn rename_playlist(&mut self, id: PlaylistId, name: &str) -> Result<(), LibraryError> {
        let name = checked_name(name)?;
        self.playlist_mut(id)?.name = name;
        Ok(())
    }

    pub fn delete_playlist(&mut self, id: PlaylistId) -> Result<Playlist, LibraryError> {
        let index = self.playlists.iter().position(|p| p.id == id).ok_or(LibraryError::NoSuchPlaylist(id))?;
        Ok(self.playlists.remove(index))
    }

    /// Replaces a smart playlist's rules.
    pub fn set_smart_rules(&mut self, id: PlaylistId, rules: SmartRules) -> Result<(), LibraryError> {
        let playlist = self.playlist_mut(id)?;
        match &mut playlist.kind {
            PlaylistKind::Smart(existing) => {
                *existing = rules;
                Ok(())
            }
            PlaylistKind::Manual { .. } => Err(LibraryError::NotSmart(playlist.name.clone())),
        }
    }

    /// Appends tracks to a playlist. Ids that are not in the library are ignored.
    pub fn add_to_playlist(&mut self, id: PlaylistId, track_ids: &[TrackId]) -> Result<(), LibraryError> {
        let existing: Vec<TrackId> = track_ids.iter().copied().filter(|t| self.tracks.contains_key(t)).collect();
        let tracks = self.manual_tracks_mut(id)?;
        tracks.extend(existing);
        Ok(())
    }

    /// Removes the entries at `positions` (indexes into the playlist, so duplicates can be
    /// removed one at a time). Out-of-range positions are ignored.
    pub fn remove_from_playlist(&mut self, id: PlaylistId, positions: &[usize]) -> Result<(), LibraryError> {
        let tracks = self.manual_tracks_mut(id)?;
        let mut index = 0;
        tracks.retain(|_| {
            let keep = !positions.contains(&index);
            index += 1;
            keep
        });
        Ok(())
    }

    /// Moves the entries at `positions` so they start at `target` (an index in the playlist
    /// before the move), keeping their relative order: drag-and-drop reordering.
    pub fn move_in_playlist(&mut self, id: PlaylistId, positions: &[usize], target: usize) -> Result<(), LibraryError> {
        let tracks = self.manual_tracks_mut(id)?;
        let mut moved = Vec::new();
        let mut kept = Vec::new();
        let mut insert_at = 0;
        for (index, track) in tracks.iter().enumerate() {
            if positions.contains(&index) {
                moved.push(*track);
            } else {
                if index < target {
                    insert_at += 1;
                }
                kept.push(*track);
            }
        }
        let insert_at = insert_at.min(kept.len());
        kept.splice(insert_at..insert_at, moved);
        *tracks = kept;
        Ok(())
    }

    fn manual_tracks_mut(&mut self, id: PlaylistId) -> Result<&mut Vec<TrackId>, LibraryError> {
        let playlist = self.playlist_mut(id)?;
        match &mut playlist.kind {
            PlaylistKind::Manual { tracks } => Ok(tracks),
            PlaylistKind::Smart(_) => Err(LibraryError::SmartPlaylist(playlist.name.clone())),
        }
    }

    /// A playlist's tracks in playlist order. Smart playlists are evaluated against the library
    /// as it is now.
    pub fn playlist_tracks(&self, id: PlaylistId, now: DateTime<Utc>) -> Result<Vec<TrackId>, LibraryError> {
        let playlist = self.playlist(id).ok_or(LibraryError::NoSuchPlaylist(id))?;
        Ok(match &playlist.kind {
            PlaylistKind::Manual { tracks } => tracks.clone(),
            PlaylistKind::Smart(rules) => rules.evaluate(self.tracks.values(), now),
        })
    }
}

fn checked_name(name: &str) -> Result<String, LibraryError> {
    let trimmed = name.trim();
    if trimmed.is_empty() { Err(LibraryError::EmptyName) } else { Ok(trimmed.to_owned()) }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::time::Duration;
    use tunebox_model::AudioFormat;

    pub fn new_track(path: &str, title: &str, artist: &str, album: &str) -> NewTrack {
        NewTrack {
            location: PathBuf::from(path),
            meta: TrackMeta {
                title: title.into(),
                artist: artist.into(),
                album: album.into(),
                genre: "Pop".into(),
                ..TrackMeta::default()
            },
            file: FileInfo {
                format: AudioFormat::Mp3,
                duration: Duration::from_secs(180),
                size_bytes: 4_000_000,
                bit_rate_kbps: Some(192),
                sample_rate: Some(44_100),
                channels: Some(2),
            },
            date_added: DateTime::<Utc>::UNIX_EPOCH,
        }
    }

    #[test]
    fn a_file_is_added_once() {
        let mut lib = Library::new();
        let first = lib.add_track(new_track("/a.mp3", "One", "A", "X"));
        lib.record_play(first, Utc::now()).unwrap();
        let again = lib.add_track(new_track("/a.mp3", "One (Remaster)", "A", "X"));
        assert_eq!(first, again);
        assert_eq!(lib.len(), 1);
        let track = lib.track(first).unwrap();
        assert_eq!(track.meta.title, "One (Remaster)");
        assert_eq!(track.stats.play_count, 1, "re-adding keeps play history");
    }

    #[test]
    fn removing_tracks_removes_them_from_playlists() {
        let mut lib = Library::new();
        let a = lib.add_track(new_track("/a.mp3", "One", "A", "X"));
        let b = lib.add_track(new_track("/b.mp3", "Two", "A", "X"));
        let list = lib.create_playlist("Mix").unwrap();
        lib.add_to_playlist(list, &[a, b, a]).unwrap();
        lib.remove_tracks(&[a]);
        assert_eq!(lib.playlist_tracks(list, Utc::now()).unwrap(), [b]);
        assert_eq!(lib.track_at(Path::new("/a.mp3")), None);
    }

    #[test]
    fn smart_playlists_refuse_manual_edits() {
        let mut lib = Library::with_default_playlists();
        let smart = lib.playlists()[0].id;
        assert!(matches!(lib.add_to_playlist(smart, &[]), Err(LibraryError::SmartPlaylist(_))));
    }

    #[test]
    fn playlist_names_are_trimmed_and_required() {
        let mut lib = Library::new();
        assert_eq!(lib.create_playlist("   "), Err(LibraryError::EmptyName));
        let id = lib.create_playlist("  Road Trip ").unwrap();
        assert_eq!(lib.playlist(id).unwrap().name, "Road Trip");
    }

    #[test]
    fn remove_by_position_removes_one_duplicate() {
        let mut lib = Library::new();
        let a = lib.add_track(new_track("/a.mp3", "One", "A", "X"));
        let list = lib.create_playlist("Mix").unwrap();
        lib.add_to_playlist(list, &[a, a, a]).unwrap();
        lib.remove_from_playlist(list, &[1, 99]).unwrap();
        assert_eq!(lib.playlist_tracks(list, Utc::now()).unwrap(), [a, a]);
    }

    #[test]
    fn move_keeps_relative_order() {
        let mut lib = Library::new();
        let ids: Vec<TrackId> =
            (0..5).map(|i| lib.add_track(new_track(&format!("/{i}.mp3"), &i.to_string(), "A", "X"))).collect();
        let list = lib.create_playlist("Mix").unwrap();
        lib.add_to_playlist(list, &ids).unwrap();
        // Drag entries 0 and 1 to before entry 4.
        lib.move_in_playlist(list, &[0, 1], 4).unwrap();
        let order = lib.playlist_tracks(list, Utc::now()).unwrap();
        assert_eq!(order, [ids[2], ids[3], ids[0], ids[1], ids[4]]);
    }

    #[test]
    fn from_parts_repairs_damage() {
        let mut lib = Library::new();
        let a = lib.add_track(new_track("/a.mp3", "One", "A", "X"));
        let list = lib.create_playlist("Mix").unwrap();
        lib.add_to_playlist(list, &[a, TrackId(999)]).unwrap_or_default();
        let mut tracks: Vec<Track> = lib.tracks().cloned().collect();
        tracks.push(tracks[0].clone()); // duplicate id
        let mut playlists = lib.playlists().to_vec();
        if let PlaylistKind::Manual { tracks } = &mut playlists[0].kind {
            tracks.push(TrackId(999)); // dangling
        }
        let repaired = Library::from_parts(tracks, playlists, 0);
        assert_eq!(repaired.len(), 1);
        assert_eq!(repaired.playlist_tracks(list, Utc::now()).unwrap(), [a]);
        assert!(repaired.next_id() > list.0);
    }
}
