//! Syncing MTP players, end to end against the simulated device.

use crate::device::Device;
use crate::discover::Access;
use crate::execute::execute_on;
use crate::mtp::{MtpLink, probe};
use crate::plan::{PlaylistFile, SyncItem, plan};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tunebox_model::TrackId;
use tunebox_mtp::codes::operation;
use tunebox_mtp::simulator::SimulatedDevice;

fn item(library: &Path, id: u64, name: &str, size: usize) -> SyncItem {
    let source = library.join(format!("{id}.flac"));
    std::fs::write(&source, vec![id as u8; size]).unwrap();
    SyncItem {
        id: TrackId(id),
        source,
        device_path: format!("Music/Artist/{name}.flac"),
        size: size as u64,
        modified: 5,
        available: true,
        label: format!("Artist - {name}"),
        duration: Duration::from_secs(61),
    }
}

/// Probes the simulated device and sets up its storage, as "Set Up for Syncing" does.
fn set_up(player: &SimulatedDevice) -> (crate::discover::Volume, Device) {
    let mut candidates = probe(&MtpLink::Simulated(player.clone()));
    assert_eq!(candidates.len(), 1);
    let candidate = candidates.remove(0);
    assert_eq!((candidate.device.as_ref(), candidate.problem.as_ref()), (None, None));
    assert!(matches!(candidate.volume.access, Access::Mtp(_)));
    let mut storage = candidate.volume.storage().unwrap();
    let device = Device::register_on(&mut *storage, &candidate.volume.root, "Walkman").unwrap();
    (candidate.volume, device)
}

fn sync(
    volume: &crate::discover::Volume,
    device: &mut Device,
    wanted: &[SyncItem],
    playlists: Vec<PlaylistFile>,
) -> Result<crate::SyncReport, crate::DeviceError> {
    let mut storage = volume.storage()?;
    let plan = plan(wanted, playlists, &device.manifest);
    execute_on(device, &mut *storage, &plan, &AtomicBool::new(false), &mut |_| {})
}

#[test]
fn syncs_then_resyncs_deleting_only_tunebox_files() {
    let library = tempfile::tempdir().unwrap();
    let player = SimulatedDevice::new("NW-A306", 1_000_000);
    player.put("Music/Mine/Own.mp3", b"the listener's own song");
    let (volume, mut device) = set_up(&player);
    let (one, two) = (item(library.path(), 1, "One", 300), item(library.path(), 2, "Two", 400));
    let mix = PlaylistFile { name: "Mix".into(), tracks: vec![TrackId(2), TrackId(1)] };

    let report = sync(&volume, &mut device, &[one.clone(), two.clone()], vec![mix]).unwrap();
    assert_eq!((report.copied, report.deleted, report.playlists_written), (2, 0, 1));
    assert_eq!(player.file("Music/Artist/Two.flac").unwrap(), vec![2u8; 400]);
    assert_eq!(
        String::from_utf8(player.file("Playlists/Mix.m3u8").unwrap()).unwrap(),
        "#EXTM3U\n#EXTINF:61,Artist - Two\n../Music/Artist/Two.flac\n#EXTINF:61,Artist - One\n../Music/Artist/One.flac\n"
    );
    let used: u64 = player.files().iter().map(|f| player.reported_size(f).unwrap()).sum();
    assert_eq!(report.free_bytes, Some(1_000_000 - used), "free space as the player reports it");
    assert!(device.manifest.pending.is_empty());

    // Plugged in again: the manifest is read back from the player.
    let probed = probe(&MtpLink::Simulated(player.clone())).remove(0);
    let mut device = probed.device.unwrap();
    assert_eq!(device.manifest.files.len(), 2);
    let report = sync(&volume, &mut device, &[one], Vec::new()).unwrap();
    assert_eq!((report.copied, report.deleted), (0, 1));
    let files = player.files();
    let (manifests, others): (Vec<&String>, Vec<&String>) = files.iter().partition(|f| f.starts_with(".tunebox/"));
    assert_eq!(others, ["Music/Artist/One.flac", "Music/Mine/Own.mp3"], "the listener's own file is kept");
    assert_eq!(manifests.len(), 1, "only the newest manifest is kept: {manifests:?}");
    assert!(!player.folders().contains(&"Playlists".to_string()), "emptied folders are removed");
}

fn manifest_size(player: &SimulatedDevice) -> u64 {
    let name = player.files().into_iter().find(|f| f.starts_with(".tunebox/")).unwrap();
    player.reported_size(&name).unwrap()
}

#[test]
fn a_transfer_cut_half_way_is_never_taken_for_a_synced_song() {
    let library = tempfile::tempdir().unwrap();
    let player = SimulatedDevice::new("Player", 1_000_000);
    let (volume, mut device) = set_up(&player);
    let songs = [item(library.path(), 1, "One", 3000), item(library.path(), 2, "Two", 5000)];

    // Unplugged 2,000 bytes into the second song (after the first and the manifest).
    let manifest_bytes = serde_json::to_vec_pretty(&device.manifest).unwrap().len() as u64;
    player.unplug_after(3000 + manifest_bytes + 100 + 2000);
    assert!(sync(&volume, &mut device, &songs, Vec::new()).is_err(), "the sync ends with an error");
    // The player kept the partial file, reporting the full size: it looks complete.
    let partial = player.file("Music/Artist/Two.flac").unwrap().len();
    assert!(partial > 0 && partial < 5000, "{partial}");
    assert_eq!(player.reported_size("Music/Artist/Two.flac"), Some(5000));

    // But the manifest on the player does not count it as synced, and lists it as unfinished.
    let mut device = probe(&MtpLink::Simulated(player.clone())).remove(0).device.unwrap();
    assert!(!device.manifest.files.contains_key(&TrackId(2)));
    assert!(device.manifest.pending.contains(&"Music/Artist/Two.flac".to_string()));

    // The next sync removes it first and copies the song again, completely.
    let report = sync(&volume, &mut device, &songs, Vec::new()).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(player.file("Music/Artist/Two.flac").unwrap(), vec![2u8; 5000]);
    assert_eq!(player.file("Music/Artist/One.flac").unwrap(), vec![1u8; 3000]);
    assert!(device.manifest.pending.is_empty());
    assert_eq!(device.manifest.files.len(), 2);
}

#[test]
fn out_of_space_fails_that_song_and_leaves_nothing_of_it() {
    let library = tempfile::tempdir().unwrap();
    let player = SimulatedDevice::new("Player", 6000);
    let (volume, mut device) = set_up(&player);
    let songs = [item(library.path(), 1, "One", 3000), item(library.path(), 2, "Two", 4000)];
    let report = sync(&volume, &mut device, &songs, Vec::new()).unwrap();
    assert_eq!(report.copied, 1);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].contains("full"), "{:?}", report.failures);
    assert_eq!(player.file("Music/Artist/Two.flac"), None);
    assert_eq!(device.manifest.files.keys().copied().collect::<Vec<_>>(), [TrackId(1)]);
}

#[test]
fn a_manifest_write_cut_off_falls_back_to_the_previous_one() {
    let player = SimulatedDevice::new("Player", 1_000_000);
    let (volume, mut device) = set_up(&player);
    device.manifest.name = "Renamed".into();
    player.unplug_after(10);
    let mut storage = volume.storage().unwrap();
    assert!(device.save_to(&mut *storage).is_err());
    drop(storage);
    let read = probe(&MtpLink::Simulated(player.clone())).remove(0).device.unwrap();
    assert_eq!(read.manifest.name, "Walkman", "the complete earlier manifest is used");
    // The next save tidies the broken copy away.
    let mut storage = volume.storage().unwrap();
    device.save_to(&mut *storage).unwrap();
    assert_eq!(player.files().len(), 1);
    assert_eq!(
        crate::storage::Storage::read_manifest(&mut *storage).unwrap().map(|b| b.len() as u64),
        Some(manifest_size(&player))
    );
}

#[test]
fn unplugged_between_songs_keeps_a_true_manifest() {
    let library = tempfile::tempdir().unwrap();
    let player = SimulatedDevice::new("Player", 10_000_000);
    let (volume, mut device) = set_up(&player);
    let songs: Vec<SyncItem> = (1..=30).map(|id| item(library.path(), id, &format!("Song {id}"), 1000)).collect();
    // Twenty songs and two manifests make it, then the cable comes out.
    let manifest_bytes = serde_json::to_vec_pretty(&device.manifest).unwrap().len() as u64;
    player.unplug_after(25_000 + 3 * manifest_bytes);
    assert!(sync(&volume, &mut device, &songs, Vec::new()).is_err());
    let device = probe(&MtpLink::Simulated(player.clone())).remove(0).device.unwrap();
    for (id, synced) in &device.manifest.files {
        let data = player.file(&synced.path).unwrap();
        assert_eq!(data, vec![id.0 as u8; 1000], "every song the manifest lists is complete");
    }
    assert!(!device.manifest.files.is_empty());
}

#[test]
fn existing_folders_are_reused_whatever_their_case() {
    let library = tempfile::tempdir().unwrap();
    let player = SimulatedDevice::new("Player", 1_000_000);
    player.put("MUSIC/readme.txt", b"x");
    let (volume, mut device) = set_up(&player);
    sync(&volume, &mut device, &[item(library.path(), 1, "One", 10)], Vec::new()).unwrap();
    assert!(player.files().contains(&"MUSIC/Artist/One.flac".to_string()), "{:?}", player.files());
    assert!(!player.folders().contains(&"Music".to_string()));
}

#[test]
fn folders_are_listed_once_per_connection() {
    let library = tempfile::tempdir().unwrap();
    let player = SimulatedDevice::new("Player", 1_000_000);
    let (volume, mut device) = set_up(&player);
    let songs: Vec<SyncItem> = (1..=10).map(|id| item(library.path(), id, &format!("Song {id}"), 10)).collect();
    let before = player.count(operation::GET_OBJECT_HANDLES);
    sync(&volume, &mut device, &songs, Vec::new()).unwrap();
    assert!(player.count(operation::GET_OBJECT_HANDLES) - before < 6);
}

#[test]
fn a_device_another_program_holds_is_listed_with_what_to_do() {
    let player = SimulatedDevice::new("Player", 1_000_000);
    player.hold(true);
    let candidates = probe(&MtpLink::Simulated(player.clone()));
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].problem.as_deref().unwrap().starts_with("Another program is using the player."));
    assert!(candidates[0].device.is_none());
    let Access::Mtp(address) = &candidates[0].volume.access else { panic!("not MTP") };
    assert!(matches!(crate::mtp::connect(address), Err(crate::DeviceError::Unavailable(_))));
}

#[test]
fn a_locked_phone_says_to_unlock_it() {
    let player = SimulatedDevice::new("Pixel", 1_000_000);
    player.lock_screen(true);
    let candidates = probe(&MtpLink::Simulated(player.clone()));
    assert!(candidates[0].problem.as_deref().unwrap().contains("unlock it"));
    player.lock_screen(false);
    let candidates = probe(&MtpLink::Simulated(player));
    assert_eq!(candidates[0].problem, None);
    assert_eq!(candidates[0].volume.label, "Simulated Pixel");
}

#[test]
fn each_storage_is_a_player_of_its_own() {
    let player = SimulatedDevice::new("Phone", 1_000_000);
    player.add_storage("SD card", 2_000_000);
    let candidates = probe(&MtpLink::Simulated(player));
    let labels: Vec<&str> = candidates.iter().map(|c| c.volume.label.as_str()).collect();
    assert_eq!(labels, ["Simulated Phone (Internal storage)", "Simulated Phone (SD card)"]);
    assert_ne!(candidates[0].volume.root, candidates[1].volume.root);
    assert_eq!(candidates[1].volume.total_bytes, 2_000_000);
}
