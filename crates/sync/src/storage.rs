//! Where a player's files live, behind the few operations sync needs, so planning and carrying
//! out a sync are the same for every kind of player. [`FolderStorage`] is a mounted
//! mass-storage player (or any folder); `MtpStorage` (in `mtp.rs`) is a player reached over
//! MTP.
//!
//! Every implementation keeps two promises: a file that is not complete never sits under its
//! final name looking complete (or, where the device makes that impossible, `execute` journals
//! it, see there), and a complete manifest exists at every moment.

use std::io;
use std::path::{Path, PathBuf};

/// The folder holding TuneBox's manifest on a player.
pub const MANIFEST_DIR: &str = ".tunebox";
/// The manifest's name when written in place (folders).
pub const MANIFEST_FILE: &str = "device.json";

pub trait Storage {
    /// The bytes of TuneBox's manifest, or `None` if the player has none.
    fn read_manifest(&mut self) -> io::Result<Option<Vec<u8>>>;

    /// Replaces the manifest so that a complete one exists at every moment.
    fn write_manifest(&mut self, bytes: &[u8]) -> io::Result<()>;

    /// Copies the library file `source` to `path` (relative, `/`-separated), replacing what is
    /// there and making folders as needed.
    fn put_file(&mut self, source: &Path, path: &str) -> io::Result<()>;

    /// Writes a small file (a playlist) at `path`, replacing what is there.
    fn write_file(&mut self, path: &str, bytes: &[u8]) -> io::Result<()>;

    /// Removes the file at `path` and the folders that leaves empty. `Ok(false)` when there is
    /// no such file.
    fn remove_file(&mut self, path: &str) -> io::Result<bool>;

    /// Free space as the device itself reports it now, where that is how to learn it (MTP).
    /// `None` for folders, whose free space comes with the volume list.
    fn free_bytes(&mut self) -> io::Result<Option<u64>> {
        Ok(None)
    }
}

/// The parts of a relative device path, with empty, `.` and `..` parts dropped so a path can
/// never leave the player's root.
pub fn path_parts(relative: &str) -> Vec<&str> {
    relative.split('/').filter(|part| !part.is_empty() && *part != "." && *part != "..").collect()
}

/// The generation of a manifest file name: 0 for `device.json`, *n* for `device-n.json`
/// (written where files cannot be replaced in place: the newest that reads wins).
pub fn manifest_generation(name: &str) -> Option<u64> {
    if name == MANIFEST_FILE {
        return Some(0);
    }
    name.strip_prefix("device-")?.strip_suffix(".json")?.parse().ok()
}

/// A player mounted as a folder (USB mass storage, an SD card).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderStorage {
    root: PathBuf,
}

impl FolderStorage {
    pub fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        path_parts(relative).iter().fold(self.root.clone(), |path, part| path.join(part))
    }

    fn manifest_dir(&self) -> PathBuf {
        self.root.join(MANIFEST_DIR)
    }

    /// Generation-numbered manifests (`device-n.json`), left by syncing the same player over
    /// MTP, newest first.
    fn numbered_manifests(&self) -> Vec<(u64, PathBuf)> {
        let mut found: Vec<(u64, PathBuf)> = std::fs::read_dir(self.manifest_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let generation = manifest_generation(&entry.file_name().to_string_lossy())?;
                (generation > 0).then(|| (generation, entry.path()))
            })
            .collect();
        found.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        found
    }

    /// Removes folders emptied by a deletion, up to (not including) the root.
    fn remove_empty_parents(&self, file: &Path) {
        let mut dir = file.parent();
        while let Some(current) = dir {
            if current == self.root || !current.starts_with(&self.root) || std::fs::remove_dir(current).is_err() {
                break;
            }
            dir = current.parent();
        }
    }
}

impl Storage for FolderStorage {
    fn read_manifest(&mut self) -> io::Result<Option<Vec<u8>>> {
        match std::fs::read(self.manifest_dir().join(MANIFEST_FILE)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // A player that also does MTP may have been synced that way.
                Ok(self.numbered_manifests().first().and_then(|(_, path)| std::fs::read(path).ok()))
            }
            Err(e) => Err(e),
        }
    }

    fn write_manifest(&mut self, bytes: &[u8]) -> io::Result<()> {
        let dir = self.manifest_dir();
        std::fs::create_dir_all(&dir)?;
        craft_atomic::atomic_write(&dir.join(MANIFEST_FILE), bytes)?;
        // Older MTP-written copies would otherwise win the next time it is synced over MTP.
        for (_, stale) in self.numbered_manifests() {
            let _ = std::fs::remove_file(stale);
        }
        Ok(())
    }

    /// Copies through a `.part` file, renamed only once complete.
    fn put_file(&mut self, source: &Path, path: &str) -> io::Result<()> {
        let target = self.path(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut part_name = target.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        part_name.push(".part");
        let part = target.with_file_name(part_name);
        let result = std::fs::copy(source, &part).and_then(|_| std::fs::rename(&part, &target));
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        result
    }

    fn write_file(&mut self, path: &str, bytes: &[u8]) -> io::Result<()> {
        let target = self.path(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        craft_atomic::atomic_write(&target, bytes)
    }

    fn remove_file(&mut self, path: &str) -> io::Result<bool> {
        let target = self.path(path);
        let removed = match std::fs::remove_file(&target) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };
        self.remove_empty_parents(&target);
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_paths_cannot_escape_the_root() {
        let storage = FolderStorage::new(Path::new("/dev/player"));
        assert_eq!(storage.path("../../etc/passwd"), PathBuf::from("/dev/player/etc/passwd"));
        assert_eq!(path_parts("./Music//a.flac"), ["Music", "a.flac"]);
    }

    #[test]
    fn manifest_generations() {
        assert_eq!(manifest_generation("device.json"), Some(0));
        assert_eq!(manifest_generation("device-12.json"), Some(12));
        assert_eq!(manifest_generation("device-x.json"), None);
        assert_eq!(manifest_generation("notes.txt"), None);
    }

    #[test]
    fn a_folder_reads_a_manifest_written_over_mtp_and_supersedes_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = FolderStorage::new(dir.path());
        assert_eq!(storage.read_manifest().unwrap(), None);
        std::fs::create_dir_all(dir.path().join(MANIFEST_DIR)).unwrap();
        std::fs::write(dir.path().join(".tunebox/device-3.json"), b"{\"a\":3}").unwrap();
        std::fs::write(dir.path().join(".tunebox/device-2.json"), b"{\"a\":2}").unwrap();
        assert_eq!(storage.read_manifest().unwrap().unwrap(), b"{\"a\":3}");
        storage.write_manifest(b"{\"a\":4}").unwrap();
        assert_eq!(storage.read_manifest().unwrap().unwrap(), b"{\"a\":4}");
        assert!(!dir.path().join(".tunebox/device-3.json").exists());
    }
}
