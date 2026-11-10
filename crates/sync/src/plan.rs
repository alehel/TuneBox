//! Deciding what a sync will do. Pure: no file system access.

use crate::device::DeviceManifest;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;
use tunebox_model::{Track, TrackId, organize};

/// A library track that should be on the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncItem {
    pub id: TrackId,
    pub source: PathBuf,
    /// Where it goes on the device, relative to the device root, with `/` separators.
    pub device_path: String,
    pub size: u64,
    /// Modification time of the source file, in seconds since the Unix epoch.
    pub modified: u64,
    /// Whether the source file can be read now. One that cannot (missing, or on a share that
    /// is not connected) is never copied, and its copy on the device is kept, not deleted.
    pub available: bool,
    /// For playlist files: "Artist - Title" and the length.
    pub label: String,
    pub duration: Duration,
}

impl SyncItem {
    /// Describes `track` for syncing into `music_folder`; `modified` is its file's
    /// modification time, or `None` when the file cannot be read (the caller looks, keeping
    /// this function pure).
    pub fn from_track(track: &Track, music_folder: &str, modified: Option<u64>) -> Self {
        let extension = track.location.extension().and_then(|e| e.to_str()).unwrap_or(track.file.format.extension());
        let relative = organize::relative_path(&track.meta, track.title(), &extension.to_lowercase());
        let mut device_path = music_folder.trim_matches('/').to_owned();
        for part in relative.iter() {
            device_path.push('/');
            device_path.push_str(&part.to_string_lossy());
        }
        let label = if track.meta.artist.is_empty() {
            track.title().to_owned()
        } else {
            format!("{} - {}", track.meta.artist, track.title())
        };
        Self {
            id: track.id,
            source: track.location.clone(),
            device_path,
            size: track.file.size_bytes,
            modified: modified.unwrap_or_default(),
            available: modified.is_some(),
            label,
            duration: track.file.duration,
        }
    }
}

/// Copy a library file to the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyOp {
    pub item: SyncItem,
}

/// Remove a file TuneBox put on the device earlier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteOp {
    pub id: TrackId,
    pub device_path: String,
}

/// A playlist to write to the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistFile {
    pub name: String,
    pub tracks: Vec<TrackId>,
}

/// What a sync will do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncPlan {
    pub copies: Vec<CopyOp>,
    pub deletes: Vec<DeleteOp>,
    /// Tracks already on the device and up to date.
    pub unchanged: usize,
    /// Wanted tracks whose files could not be read: kept as they are on the device if they
    /// were there, left out if not.
    pub unavailable: usize,
    /// Wanted tracks in a format the device's player cannot play: left off (and removed, if an
    /// earlier sync for another player put them there).
    pub unplayable: usize,
    pub playlists: Vec<PlaylistFile>,
    /// Everything that will be on the device afterwards, for writing playlists.
    pub final_paths: BTreeMap<TrackId, SyncItem>,
    pub bytes_to_copy: u64,
    pub bytes_to_free: u64,
}

impl SyncPlan {
    /// Whether the plan fits in `free_bytes` once its deletions have made room.
    pub fn fits(&self, free_bytes: u64) -> bool {
        self.bytes_to_copy <= free_bytes.saturating_add(self.bytes_to_free)
    }

    pub fn is_empty(&self) -> bool {
        self.copies.is_empty() && self.deletes.is_empty()
    }
}

/// Plans a sync of `wanted` onto a device whose manifest is `manifest`.
///
/// A track is copied when it is not on the device, or when its file changed (size or
/// modification time) or its place on the device did (its tags changed). Files TuneBox put on
/// the device that are no longer wanted are deleted. Files TuneBox did not put there are never
/// touched. Songs the device's player cannot play are left off. Copies are made in path order
/// (artist, album, track), so players that list songs in the order their files were made
/// (Snowsky's Echo players) list an album in track order.
pub fn plan(wanted: &[SyncItem], playlists: Vec<PlaylistFile>, manifest: &DeviceManifest) -> SyncPlan {
    let mut result = SyncPlan { playlists, ..SyncPlan::default() };
    let mut seen = HashSet::new();
    for item in wanted {
        if seen.contains(&item.id) {
            continue;
        }
        if !manifest.settings.player.plays(&item.device_path) {
            seen.insert(item.id);
            result.unplayable += 1;
            continue;
        }
        if !item.available {
            result.unavailable += 1;
            if let Some(synced) = manifest.files.get(&item.id) {
                // Kept where it is: a share being offline is no reason to take a song away.
                seen.insert(item.id);
                result.final_paths.insert(item.id, SyncItem { device_path: synced.path.clone(), ..item.clone() });
                result.unchanged += 1;
            }
            continue;
        }
        seen.insert(item.id);
        result.final_paths.insert(item.id, item.clone());
        match manifest.files.get(&item.id) {
            Some(synced)
                if synced.path == item.device_path
                    && synced.source_size == item.size
                    && synced.source_modified == item.modified =>
            {
                result.unchanged += 1;
            }
            Some(synced) => {
                if synced.path != item.device_path {
                    result.deletes.push(DeleteOp { id: item.id, device_path: synced.path.clone() });
                }
                result.bytes_to_free = result.bytes_to_free.saturating_add(synced.source_size);
                result.bytes_to_copy = result.bytes_to_copy.saturating_add(item.size);
                result.copies.push(CopyOp { item: item.clone() });
            }
            None => {
                result.bytes_to_copy = result.bytes_to_copy.saturating_add(item.size);
                result.copies.push(CopyOp { item: item.clone() });
            }
        }
    }
    // Whatever will not be on the device (unwanted, or unplayable there) is removed.
    for (id, synced) in &manifest.files {
        if !result.final_paths.contains_key(id) {
            result.bytes_to_free = result.bytes_to_free.saturating_add(synced.source_size);
            result.deletes.push(DeleteOp { id: *id, device_path: synced.path.clone() });
        }
    }
    let on_device = &result.final_paths;
    result.playlists.iter_mut().for_each(|p| p.tracks.retain(|id| on_device.contains_key(id)));
    result.copies.sort_by(|a, b| a.item.device_path.cmp(&b.item.device_path));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{Device, SyncedFile};

    fn item(id: u64, path: &str, size: u64) -> SyncItem {
        SyncItem {
            id: TrackId(id),
            source: PathBuf::from(format!("/lib/{id}.flac")),
            device_path: path.into(),
            size,
            modified: 100,
            available: true,
            label: String::new(),
            duration: Duration::from_secs(1),
        }
    }

    fn manifest(entries: &[(u64, &str, u64)]) -> DeviceManifest {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = Device::register(dir.path(), "Test").unwrap().manifest;
        for (id, path, size) in entries {
            manifest
                .files
                .insert(TrackId(*id), SyncedFile { path: (*path).into(), source_size: *size, source_modified: 100 });
        }
        manifest
    }

    #[test]
    fn copies_new_keeps_current_deletes_unwanted() {
        let m = manifest(&[(1, "Music/a.flac", 10), (2, "Music/b.flac", 20)]);
        let p = plan(&[item(1, "Music/a.flac", 10), item(3, "Music/c.flac", 30)], Vec::new(), &m);
        assert_eq!(p.unchanged, 1);
        assert_eq!(p.copies.iter().map(|c| c.item.id).collect::<Vec<_>>(), [TrackId(3)]);
        assert_eq!(p.deletes, [DeleteOp { id: TrackId(2), device_path: "Music/b.flac".into() }]);
        assert_eq!((p.bytes_to_copy, p.bytes_to_free), (30, 20));
        assert!(p.fits(10));
        assert!(!p.fits(9));
    }

    #[test]
    fn changed_files_and_moved_tracks_are_recopied() {
        let m = manifest(&[(1, "Music/a.flac", 10), (2, "Music/old name.flac", 20)]);
        let p = plan(&[item(1, "Music/a.flac", 11), item(2, "Music/new name.flac", 20)], Vec::new(), &m);
        assert_eq!(p.copies.len(), 2);
        assert_eq!(p.deletes, [DeleteOp { id: TrackId(2), device_path: "Music/old name.flac".into() }]);
    }

    #[test]
    fn a_song_that_cannot_be_read_keeps_its_copy_and_is_never_deleted() {
        let m = manifest(&[(1, "Music/a.flac", 10)]);
        let offline = |id, path: &str| SyncItem { available: false, modified: 0, ..item(id, path, 10) };
        // Even with its tags changed (a new device path) and its time unknown.
        let plan = plan(&[offline(1, "Music/renamed.flac"), offline(2, "Music/b.flac")], Vec::new(), &m);
        assert!(plan.copies.is_empty() && plan.deletes.is_empty(), "{plan:?}");
        assert_eq!((plan.unchanged, plan.unavailable), (1, 2));
        assert_eq!(plan.final_paths[&TrackId(1)].device_path, "Music/a.flac", "playlists point at the kept copy");
        assert!(!plan.final_paths.contains_key(&TrackId(2)));
    }

    #[test]
    fn songs_the_player_cannot_play_are_left_off_and_removed() {
        let mut m = manifest(&[(1, "Music/a.ogg", 10), (2, "Music/b.flac", 20)]);
        m.settings.player = crate::PlayerModel::SnowskyEchoMini;
        let wanted = [item(1, "Music/a.ogg", 10), item(2, "Music/b.flac", 20), item(3, "Music/c.opus", 30)];
        let playlist = PlaylistFile { name: "Mix".into(), tracks: vec![TrackId(3), TrackId(2), TrackId(1)] };
        let p = plan(&wanted, vec![playlist], &m);
        assert_eq!((p.unchanged, p.unplayable), (1, 2));
        assert!(p.copies.is_empty());
        assert_eq!(
            p.deletes,
            [DeleteOp { id: TrackId(1), device_path: "Music/a.ogg".into() }],
            "put there for another player"
        );
        assert_eq!(p.playlists[0].tracks, [TrackId(2)]);
    }

    #[test]
    fn copies_follow_artist_album_track_order() {
        let m = manifest(&[]);
        let wanted = [
            item(1, "Music/B/Album/02 Two.flac", 1),
            item(2, "Music/A/Album/01 One.flac", 1),
            item(3, "Music/B/Album/01 One.flac", 1),
        ];
        let order: Vec<u64> = plan(&wanted, Vec::new(), &m).copies.iter().map(|c| c.item.id.0).collect();
        assert_eq!(order, [2, 3, 1]);
    }

    #[test]
    fn playlists_only_name_synced_tracks() {
        let m = manifest(&[]);
        let playlist = PlaylistFile { name: "Mix".into(), tracks: vec![TrackId(1), TrackId(9)] };
        let p = plan(&[item(1, "Music/a.flac", 1), item(1, "Music/a.flac", 1)], vec![playlist], &m);
        assert_eq!(p.copies.len(), 1, "duplicates are copied once");
        assert_eq!(p.playlists[0].tracks, [TrackId(1)]);
    }
}
