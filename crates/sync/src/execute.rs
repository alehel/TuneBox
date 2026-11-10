//! Carrying out a sync plan on the device.

use crate::device::{Device, DeviceError, SyncedFile};
use crate::plan::{SyncItem, SyncPlan};
use std::path::{Path, PathBuf};
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
}

/// How many copies happen between manifest saves, so an unplugged player loses little.
const SAVE_EVERY: usize = 20;

/// Carries out `plan` on `device`, updating and saving its manifest. Individual files that fail
/// are reported and skipped; the manifest always reflects what is really on the device.
pub fn execute(device: &mut Device, plan: &SyncPlan, cancel: &AtomicBool, progress: &mut dyn FnMut(SyncProgress)) -> Result<SyncReport, DeviceError> {
    let mut report = SyncReport::default();
    let total = plan.deletes.len() + plan.copies.len();
    let mut done = 0;

    for delete in &plan.deletes {
        progress(SyncProgress { done, total, message: format!("Removing {}", delete.device_path) });
        let path = device_path(&device.root, &delete.device_path);
        match std::fs::remove_file(&path) {
            Ok(()) => report.deleted += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => report.failures.push(format!("Could not remove {}: {e}", delete.device_path)),
        }
        remove_empty_parents(&device.root, &path);
        device.manifest.files.remove(&delete.id);
        done += 1;
    }
    device.save()?;

    for (index, copy) in plan.copies.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        progress(SyncProgress { done, total, message: format!("Copying {}", copy.item.label) });
        match copy_file(&device.root, &copy.item) {
            Ok(()) => {
                device.manifest.files.insert(
                    copy.item.id,
                    SyncedFile { path: copy.item.device_path.clone(), source_size: copy.item.size, source_modified: copy.item.modified },
                );
                report.copied += 1;
            }
            Err(e) => report.failures.push(format!("Could not copy {}: {e}", copy.item.label)),
        }
        done += 1;
        if (index + 1) % SAVE_EVERY == 0 {
            device.save()?;
        }
    }

    if device.manifest.settings.write_playlists && !report.cancelled {
        report.playlists_written = write_playlists(device, plan, &mut report.failures);
    }
    device.save()?;
    progress(SyncProgress { done, total, message: "Sync complete".into() });
    Ok(report)
}

fn device_path(root: &Path, relative: &str) -> PathBuf {
    relative.split('/').filter(|part| !part.is_empty() && *part != "..").fold(root.to_path_buf(), |path, part| path.join(part))
}

/// Copies through a `.part` file, renamed only once complete.
fn copy_file(root: &Path, item: &SyncItem) -> std::io::Result<()> {
    let target = device_path(root, &item.device_path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut part_name = target.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    part_name.push(".part");
    let part = target.with_file_name(part_name);
    let result = std::fs::copy(&item.source, &part).and_then(|_| std::fs::rename(&part, &target));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

/// Removes folders emptied by a deletion, up to (not including) the device root.
fn remove_empty_parents(root: &Path, file: &Path) {
    let mut dir = file.parent();
    while let Some(current) = dir {
        if current == root || !current.starts_with(root) || std::fs::remove_dir(current).is_err() {
            break;
        }
        dir = current.parent();
    }
}

/// Writes one M3U8 per playlist, with paths relative to the playlist folder, and removes
/// playlist files from earlier syncs that are no longer wanted.
fn write_playlists(device: &mut Device, plan: &SyncPlan, failures: &mut Vec<String>) -> usize {
    let folder = device.manifest.settings.playlist_folder.trim_matches('/').to_owned();
    let depth = folder.split('/').filter(|p| !p.is_empty()).count();
    let up = "../".repeat(depth);
    let mut written = Vec::new();
    for playlist in &plan.playlists {
        let mut text = String::from("#EXTM3U\n");
        for id in &playlist.tracks {
            let Some(item) = plan.final_paths.get(id) else { continue };
            if !device.manifest.files.contains_key(id) {
                continue; // Its copy failed.
            }
            text.push_str(&format!("#EXTINF:{},{}\n{up}{}\n", item.duration.as_secs(), item.label, item.device_path));
        }
        let name = tunebox_model::organize::component(&playlist.name, "Playlist");
        let relative = format!("{folder}/{name}.m3u8");
        let path = device_path(&device.root, &relative);
        let result = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| craft_atomic::atomic_write(&path, text.as_bytes()));
        match result {
            Ok(()) => written.push(relative),
            Err(e) => failures.push(format!("Could not write playlist {}: {e}", playlist.name)),
        }
    }
    for stale in device.manifest.playlists.iter().filter(|p| !written.contains(p)) {
        let _ = std::fs::remove_file(device_path(&device.root, stale));
    }
    let count = written.len();
    device.manifest.playlists = written;
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{PlaylistFile, plan};
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
        assert_eq!(m3u, "#EXTM3U\n#EXTINF:61,Artist - Two\n../Music/Artist/Two.flac\n#EXTINF:61,Artist - One\n../Music/Artist/One.flac\n");

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

    #[test]
    fn device_paths_cannot_escape_the_root() {
        assert_eq!(device_path(Path::new("/dev/player"), "../../etc/passwd"), PathBuf::from("/dev/player/etc/passwd"));
    }
}
