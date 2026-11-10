//! Carrying out a sync plan on the device, through its [`Storage`].
//!
//! The order keeps the listener's music safe whatever happens part way (a full disk, a cable
//! pulled): deletions first, to make room; copies in batches, with the manifest saved between
//! batches; playlists last. Before each batch the manifest lists the batch's files as
//! *pending*, and each leaves that list only once its copy is complete. MTP players cannot
//! rename, so a song cut off mid-transfer sits under its final name; being pending, it is never
//! taken for a synced song, and the next sync removes it (TuneBox put it there) before anything
//! else. On folders the `*.part` copies already guarantee this, and the journal costs nothing.

use crate::device::{Device, DeviceError, SyncedFile};
use crate::plan::SyncPlan;
use crate::storage::{FolderStorage, Storage};
use std::sync::atomic::{AtomicBool, Ordering};

/// Where a sync has got to.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncProgress {
    pub done: usize,
    pub total: usize,
    /// What is happening, e.g. "Copying Artist - Title".
    pub message: String,
}

/// What a sync did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub copied: usize,
    pub deleted: usize,
    pub playlists_written: usize,
    pub failures: Vec<String>,
    pub cancelled: bool,
    /// Wanted songs whose files could not be read (see `SyncPlan::unavailable`).
    pub unavailable: usize,
    /// Wanted songs in a format the device's player cannot play (see `SyncPlan::unplayable`).
    pub unplayable: usize,
    /// Free space afterwards, where the device reports it (MTP).
    pub free_bytes: Option<u64>,
}

/// How many copies happen between manifest saves, so an unplugged player loses little.
const SAVE_EVERY: usize = 20;

/// Carries out `plan` on the folder player `device`, updating and saving its manifest.
pub fn execute(
    device: &mut Device,
    plan: &SyncPlan,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(SyncProgress),
) -> Result<SyncReport, DeviceError> {
    let mut storage = FolderStorage::new(&device.root);
    execute_on(device, &mut storage, plan, cancel, progress)
}

/// Carries out `plan` on `device` through `storage`, updating and saving its manifest.
/// Individual files that fail are reported and skipped; the manifest always reflects what is
/// really on the device. An error means the manifest could not be saved (the player is gone).
pub fn execute_on(
    device: &mut Device,
    storage: &mut dyn Storage,
    plan: &SyncPlan,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(SyncProgress),
) -> Result<SyncReport, DeviceError> {
    let mut report = SyncReport { unavailable: plan.unavailable, unplayable: plan.unplayable, ..SyncReport::default() };
    let total = plan.deletes.len() + plan.copies.len();
    let mut done = 0;

    remove_leftovers(device, storage, &mut report.failures);
    for delete in &plan.deletes {
        progress(SyncProgress { done, total, message: format!("Removing {}", delete.device_path) });
        match storage.remove_file(&delete.device_path) {
            Ok(true) => report.deleted += 1,
            Ok(false) => {}
            Err(e) => report.failures.push(format!("Could not remove {}: {e}", delete.device_path)),
        }
        device.manifest.files.remove(&delete.id);
        done += 1;
    }
    device.save_to(storage)?;

    for batch in plan.copies.chunks(SAVE_EVERY) {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        for copy in batch {
            // A song copied again to the same place loses its record first: until the new copy
            // is complete, the file there is not a synced song.
            device.manifest.files.remove(&copy.item.id);
            add_pending(device, &copy.item.device_path);
        }
        device.save_to(storage)?;
        for copy in batch {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                break;
            }
            progress(SyncProgress { done, total, message: format!("Copying {}", copy.item.label) });
            match storage.put_file(&copy.item.source, &copy.item.device_path) {
                Ok(()) => {
                    device.manifest.pending.retain(|p| *p != copy.item.device_path);
                    device.manifest.files.insert(
                        copy.item.id,
                        SyncedFile {
                            path: copy.item.device_path.clone(),
                            source_size: copy.item.size,
                            source_modified: copy.item.modified,
                        },
                    );
                    report.copied += 1;
                }
                // It stays pending: whatever the failure left behind is removed next time.
                Err(e) => report.failures.push(format!("Could not copy {}: {e}", copy.item.label)),
            }
            done += 1;
        }
    }

    if device.manifest.settings.write_playlists && !report.cancelled {
        report.playlists_written = write_playlists(device, storage, plan, &mut report.failures)?;
    }
    device.save_to(storage)?;
    report.free_bytes = storage.free_bytes().ok().flatten();
    progress(SyncProgress { done, total, message: "Sync complete".into() });
    Ok(report)
}

fn add_pending(device: &mut Device, path: &str) {
    if !device.manifest.pending.iter().any(|p| p == path) {
        device.manifest.pending.push(path.to_owned());
    }
}

/// Removes what an interrupted sync left half written. These are files TuneBox started
/// writing itself, so removing them takes nothing of the listener's.
fn remove_leftovers(device: &mut Device, storage: &mut dyn Storage, failures: &mut Vec<String>) {
    let pending = std::mem::take(&mut device.manifest.pending);
    for path in pending {
        let synced =
            device.manifest.files.values().any(|f| f.path == path) || device.manifest.playlists.contains(&path);
        if synced {
            continue; // Finished after all (recorded before the journal was saved).
        }
        if let Err(e) = storage.remove_file(&path) {
            failures.push(format!("Could not remove the unfinished {path}: {e}"));
            device.manifest.pending.push(path);
        }
    }
}

/// Writes one M3U8 per playlist, with paths relative to the playlist folder, and removes
/// playlist files from earlier syncs that are no longer wanted.
fn write_playlists(
    device: &mut Device,
    storage: &mut dyn Storage,
    plan: &SyncPlan,
    failures: &mut Vec<String>,
) -> Result<usize, DeviceError> {
    let folder = device.manifest.settings.playlist_folder.trim_matches('/').to_owned();
    let depth = folder.split('/').filter(|p| !p.is_empty()).count();
    let up = "../".repeat(depth);
    let files: Vec<(String, String)> = plan
        .playlists
        .iter()
        .map(|playlist| {
            let name = tunebox_model::organize::component(&playlist.name, "Playlist");
            (playlist.name.clone(), format!("{folder}/{name}.m3u8"))
        })
        .collect();
    // Journalled like songs: a player that cannot replace a file in one step could be left
    // with a half-written playlist.
    files.iter().for_each(|(_, relative)| add_pending(device, relative));
    if !files.is_empty() {
        device.save_to(storage)?;
    }
    let mut written = Vec::new();
    for (playlist, (name, relative)) in plan.playlists.iter().zip(&files) {
        let mut text = String::from("#EXTM3U\n");
        for id in &playlist.tracks {
            let Some(item) = plan.final_paths.get(id) else { continue };
            if !device.manifest.files.contains_key(id) {
                continue; // Its copy failed.
            }
            text.push_str(&format!("#EXTINF:{},{}\n{up}{}\n", item.duration.as_secs(), item.label, item.device_path));
        }
        match storage.write_file(relative, text.as_bytes()) {
            Ok(()) => {
                device.manifest.pending.retain(|p| p != relative);
                written.push(relative.clone());
            }
            Err(e) => failures.push(format!("Could not write playlist {name}: {e}")),
        }
    }
    for stale in device.manifest.playlists.iter().filter(|p| !written.contains(p)) {
        let _ = storage.remove_file(stale);
    }
    let count = written.len();
    device.manifest.playlists = written;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{PlaylistFile, SyncItem, plan};
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use tunebox_model::TrackId;

    fn item(library: &Path, id: u64, name: &str) -> SyncItem {
        let source = library.join(format!("{id}.flac"));
        std::fs::write(&source, vec![id as u8; 100 + id as usize]).unwrap();
        SyncItem {
            id: TrackId(id),
            source,
            device_path: format!("Music/Artist/{name}.flac"),
            size: 100 + id,
            modified: 5,
            available: true,
            label: format!("Artist - {name}"),
            duration: Duration::from_secs(61),
        }
    }

    #[test]
    fn syncs_then_resyncs_incrementally() {
        let library = tempfile::tempdir().unwrap();
        let player = tempfile::tempdir().unwrap();
        let mut device = Device::register(player.path(), "Player").unwrap();
        let (a, b) = (item(library.path(), 1, "One"), item(library.path(), 2, "Two"));
        let mix = PlaylistFile { name: "Mix/Tape".into(), tracks: vec![TrackId(2), TrackId(1)] };

        let first = plan(&[a.clone(), b.clone()], vec![mix], &device.manifest);
        let report = execute(&mut device, &first, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!((report.copied, report.deleted, report.playlists_written), (2, 0, 1));
        assert_eq!(std::fs::read(player.path().join("Music/Artist/Two.flac")).unwrap().len(), 102);
        let m3u = std::fs::read_to_string(player.path().join("Playlists/Mix_Tape.m3u8")).unwrap();
        assert_eq!(
            m3u,
            "#EXTM3U\n#EXTINF:61,Artist - Two\n../Music/Artist/Two.flac\n#EXTINF:61,Artist - One\n../Music/Artist/One.flac\n"
        );

        // The manifest survives a reconnect, and dropping a track deletes only that file.
        let mut device = Device::open(player.path()).unwrap().unwrap();
        let second = plan(&[a], Vec::new(), &device.manifest);
        assert_eq!((second.copies.len(), second.unchanged), (0, 1));
        let report = execute(&mut device, &second, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(report.deleted, 1);
        assert!(!player.path().join("Music/Artist/Two.flac").exists());
        assert!(player.path().join("Music/Artist/One.flac").exists());
        assert!(!player.path().join("Playlists/Mix_Tape.m3u8").exists(), "stale playlists are removed");
    }

    #[test]
    fn a_missing_source_is_reported_and_not_recorded() {
        let library = tempfile::tempdir().unwrap();
        let player = tempfile::tempdir().unwrap();
        let mut device = Device::register(player.path(), "Player").unwrap();
        let mut gone = item(library.path(), 1, "Gone");
        gone.source = library.path().join("missing.flac");
        let p = plan(&[gone], Vec::new(), &device.manifest);
        let report = execute(&mut device, &p, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(report.failures.len(), 1);
        assert!(device.manifest.files.is_empty());
        assert!(!player.path().join("Music/Artist/Gone.flac.part").exists());
    }

    fn device_path(root: &Path, relative: &str) -> PathBuf {
        FolderStorage::new(root).path(relative)
    }

    #[test]
    fn device_paths_cannot_escape_the_root() {
        assert_eq!(device_path(Path::new("/dev/player"), "../../etc/passwd"), PathBuf::from("/dev/player/etc/passwd"));
    }
}
