//! The library file: versioned JSON, written atomically so a crash or a full disk never leaves a
//! half-written library behind.

use crate::Library;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tunebox_model::{MediaKind, Playlist, Track};

/// Identifies the file's contents.
const FORMAT: &str = "tunebox-library";
/// The newest file version this build reads and the version it writes.
const VERSION: u32 = 1;

/// Why the library file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not read or write the library file: {0}")]
    Io(#[from] std::io::Error),
    #[error("the library file is damaged: {0}")]
    Damaged(String),
    #[error("the library file was written by a newer TuneBox (version {0}); this version reads up to {VERSION}")]
    TooNew(u32),
}

#[derive(Serialize, Deserialize)]
struct LibraryFile {
    format: String,
    version: u32,
    next_id: u64,
    tracks: Vec<Track>,
    playlists: Vec<Playlist>,
    /// Added after version 1 shipped; older files have none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    network_places: Vec<std::path::PathBuf>,
    /// Whether tracks carry their media kind. Files written before media kinds existed have
    /// none; their `.m4b` tracks are audiobooks, decided once when such a file is read.
    #[serde(default)]
    media_kinds: bool,
}

/// Reads the library at `path`. A missing file is a new, empty library with the default
/// playlists.
pub fn load(path: &Path) -> Result<Library, StoreError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Library::with_default_playlists()),
        Err(e) => return Err(e.into()),
    };
    from_json(&bytes)
}

/// Parses a library file's bytes.
pub fn from_json(bytes: &[u8]) -> Result<Library, StoreError> {
    let file: LibraryFile = serde_json::from_slice(bytes).map_err(|e| StoreError::Damaged(e.to_string()))?;
    if file.format != FORMAT {
        return Err(StoreError::Damaged(format!("expected a {FORMAT} file, found \"{}\"", file.format)));
    }
    if file.version > VERSION {
        return Err(StoreError::TooNew(file.version));
    }
    let mut tracks = file.tracks;
    if !file.media_kinds {
        for track in &mut tracks {
            track.kind = MediaKind::from_path(&track.location);
        }
    }
    Ok(Library::from_parts(tracks, file.playlists, file.next_id).with_network_places(file.network_places))
}

/// The library as file bytes.
pub fn to_json(library: &Library) -> Result<Vec<u8>, StoreError> {
    let file = LibraryFile {
        format: FORMAT.to_owned(),
        version: VERSION,
        next_id: library.next_id(),
        tracks: library.tracks().cloned().collect(),
        playlists: library.playlists().to_vec(),
        network_places: library.network_places().to_vec(),
        media_kinds: true,
    };
    serde_json::to_vec_pretty(&file).map_err(|e| StoreError::Damaged(e.to_string()))
}

/// Writes the library to `path`, replacing the old file only once the new one is complete.
pub fn save(library: &Library, path: &Path) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    craft_atomic::atomic_write(path, &to_json(library)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::new_track;
    use chrono::Utc;

    #[test]
    fn round_trips_through_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/library.json");
        let mut lib = Library::with_default_playlists();
        let id = lib.add_track(new_track("/a.mp3", "One", "A", "X"));
        let list = lib.create_playlist("Mix").unwrap();
        lib.add_to_playlist(list, &[id]).unwrap();
        assert!(lib.remember_network_place("/mnt/nas".into()));
        assert!(!lib.remember_network_place("/mnt/nas".into()), "remembered once");
        save(&lib, &path).unwrap();

        let back = load(&path).unwrap();
        assert_eq!(back.network_places(), [std::path::PathBuf::from("/mnt/nas")]);
        assert_eq!(back.len(), 1);
        assert_eq!(back.playlists(), lib.playlists());
        assert_eq!(back.playlist_tracks(list, Utc::now()).unwrap(), [id]);
        assert_eq!(back.next_id(), lib.next_id());
    }

    #[test]
    fn an_older_library_has_its_m4b_tracks_marked_as_audiobooks_once() {
        let mut lib = Library::with_default_playlists();
        let book = lib.add_track(new_track("/b/Book.m4b", "Book", "A", "B"));
        let song = lib.add_track(new_track("/s/Song.m4a", "Song", "A", "S"));
        // Written before media kinds: no marker, no kinds.
        let mut json: serde_json::Value = serde_json::from_slice(&to_json(&lib).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("media_kinds");
        let older = from_json(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(older.track(book).unwrap().kind, MediaKind::Audiobook);
        assert_eq!(older.track(song).unwrap().kind, MediaKind::Music);
        // From then on the kinds are the listener's: an .m4b made music stays music.
        let mut lib = older;
        lib.set_kind(book, MediaKind::Music).unwrap();
        let again = from_json(&to_json(&lib).unwrap()).unwrap();
        assert_eq!(again.track(book).unwrap().kind, MediaKind::Music);
    }

    #[test]
    fn missing_file_is_a_new_library() {
        let dir = tempfile::tempdir().unwrap();
        let lib = load(&dir.path().join("none.json")).unwrap();
        assert!(lib.is_empty());
        assert!(!lib.playlists().is_empty());
    }

    #[test]
    fn rejects_garbage_and_newer_versions() {
        assert!(matches!(from_json(b"{not json"), Err(StoreError::Damaged(_))));
        let newer = br#"{"format":"tunebox-library","version":99,"next_id":1,"tracks":[],"playlists":[]}"#;
        assert!(matches!(from_json(newer), Err(StoreError::TooNew(99))));
        let other = br#"{"format":"something-else","version":1,"next_id":1,"tracks":[],"playlists":[]}"#;
        assert!(matches!(from_json(other), Err(StoreError::Damaged(_))));
    }
}
