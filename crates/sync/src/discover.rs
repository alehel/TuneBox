//! Spotting players among the mounted volumes.

use crate::device::Device;
use crate::mtp::MtpAddress;
use crate::storage::{FolderStorage, Storage};
use std::path::{Path, PathBuf};

/// How TuneBox reaches a player's files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Access {
    /// Mounted as a folder at the volume's root (USB mass storage, an SD card).
    #[default]
    Folder,
    /// Over MTP; the volume's root is then only its identity in the device list.
    Mtp(MtpAddress),
}

/// A mounted volume, as the platform reports it, or a storage of an MTP player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub root: PathBuf,
    /// The volume label, e.g. "SANSA CLIP".
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// Whether the system marks it removable (USB, SD card).
    pub removable: bool,
    pub access: Access,
}

impl Volume {
    pub fn is_mtp(&self) -> bool {
        matches!(self.access, Access::Mtp(_))
    }

    /// Opens the player's files for reading and writing (for MTP, a connection to it).
    pub fn storage(&self) -> Result<Box<dyn Storage>, crate::DeviceError> {
        match &self.access {
            Access::Folder => Ok(Box::new(FolderStorage::new(&self.root))),
            Access::Mtp(address) => Ok(Box::new(crate::mtp::connect(address)?)),
        }
    }
}

/// A volume that is, or might be, a player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub volume: Volume,
    /// Set when TuneBox already manages it.
    pub device: Option<Device>,
    /// Why it cannot be used now, and what to do (an MTP player another program holds).
    pub problem: Option<String>,
}

/// Folders and files that mark a volume as a music player.
const PLAYER_MARKERS: &[&str] = &[".rockbox", "Music", "MUSIC", "music"];

/// Whether the volume at `root` looks like a music player (it has a marker such as a Rockbox
/// folder or a top-level Music folder).
pub fn looks_like_player(root: &Path) -> bool {
    PLAYER_MARKERS.iter().any(|marker| root.join(marker).is_dir())
}

/// The volumes worth offering as devices: every managed player, plus removable volumes that
/// look like players. A volume whose manifest cannot be read is still offered, unmanaged, and
/// the problem logged.
pub fn scan(volumes: &[Volume]) -> Vec<Candidate> {
    volumes
        .iter()
        .filter_map(|volume| {
            let device = Device::open(&volume.root).unwrap_or_else(|e| {
                log::warn!("ignoring the TuneBox manifest on {}: {e}", volume.root.display());
                None
            });
            let offer = device.is_some() || (volume.removable && looks_like_player(&volume.root));
            offer.then(|| Candidate { volume: volume.clone(), device, problem: None })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(root: &Path, removable: bool) -> Volume {
        Volume {
            root: root.to_path_buf(),
            label: "X".into(),
            total_bytes: 1,
            free_bytes: 1,
            removable,
            access: Access::Folder,
        }
    }

    #[test]
    fn offers_managed_and_player_like_volumes() {
        let managed = tempfile::tempdir().unwrap();
        Device::register(managed.path(), "Mine").unwrap();
        let rockbox = tempfile::tempdir().unwrap();
        std::fs::create_dir(rockbox.path().join(".rockbox")).unwrap();
        let usb_stick = tempfile::tempdir().unwrap();
        let fixed_disk = tempfile::tempdir().unwrap();
        std::fs::create_dir(fixed_disk.path().join("Music")).unwrap();

        let found = scan(&[
            volume(managed.path(), false),
            volume(rockbox.path(), true),
            volume(usb_stick.path(), true),
            volume(fixed_disk.path(), false),
        ]);
        let roots: Vec<_> = found.iter().map(|c| c.volume.root.clone()).collect();
        assert_eq!(roots, [managed.path().to_path_buf(), rockbox.path().to_path_buf()]);
        assert!(found[0].device.is_some());
        assert!(found[1].device.is_none());
    }
}
