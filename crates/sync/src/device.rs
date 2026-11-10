//! A player TuneBox manages, and the manifest it keeps on it.

use crate::storage::{FolderStorage, Storage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tunebox_model::{PlaylistId, TrackId};

const FORMAT: &str = "tunebox-device";
const VERSION: u32 = 1;

/// Why a device could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("could not read or write the device: {0}")]
    Io(#[from] std::io::Error),
    #[error("the device's TuneBox manifest is damaged: {0}")]
    Damaged(String),
    #[error("the device was set up by a newer TuneBox (manifest version {0})")]
    TooNew(u32),
    /// The player could not be reached; the message says why and what to do.
    #[error("{0}")]
    Unavailable(String),
}

/// What goes onto the device.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncMode {
    /// "Entire music library".
    #[default]
    Everything,
    /// "Selected playlists": the tracks of these playlists.
    Playlists(Vec<PlaylistId>),
}

/// The device's sync preferences (the Music tab of the device page).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncSettings {
    pub mode: SyncMode,
    /// "Sync only checked songs".
    pub only_checked: bool,
    /// Write the synced playlists as M3U8 files the player can read.
    pub write_playlists: bool,
    /// Music folder on the device, relative to its root.
    pub music_folder: String,
    /// Playlist folder on the device, relative to its root.
    pub playlist_folder: String,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            mode: SyncMode::Everything,
            only_checked: true,
            write_playlists: true,
            music_folder: "Music".into(),
            playlist_folder: "Playlists".into(),
        }
    }
}

/// A file TuneBox put on the device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncedFile {
    /// Path relative to the device root, with `/` separators.
    pub path: String,
    /// Size and modification time of the library file when it was copied: if either changes the
    /// file is copied again.
    pub source_size: u64,
    pub source_modified: u64,
}

/// The contents of `.tunebox/device.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceManifest {
    format: String,
    version: u32,
    /// A random identifier, so devices with the same name stay apart.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub settings: SyncSettings,
    #[serde(default)]
    pub files: BTreeMap<TrackId, SyncedFile>,
    /// Playlist files TuneBox wrote, relative to the device root, so stale ones can be removed.
    #[serde(default)]
    pub playlists: Vec<String>,
    /// Files TuneBox started writing and has not seen finish (a journal, see `execute`): the
    /// next sync removes them first. Empty, and left out of the file, after a clean sync.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending: Vec<String>,
}

/// A player with a TuneBox manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub root: PathBuf,
    pub manifest: DeviceManifest,
}

impl Device {
    /// The device at `root`, or `None` if TuneBox has not set it up.
    pub fn open(root: &Path) -> Result<Option<Device>, DeviceError> {
        Self::read_from(&mut FolderStorage::new(root), root)
    }

    /// The device whose files are in `storage` (known as `root`), or `None` if TuneBox has not
    /// set it up.
    pub fn read_from(storage: &mut dyn Storage, root: &Path) -> Result<Option<Device>, DeviceError> {
        let Some(bytes) = storage.read_manifest()? else { return Ok(None) };
        let manifest: DeviceManifest =
            serde_json::from_slice(&bytes).map_err(|e| DeviceError::Damaged(e.to_string()))?;
        if manifest.format != FORMAT {
            return Err(DeviceError::Damaged(format!("unexpected format \"{}\"", manifest.format)));
        }
        if manifest.version > VERSION {
            return Err(DeviceError::TooNew(manifest.version));
        }
        Ok(Some(Device { root: root.to_path_buf(), manifest }))
    }

    /// Sets up the player at `root` for syncing, under `name`. An existing manifest is kept
    /// (only renamed), so its record of synced files survives.
    pub fn register(root: &Path, name: &str) -> Result<Device, DeviceError> {
        Self::register_on(&mut FolderStorage::new(root), root, name)
    }

    /// [`Device::register`] for a player reached through `storage`.
    pub fn register_on(storage: &mut dyn Storage, root: &Path, name: &str) -> Result<Device, DeviceError> {
        if let Some(mut device) = Self::read_from(storage, root)? {
            device.manifest.name = name.to_owned();
            device.save_to(storage)?;
            return Ok(device);
        }
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
        let manifest = DeviceManifest {
            format: FORMAT.into(),
            version: VERSION,
            id: format!("{nanos:x}"),
            name: name.to_owned(),
            settings: SyncSettings::default(),
            files: BTreeMap::new(),
            playlists: Vec::new(),
            pending: Vec::new(),
        };
        let device = Device { root: root.to_path_buf(), manifest };
        device.save_to(storage)?;
        Ok(device)
    }

    /// Writes the manifest atomically to a folder player.
    pub fn save(&self) -> Result<(), DeviceError> {
        self.save_to(&mut FolderStorage::new(&self.root))
    }

    /// Writes the manifest to `storage`, so a complete one exists at every moment.
    pub fn save_to(&self, storage: &mut dyn Storage) -> Result<(), DeviceError> {
        let bytes = serde_json::to_vec_pretty(&self.manifest).map_err(|e| DeviceError::Damaged(e.to_string()))?;
        storage.write_manifest(&bytes)?;
        Ok(())
    }

    /// Bytes of music TuneBox has put on the device, as last recorded.
    pub fn music_bytes(&self) -> u64 {
        self.manifest.files.values().map(|f| f.source_size).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{MANIFEST_DIR, MANIFEST_FILE};

    impl Device {
        fn manifest_path(root: &Path) -> PathBuf {
            root.join(MANIFEST_DIR).join(MANIFEST_FILE)
        }
    }

    #[test]
    fn register_open_and_rename() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Device::open(dir.path()).unwrap(), None);
        let mut device = Device::register(dir.path(), "Pocket Player").unwrap();
        device
            .manifest
            .files
            .insert(TrackId(7), SyncedFile { path: "Music/a.flac".into(), source_size: 10, source_modified: 1 });
        device.save().unwrap();
        let renamed = Device::register(dir.path(), "Running Player").unwrap();
        assert_eq!(renamed.manifest.name, "Running Player");
        assert_eq!(renamed.manifest.id, device.manifest.id);
        assert_eq!(renamed.manifest.files.len(), 1);
        assert_eq!(renamed.music_bytes(), 10);
    }

    #[test]
    fn damaged_manifest_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(MANIFEST_DIR)).unwrap();
        std::fs::write(Device::manifest_path(dir.path()), b"{oops").unwrap();
        assert!(matches!(Device::open(dir.path()), Err(DeviceError::Damaged(_))));
    }
}
