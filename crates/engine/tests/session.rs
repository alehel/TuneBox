//! The session end to end, headless: import, browse, play, rip a CD, sync a player.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tunebox_audio::{LosslessFormat, PcmWriter};
use tunebox_engine::Preferences;
use tunebox_engine::model::TrackId;
use tunebox_engine::model::{Column, NumberOf, Rating, SortOrder, TagField, TrackMeta};
use tunebox_engine::{
    Access, EngineError, FileStatus, LibraryOnlyEdits, LibrarySnapshot, NullOutput, Planned, PlayState, Player,
    PlayerModel, RenameBase, RenameConflicts, RenameRequest, RepeatMode, Session, SessionPaths, SkipReason, Source,
    SyncMode, SyncSettings, TrackQuery, VirtualDrive, Volume,
};

struct Fixture {
    dir: tempfile::TempDir,
    session: Session,
}

/// The library, preferences and media folder inside `dir`.
fn paths_in(dir: &tempfile::TempDir) -> SessionPaths {
    SessionPaths {
        library_file: dir.path().join("TuneBox/Library.json"),
        preferences_file: dir.path().join("TuneBox/Preferences.json"),
        default_media_folder: dir.path().join("TuneBox/Media"),
    }
}

fn fixture(speed: f64) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let session = Session::open(paths_in(&dir), Player::with_output(NullOutput { speed })).unwrap();
    Fixture { dir, session }
}

/// A short tagged FLAC.
fn write_song(path: &Path, title: &str, artist: &str, album: &str, track: u32) {
    write_song_lasting(path, title, artist, album, track, 0.5);
}

/// A tagged FLAC, `seconds` long.
fn write_song_lasting(path: &Path, title: &str, artist: &str, album: &str, track: u32, seconds: f64) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut writer = PcmWriter::create(path, LosslessFormat::Flac, 8_000, 1).unwrap();
    writer.write(&[vec![0.1; (seconds * 8_000.0) as usize]]).unwrap();
    writer.finish().unwrap();
    let meta = TrackMeta {
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        genre: "Ambient".into(),
        track: NumberOf::new(track, 2),
        ..TrackMeta::default()
    };
    tunebox_tags::write(path, &meta).unwrap();
}

/// Ticks the session until `done` holds (or a generous timeout).
fn tick_until(session: &mut Session, mut done: impl FnMut(&Session) -> bool) {
    let start = Instant::now();
    while !done(session) {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        session.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn import_two_songs(f: &mut Fixture) {
    let downloads = f.dir.path().join("Downloads");
    write_song(&downloads.join("b.flac"), "Second Light", "Quiet Harbour", "Lanterns", 2);
    write_song(&downloads.join("a.flac"), "First Light", "Quiet Harbour", "Lanterns", 1);
    f.session.import(vec![downloads]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 2);
}

fn copy_into_media_folder(f: &mut Fixture) {
    let mut prefs = f.session.preferences().clone();
    prefs.copy_to_media_folder = true;
    f.session.set_preferences(prefs);
}

#[test]
fn import_query_and_playlists() {
    let mut f = fixture(1.0);
    copy_into_media_folder(&mut f);
    import_two_songs(&mut f);
    let notices = f.session.take_notices();
    assert!(notices.iter().any(|n| n.text.contains("Added 2 songs")), "{notices:?}");

    let all = f.session.query(&TrackQuery::new(Source::Music));
    let titles: Vec<_> = all.iter().map(|id| f.session.track(*id).unwrap().title().to_owned()).collect();
    assert_eq!(titles, ["First Light", "Second Light"], "album order by default");
    let media = f.dir.path().join("TuneBox/Media/Quiet Harbour/Lanterns/01 First Light.flac");
    assert_eq!(f.session.track(all[0]).unwrap().location, media, "copied into the media folder");

    let mut query = TrackQuery::new(Source::Music);
    query.search = "second".into();
    assert_eq!(f.session.query(&query).len(), 1);
    query.search.clear();
    query.sort = Some((Column::Name, SortOrder::Descending));
    assert_eq!(f.session.query(&query), [all[1], all[0]]);

    let playlist = f.session.create_playlist("Evening", &[all[1], all[0]]).unwrap();
    assert_eq!(f.session.query(&TrackQuery::new(Source::Playlist(playlist))), [all[1], all[0]]);

    f.session.set_rating(&[all[0]], Rating::from_stars(5));
    let top = f.session.library().playlists().iter().find(|p| p.name == "My Top Rated").unwrap().id;
    assert_eq!(f.session.query(&TrackQuery::new(Source::Playlist(top))), [all[0]]);

    // Saved and reopened, everything is still there.
    let Fixture { dir, mut session } = f;
    session.shutdown().unwrap();
    drop(session);
    let reopened = fixture_at(&dir);
    assert_eq!(reopened.library().len(), 2);
    assert!(reopened.library().playlist(playlist).is_some());
}

fn fixture_at(dir: &tempfile::TempDir) -> Session {
    Session::open(paths_in(dir), Player::with_output(NullOutput::default())).unwrap()
}

#[test]
fn playing_through_counts_plays() {
    let mut f = fixture(50.0);
    import_two_songs(&mut f);
    let all = f.session.query(&TrackQuery::new(Source::Music));
    f.session.play_tracks(&all, 0);
    assert_eq!(f.session.now_playing().map(|n| n.track), Some(all[0]));
    tick_until(&mut f.session, |s| s.now_playing().is_none());
    for id in &all {
        assert_eq!(f.session.track(*id).unwrap().stats.play_count, 1);
    }
}

/// Imports an album of `count` songs, each `seconds` long, and returns them in album order.
fn import_album(f: &mut Fixture, count: u32, seconds: f64) -> Vec<TrackId> {
    let folder = f.dir.path().join("Album");
    for n in 1..=count {
        write_song_lasting(&folder.join(format!("{n}.flac")), &format!("Part {n}"), "Tone", "Suite", n, seconds);
    }
    f.session.import(vec![folder]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == count as usize);
    f.session.query(&TrackQuery::new(Source::Music))
}

/// Ticks until `done`, noting each song heard and asserting that playback never stops in
/// between: no gap where the player was stopped and the session loaded the next song.
fn songs_heard_until(session: &mut Session, mut done: impl FnMut(&Session) -> bool) -> Vec<TrackId> {
    let mut heard: Vec<TrackId> = Vec::new();
    let mut stopped = false;
    tick_until(session, |s| {
        if let Some(now) = s.now_playing() {
            // Stopped before the first song starts, or after the last, is fine.
            stopped |= !heard.is_empty() && now.state == PlayState::Stopped;
            if now.state == PlayState::Playing && heard.last() != Some(&now.track) {
                assert!(!stopped, "playback stopped before {:?} started, after {heard:?}", now.track);
                heard.push(now.track);
            }
        }
        done(s)
    });
    heard
}

#[test]
fn an_album_plays_through_without_stopping_between_songs() {
    let mut f = fixture(8.0);
    let album = import_album(&mut f, 3, 1.0);
    f.session.play_tracks(&album, 0);
    let heard = songs_heard_until(&mut f.session, |s| s.now_playing().is_none());
    assert_eq!(heard, album);
    for id in &album {
        assert_eq!(f.session.track(*id).unwrap().stats.play_count, 1, "one play each");
    }
    assert!(f.session.take_notices().iter().all(|n| !n.text.contains("Could not play")));
}

#[test]
fn turning_repeat_on_queues_the_first_song_again() {
    let mut f = fixture(4.0);
    let album = import_album(&mut f, 2, 2.0);
    // The last song: on its own it would end the queue.
    f.session.play_tracks(&album, 1);
    f.session.cycle_repeat();
    assert_eq!(f.session.repeat(), RepeatMode::All);
    let heard = songs_heard_until(&mut f.session, |s| s.now_playing().is_some_and(|n| n.track == album[0]));
    assert_eq!(heard, [album[1], album[0]]);
    // Repeat One: the song goes straight on into itself.
    f.session.cycle_repeat();
    let heard = songs_heard_until(&mut f.session, |s| s.track(album[0]).unwrap().stats.play_count == 1);
    assert_eq!(heard, [album[0]]);
    assert_eq!(f.session.now_playing().map(|n| (n.track, n.state)), Some((album[0], PlayState::Playing)));
    f.session.stop();
}

#[test]
fn turning_shuffle_off_queues_the_next_song_in_list_order() {
    let mut f = fixture(4.0);
    let album = import_album(&mut f, 4, 2.0);
    f.session.set_shuffle(true);
    f.session.play_tracks(&album, 0);
    f.session.set_shuffle(false);
    let heard = songs_heard_until(&mut f.session, |s| s.now_playing().is_some_and(|n| n.track != album[0]));
    assert_eq!(heard, [album[0], album[1]]);
    f.session.stop();
}

#[test]
fn a_song_that_goes_missing_or_is_unchecked_is_not_played_next() {
    let mut f = fixture(4.0);
    let album = import_album(&mut f, 4, 2.0);
    f.session.play_tracks(&album, 0);
    // While the first plays, the third's file goes, then the second (already queued in the
    // player) is unchecked: the player is told to go on to the fourth, never the third.
    let third = f.session.track(album[2]).unwrap().location.clone();
    std::fs::remove_file(third).unwrap();
    f.session.check_files_now();
    assert_eq!(*f.session.file_status(album[2]), FileStatus::Missing);
    f.session.set_checked(&[album[1]], false);
    let heard = songs_heard_until(&mut f.session, |s| s.now_playing().is_some_and(|n| n.track != album[0]));
    assert_eq!(heard, [album[0], album[3]]);
    let notices = f.session.take_notices();
    assert!(notices.iter().all(|n| !n.text.contains("Could not play")), "{notices:?}");
    f.session.stop();
}

#[test]
fn a_song_that_cannot_be_opened_is_reported_and_passed_over() {
    let mut f = fixture(8.0);
    let album = import_album(&mut f, 3, 1.0);
    // Damaged after import, so the session takes it for playable until the player tries it.
    std::fs::write(&f.session.track(album[1]).unwrap().location, b"not audio").unwrap();
    f.session.play_tracks(&album, 0);
    let heard = songs_heard_until(&mut f.session, |s| s.now_playing().is_none());
    assert_eq!(heard, [album[0], album[2]]);
    let failures: Vec<_> = f.session.take_notices().into_iter().filter(|n| n.text.contains("Could not play")).collect();
    assert_eq!(failures.len(), 1, "reported once: {failures:?}");
    assert_eq!(f.session.track(album[1]).unwrap().stats.play_count, 0);
}

#[test]
fn rips_a_cd_into_the_library() {
    let mut f = fixture(1.0);
    f.session.read_disc(Arc::new(VirtualDrive::new("Virtual Drive", vec![1, 2, 1])));
    tick_until(&mut f.session, |s| s.cd().is_some());
    let mut info = f.session.cd().unwrap().info.clone();
    info.album = "Field Recordings".into();
    info.artist = "Tone Generator".into();
    f.session.set_disc_info(info);
    f.session.set_cd_track_included(1, false);
    f.session.import_cd().unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 2);
    let cd = f.session.cd().unwrap();
    assert_eq!(cd.imported.iter().copied().collect::<Vec<_>>(), [1, 3]);
    let album: Vec<_> = f.session.library().tracks().map(|t| t.meta.album.clone()).collect();
    assert_eq!(album, ["Field Recordings", "Field Recordings"]);
}

#[test]
fn rips_with_the_drive_read_offset_saved_in_preferences() {
    let mut f = fixture(1.0);
    // A drive that reads 667 samples early, holding a disc whose track 1 starts at sector 0.
    let exact = VirtualDrive::new("Early Drive", vec![1, 1]).with_pregap(0);
    f.session.read_disc(Arc::new(exact.clone().with_read_offset(667)));
    tick_until(&mut f.session, |s| s.cd().is_some());
    assert_eq!(f.session.cd_read_offset().map(|(_, offset)| offset), Some(0));
    assert!(matches!(f.session.set_cd_read_offset(100_000), Err(EngineError::ReadOffset(_))));
    f.session.set_cd_read_offset(667).unwrap();
    let (drive, offset) = f.session.cd_read_offset().unwrap();
    assert_eq!((drive.key(), offset), ("Early Drive", 667));

    // Saved with the preferences.
    let saved = Preferences::load(&paths_in(&f.dir).preferences_file, Path::new(""));
    assert_eq!(saved.read_offset("Early Drive"), 667);

    f.session.set_cd_track_included(1, false);
    f.session.import_cd().unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 1);
    let ripped = f.session.library().tracks().next().unwrap().location.clone();
    let mut decoder = tunebox_audio::Decoder::open(&ripped).unwrap();
    let mut samples = Vec::new();
    while let Some(block) = decoder.next_block().unwrap() {
        samples.extend(block.samples.iter().map(|s| (s * 32_768.0).round() as i16));
    }
    let truth: Vec<i16> =
        exact.true_audio(0, 75 * 588).chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    assert_eq!(samples, truth, "track 1 is sample-exact");

    // Setting 0 forgets the drive.
    f.session.set_cd_read_offset(0).unwrap();
    assert!(f.session.preferences().read_offsets.is_empty());
}

#[test]
fn syncs_a_player() {
    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    let player_root = f.dir.path().join("PLAYER");
    std::fs::create_dir_all(player_root.join("Music")).unwrap();
    let volume = Volume {
        root: player_root.clone(),
        label: "PLAYER".into(),
        total_bytes: 1 << 30,
        free_bytes: 1 << 30,
        removable: true,
        access: Access::Folder,
    };
    f.session.refresh_devices(&[volume]);
    assert_eq!(f.session.devices().len(), 1);
    f.session.register_device(&player_root, "Pocket", PlayerModel::Generic).unwrap();

    let all = f.session.query(&TrackQuery::new(Source::Music));
    let only_first = f.session.create_playlist("Commute", &[all[0]]).unwrap();
    let settings = SyncSettings { mode: SyncMode::Playlists(vec![only_first]), ..SyncSettings::default() };
    f.session.set_device_settings(&player_root, settings).unwrap();
    f.session.sync_device(&player_root).unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty());

    assert!(player_root.join("Music/Quiet Harbour/Lanterns/01 First Light.flac").exists());
    assert!(!player_root.join("Music/Quiet Harbour/Lanterns/02 Second Light.flac").exists());
    assert!(player_root.join("Playlists/Commute.m3u8").exists());
    assert_eq!(f.session.query(&TrackQuery::new(Source::Device(player_root))), [all[0]]);
}

#[test]
fn a_blank_card_is_set_up_for_a_snowsky_echo_and_synced_for_it() {
    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    // An AIFF song, which the Echo players do not play.
    let aiff = f.dir.path().join("More/c.aiff");
    std::fs::create_dir_all(aiff.parent().unwrap()).unwrap();
    let mut writer = PcmWriter::create(&aiff, LosslessFormat::Aiff, 8_000, 1).unwrap();
    writer.write(&[vec![0.1; 4_000]]).unwrap();
    writer.finish().unwrap();
    f.session.import(vec![aiff.parent().unwrap().to_path_buf()]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 3);

    // A blank microSD card in a reader: removable, but nothing marks it as a player.
    let card = f.dir.path().join("CARD");
    std::fs::create_dir_all(&card).unwrap();
    let volume = Volume {
        root: card.clone(),
        label: "NO NAME".into(),
        total_bytes: 1 << 30,
        free_bytes: 1 << 30,
        removable: true,
        access: Access::Folder,
    };
    f.session.refresh_devices(std::slice::from_ref(&volume));
    assert!(f.session.devices().is_empty(), "a blank card is not listed as a player");
    assert_eq!(f.session.drives().len(), 1, "but can be set up as one");

    f.session.register_device(&card, "Echo", PlayerModel::SnowskyEchoMini).unwrap();
    assert!(f.session.drives().is_empty());
    let settings = f.session.devices()[0].device.as_ref().unwrap().manifest.settings.clone();
    assert_eq!(settings.player, PlayerModel::SnowskyEchoMini);
    assert!(!settings.write_playlists, "the Echo may not read playlists: off by default");
    // Taken out and put back: the card says what it is.
    f.session.refresh_devices(&[]);
    f.session.refresh_devices(&[volume]);
    assert_eq!(f.session.devices()[0].name(), "Echo");

    f.session.sync_device(&card).unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    assert!(card.join("Music/Quiet Harbour/Lanterns/01 First Light.flac").exists());
    assert!(card.join("Music/Quiet Harbour/Lanterns/02 Second Light.flac").exists());
    assert!(!card.join("Playlists").exists());
    let manifest = tunebox_engine::Device::open(&card).unwrap().unwrap().manifest;
    assert_eq!(manifest.files.len(), 2, "the AIFF song is left off");
    let notices: Vec<String> = f.session.take_notices().into_iter().map(|n| n.text).collect();
    assert!(
        notices.iter().any(|n| n.contains("1 song was left off: the Snowsky Echo Mini cannot play its format.")),
        "{notices:?}"
    );
}

#[test]
fn mtp_settings_are_saved_in_the_background_latest_last() {
    use tunebox_mtp::simulator::SimulatedDevice;
    use tunebox_sync::mtp::{MtpLink, probe};

    let mut f = fixture(1.0);
    let player = SimulatedDevice::new("NW-A306", 1 << 30);
    f.session.refresh_mtp_devices(&probe(&MtpLink::Simulated(player.clone())));
    let root = f.session.devices()[0].volume.root.clone();
    f.session.register_device(&root, "Walkman", PlayerModel::Generic).unwrap();
    let on_player = || probe(&MtpLink::Simulated(player.clone())).remove(0).device.unwrap().manifest;

    // Both return at once and show at once; the player is written by jobs, one at a time.
    f.session.rename_device(&root, "First").unwrap();
    f.session.rename_device(&root, "Second").unwrap();
    let settings = SyncSettings { only_checked: false, ..SyncSettings::default() };
    f.session.set_device_settings(&root, settings.clone()).unwrap();
    assert_eq!(f.session.devices()[0].name(), "Second");
    assert!(f.session.is_busy(tunebox_engine::JobKind::SaveDevice));
    assert!(f.session.sync_device(&root).is_err(), "a sync waits for the save, which holds the player");
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    let manifest = on_player();
    assert_eq!((manifest.name.as_str(), &manifest.settings), ("Second", &settings), "the latest change wins");

    // A save that fails says so, and that the next sync will write it.
    player.hold(true);
    f.session.rename_device(&root, "Third").unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    let notices: Vec<String> = f.session.take_notices().into_iter().map(|n| n.text).collect();
    assert!(notices.iter().any(|n| n.contains("will be saved with the next sync")), "{notices:?}");
    player.hold(false);
    f.session.sync_device(&root).unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    assert_eq!(on_player().name, "Third");
}

#[test]
fn syncs_an_mtp_player() {
    use tunebox_mtp::simulator::SimulatedDevice;
    use tunebox_sync::mtp::{MtpLink, probe};

    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    let player = SimulatedDevice::new("NW-A306", 1 << 30);
    let found = probe(&MtpLink::Simulated(player.clone()));
    f.session.refresh_mtp_devices(&found);
    // Mounted volumes coming and going leave MTP players alone.
    f.session.refresh_devices(&[]);
    assert_eq!(f.session.devices().len(), 1);
    let root = f.session.devices()[0].volume.root.clone();
    assert_eq!(f.session.devices()[0].name(), "Simulated NW-A306");
    f.session.register_device(&root, "Walkman", PlayerModel::Generic).unwrap();
    // The watcher keeps reporting what it found at plug-in; the session keeps what it knows.
    f.session.refresh_mtp_devices(&found);
    assert_eq!(f.session.devices()[0].name(), "Walkman");

    f.session.sync_device(&root).unwrap();
    // The sync holds the player: changing its settings waits (instead of calling it busy).
    let busy = f.session.rename_device(&root, "Later").unwrap_err().to_string();
    assert_eq!(busy, "A sync is already in progress");
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    let files = player.files();
    assert!(files.contains(&"Music/Quiet Harbour/Lanterns/01 First Light.flac".to_string()), "{files:?}");
    assert!(files.contains(&"Music/Quiet Harbour/Lanterns/02 Second Light.flac".to_string()));
    let all = f.session.query(&TrackQuery::new(Source::Music));
    assert_eq!(f.session.query(&TrackQuery::new(Source::Device(root.clone()))).len(), all.len());
    let free = f.session.devices()[0].volume.free_bytes;
    assert!(free < 1 << 30, "free space is updated from the player");

    // Held by another program: the next sync ends with an error saying what to do.
    player.hold(true);
    f.session.sync_device(&root).unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    let notices: Vec<String> = f.session.take_notices().into_iter().map(|n| n.text).collect();
    assert!(notices.iter().any(|n| n.contains("Another program is using the player")), "{notices:?}");

    // Unplugged for good: the player leaves the list.
    f.session.refresh_mtp_devices(&[]);
    assert!(f.session.devices().is_empty());
}

/// Every file in `dir`, with its bytes, for before/after comparisons.
fn snapshot(dir: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).unwrap().flatten() {
            if entry.path().is_dir() {
                pending.push(entry.path());
            } else {
                files.push((entry.path(), std::fs::read(entry.path()).unwrap()));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn by_default_songs_are_played_where_they_are() {
    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    let all = f.session.query(&TrackQuery::new(Source::Music));
    let location = &f.session.track(all[0]).unwrap().location;
    assert!(location.starts_with(f.dir.path().join("Downloads")), "{}", location.display());
    assert!(!f.dir.path().join("TuneBox/Media").exists(), "nothing is created in the media folder");
}

#[test]
fn a_read_only_library_folder_is_never_changed() {
    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    let downloads = f.dir.path().join("Downloads");
    let mut prefs = f.session.preferences().clone();
    prefs.add_library_folder(downloads.clone(), true);
    f.session.set_preferences(prefs.clone());
    let before = snapshot(&downloads);

    // Editing details changes the library only.
    let all = f.session.query(&TrackQuery::new(Source::Music));
    let mut meta = f.session.track(all[0]).unwrap().meta.clone();
    meta.title = "Renamed".into();
    f.session.edit_track(all[0], meta).unwrap();
    assert_eq!(f.session.track(all[0]).unwrap().title(), "Renamed");

    // Deleting removes the song from the library, never the file.
    f.session.remove_tracks(&[all[1]], true);
    assert_eq!(f.session.library().len(), 1);
    assert_eq!(snapshot(&downloads), before, "the protected folder is byte-for-byte unchanged");

    // Copies and rips may not be filed inside it.
    prefs.media_folder = downloads.join("TuneBox Media");
    prefs.copy_to_media_folder = true;
    f.session.set_preferences(prefs);
    assert!(f.session.import(vec![downloads.clone()]).is_err());
    f.session.read_disc(Arc::new(VirtualDrive::new("Virtual Drive", vec![1])));
    tick_until(&mut f.session, |s| s.cd().is_some());
    assert!(f.session.import_cd().is_err());
    assert_eq!(snapshot(&downloads), before);
}

#[test]
fn a_writable_imports_folder_inside_a_read_only_collection() {
    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    let collection = f.dir.path().join("Downloads");
    let imports = collection.join("Imports");
    let mut prefs = f.session.preferences().clone();
    prefs.add_library_folder(collection.clone(), true);
    prefs.add_library_folder(imports.clone(), false);
    prefs.media_folder = imports.clone();
    f.session.set_preferences(prefs);
    let before = snapshot(&collection);

    // CD imports go to the writable folder…
    f.session.read_disc(Arc::new(VirtualDrive::new("Virtual Drive", vec![1])));
    tick_until(&mut f.session, |s| s.cd().is_some());
    f.session.import_cd().unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 3);
    let ripped = snapshot(&imports);
    assert_eq!(ripped.len(), 1);
    // …and nothing else in the collection changed.
    let mut after = snapshot(&collection);
    after.retain(|(path, _)| !path.starts_with(&imports));
    assert_eq!(after, before);

    // Songs dropped into a library folder outside TuneBox are picked up by a scan.
    write_song(&collection.join("New/late.flac"), "Late Arrival", "Quiet Harbour", "Lanterns", 3);
    f.session.scan_library_folders().unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 4);
}

#[test]
fn additional_fields_are_imported_edited_and_kept() {
    let mut f = fixture(1.0);
    let downloads = f.dir.path().join("Downloads");
    let path = downloads.join("nocturne.flac");
    write_song(&path, "Nocturne", "Quiet Harbour", "Lanterns", 1);
    let mut tagged = tunebox_tags::read(&path).unwrap().meta;
    tagged.extra.insert(TagField::Conductor, "C. Baton".into());
    tagged.extra.insert(TagField::Label, "Harbour Records".into());
    tunebox_tags::write(&path, &tagged).unwrap();
    f.session.import(vec![downloads]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 1);

    let id = f.session.query(&TrackQuery::new(Source::Music))[0];
    let mut meta = f.session.track(id).unwrap().meta.clone();
    assert_eq!(meta.extra(TagField::Conductor), "C. Baton", "read on import");
    meta.set("label", "").unwrap();
    meta.set("work", "Night Pieces").unwrap();
    // FLAC has no place for a composer sort name, and a date must be a date: neither is kept.
    meta.extra.insert(TagField::SortComposer, "Baton, C.".into());
    meta.extra.insert(TagField::ReleaseDate, "spring".into());
    f.session.edit_track(id, meta).unwrap();

    for back in [tunebox_tags::read(&path).unwrap().meta, f.session.track(id).unwrap().meta.clone()] {
        assert_eq!(back.extra(TagField::Conductor), "C. Baton");
        assert_eq!(back.extra(TagField::Label), "");
        assert_eq!(back.extra(TagField::Work), "Night Pieces");
        assert_eq!(back.extra(TagField::SortComposer), "");
        assert_eq!(back.extra(TagField::ReleaseDate), "");
    }
}

#[test]
fn a_scan_reads_the_additional_fields_of_songs_added_before_they_were_read() {
    let mut f = fixture(1.0);
    let music = f.dir.path().join("Music");
    let path = music.join("nocturne.flac");
    write_song(&path, "Nocturne", "Quiet Harbour", "Lanterns", 1);
    let mut tagged = tunebox_tags::read(&path).unwrap().meta;
    tagged.extra.insert(TagField::Conductor, "C. Baton".into());
    tagged.extra.insert(TagField::Label, "Harbour Records".into());
    tunebox_tags::write(&path, &tagged).unwrap();
    f.session.import(vec![music.clone()]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 1);
    let Fixture { dir, mut session } = f;
    session.shutdown().unwrap();
    drop(session);

    // Make the library file look as an older TuneBox left it: no additional fields, no marker.
    let library_file = dir.path().join("TuneBox/Library.json");
    let mut json: serde_json::Value = serde_json::from_slice(&std::fs::read(&library_file).unwrap()).unwrap();
    for track in json["tracks"].as_array_mut().unwrap() {
        let track = track.as_object_mut().unwrap();
        track.remove("extra_fields_read");
        track["meta"].as_object_mut().unwrap().remove("extra");
    }
    std::fs::write(&library_file, serde_json::to_vec(&json).unwrap()).unwrap();

    let mut session = fixture_at(&dir);
    let id = session.query(&TrackQuery::new(Source::Music))[0];
    assert_eq!(session.track(id).unwrap().meta.extra(TagField::Conductor), "", "not read yet");
    // An edit made before the scan, here kept in TuneBox only, must survive it.
    let mut prefs = session.preferences().clone();
    prefs.add_library_folder(music.clone(), true);
    session.set_preferences(prefs);
    let mut meta = session.track(id).unwrap().meta.clone();
    meta.set("label", "").unwrap();
    session.edit_track(id, meta).unwrap();
    session.take_notices();

    let scan = |session: &mut Session| {
        session.scan_library_folders().unwrap();
        tick_until(session, |s| s.jobs().is_empty());
        session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ")
    };
    let first = scan(&mut session);
    let track = session.track(id).unwrap();
    assert_eq!(track.meta.extra(TagField::Conductor), "C. Baton");
    assert_eq!(track.meta.extra(TagField::Label), "", "the listener's edit is kept");
    assert!(track.extra_fields_read);
    assert!(first.contains("1 song"), "{first}");
    // Read once: the next scan leaves it alone.
    let second = scan(&mut session);
    assert!(!second.contains("details"), "{second}");
}

#[test]
fn refresh_from_files_picks_up_changes_made_by_other_programs() {
    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    let ids = f.session.query(&TrackQuery::new(Source::Music));
    let (first, second) = (ids[0], ids[1]);
    f.session.set_rating(&[first], Rating::from_stars(4));
    f.session.take_notices();
    let refresh = |session: &mut Session, ids: &[_], edits| {
        session.refresh_from_files(ids, edits).unwrap();
        tick_until(session, |s| s.jobs().is_empty());
        session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ")
    };

    // After an edit TuneBox wrote itself, file and library agree.
    let mut meta = f.session.track(first).unwrap().meta.clone();
    meta.genre = "Dream Pop".into();
    f.session.edit_track(first, meta).unwrap();
    let path = f.session.track(first).unwrap().location.clone();
    assert_eq!(f.session.track(first).unwrap().file.size_bytes, std::fs::metadata(&path).unwrap().len());
    let notice = refresh(&mut f.session, &[first], LibraryOnlyEdits::Keep);
    assert_eq!(notice, "The song already matched its file.");

    // Another program retags the first song.
    let mut retagged = tunebox_tags::read(&path).unwrap().meta;
    retagged.title = "First Light (Remastered)".into();
    retagged.extra.insert(TagField::Conductor, "C. Baton".into());
    tunebox_tags::write(&path, &retagged).unwrap();
    let before = snapshot(&f.dir.path().join("Downloads"));
    let notice = refresh(&mut f.session, &ids, LibraryOnlyEdits::Keep);
    let track = f.session.track(first).unwrap();
    assert_eq!(track.title(), "First Light (Remastered)");
    assert_eq!(track.meta.extra(TagField::Conductor), "C. Baton");
    assert_eq!(track.stats.rating, Rating::from_stars(4), "TuneBox's own data is kept");
    assert!(notice.starts_with("Updated 1 of 2 songs"), "{notice}");
    assert_eq!(snapshot(&f.dir.path().join("Downloads")), before, "refreshing only reads");

    // An edit kept in TuneBox only (read-only folder) is kept unless the listener says otherwise.
    let mut prefs = f.session.preferences().clone();
    prefs.add_library_folder(f.dir.path().join("Downloads"), true);
    f.session.set_preferences(prefs);
    let mut meta = f.session.track(second).unwrap().meta.clone();
    meta.title = "Renamed in TuneBox".into();
    f.session.edit_track(second, meta).unwrap();
    assert_eq!(f.session.library_only_edits(&ids), 1);
    let notice = refresh(&mut f.session, &ids, LibraryOnlyEdits::Keep);
    assert_eq!(f.session.track(second).unwrap().title(), "Renamed in TuneBox");
    assert!(notice.contains("1 song with changes kept in TuneBox only"), "{notice}");
    refresh(&mut f.session, &ids, LibraryOnlyEdits::Replace);
    assert_eq!(f.session.track(second).unwrap().title(), "Second Light");
    assert_eq!(f.session.library_only_edits(&ids), 0);

    // A missing file leaves the song as it is, with a warning.
    std::fs::rename(&path, path.with_extension("moved")).unwrap();
    let notice = refresh(&mut f.session, &[first], LibraryOnlyEdits::Keep);
    assert_eq!(f.session.track(first).unwrap().title(), "First Light (Remastered)");
    assert!(notice.contains("could not be read"), "{notice}");
}

#[test]
fn watched_folder_changes_add_read_and_relink_songs_without_losing_any() {
    let mut f = fixture(1.0);
    let music = f.dir.path().join("Music");
    std::fs::create_dir_all(&music).unwrap();
    let mut prefs = f.session.preferences().clone();
    prefs.add_library_folder(music.clone(), false);
    f.session.set_preferences(prefs);
    let settle = |session: &mut Session, paths: Vec<std::path::PathBuf>| {
        session.folders_changed(paths);
        tick_until(session, |s| s.jobs().is_empty());
        session.tick();
        tick_until(session, |s| s.jobs().is_empty());
        session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ")
    };

    // A song appears.
    let album = music.join("Lanterns");
    let path = album.join("first.flac");
    write_song(&path, "First Light", "Quiet Harbour", "Lanterns", 1);
    let notice = settle(&mut f.session, vec![album.clone(), path.clone()]);
    assert_eq!(f.session.library().len(), 1);
    assert_eq!(notice, "Added 1 song that appeared in your library folders.");
    let id = f.session.query(&TrackQuery::new(Source::Music))[0];
    f.session.set_rating(&[id], Rating::from_stars(5));

    // TuneBox's own edit comes back as a change: read, and nothing said.
    let mut meta = f.session.track(id).unwrap().meta.clone();
    meta.genre = "Dream Pop".into();
    f.session.edit_track(id, meta).unwrap();
    f.session.take_notices();
    assert_eq!(settle(&mut f.session, vec![path.clone()]), "");

    // Another program retags it.
    let mut retagged = tunebox_tags::read(&path).unwrap().meta;
    retagged.title = "First Light (Live)".into();
    tunebox_tags::write(&path, &retagged).unwrap();
    assert_eq!(settle(&mut f.session, vec![path.clone()]), "Updated 1 song changed by another program.");
    assert_eq!(f.session.track(id).unwrap().title(), "First Light (Live)");

    // The album folder is renamed: the song is relinked, not duplicated.
    let renamed = music.join("Lanterns (1997)");
    std::fs::rename(&album, &renamed).unwrap();
    let notice = settle(&mut f.session, vec![album.clone(), renamed.clone(), path.clone()]);
    assert_eq!(f.session.library().len(), 1, "{notice}");
    let track = f.session.track(id).unwrap();
    assert_eq!(track.location, renamed.join("first.flac"));
    assert_eq!(track.stats.rating, Rating::from_stars(5));
    assert!(notice.contains("Found 1 song that had moved"), "{notice}");

    // TuneBox's temporary files are not songs; a deleted song keeps its place and history.
    let temp = renamed.join(".tunebox-edit-first.flac");
    std::fs::copy(renamed.join("first.flac"), &temp).unwrap();
    assert_eq!(settle(&mut f.session, vec![temp]), "");
    std::fs::remove_file(renamed.join("first.flac")).unwrap();
    assert_eq!(settle(&mut f.session, vec![renamed.join("first.flac")]), "");
    assert_eq!(f.session.library().len(), 1);
    assert_eq!(f.session.track(id).unwrap().stats.rating, Rating::from_stars(5));
}

#[test]
fn a_library_open_in_one_tunebox_cannot_be_opened_in_another() {
    let f = fixture(1.0);
    let Err(error) = Session::open(paths_in(&f.dir), Player::with_output(NullOutput::default())) else {
        panic!("a second session opened the same library");
    };
    assert!(error.to_string().contains("already open in another TuneBox"), "{error}");
    let Fixture { dir, session } = f;
    drop(session);
    assert!(Session::open(paths_in(&dir), Player::with_output(NullOutput::default())).is_ok());
}

#[test]
fn a_snapshot_reads_a_library_another_tunebox_has_open_and_never_writes() {
    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    f.session.save().unwrap();

    let snapshot = LibrarySnapshot::open(&paths_in(&f.dir)).unwrap();
    let view = snapshot.view();
    let songs = view.query(&TrackQuery::new(Source::Music));
    assert_eq!(songs.len(), 2);
    assert_eq!(view.summary(&songs).count, 2);

    // It sees what was last saved, not the session's unsaved changes.
    f.session.set_rating(&[songs[0]], Rating::from_stars(3));
    assert_eq!(view.track(songs[0]).unwrap().stats.rating, Rating::UNRATED);

    // A damaged library is reported and left where it is: setting it aside is a write.
    let Fixture { dir, session } = f;
    drop(session);
    let library_file = paths_in(&dir).library_file;
    std::fs::write(&library_file, b"{ not json").unwrap();
    let Err(error) = LibrarySnapshot::open(&paths_in(&dir)) else { panic!("opened a damaged library") };
    assert!(error.to_string().contains("damaged"), "{error}");
    assert_eq!(std::fs::read(&library_file).unwrap(), b"{ not json");
}

#[test]
fn songs_on_a_share_that_is_not_mounted_are_unavailable_not_missing() {
    let mut f = fixture(1.0);
    let nas = f.dir.path().join("nas");
    let local = f.dir.path().join("local");
    write_song(&nas.join("one.flac"), "Harbour One", "Quiet Harbour", "Shared", 1);
    write_song(&nas.join("two.flac"), "Harbour Two", "Quiet Harbour", "Shared", 2);
    write_song(&local.join("three.flac"), "Harbour Three", "Quiet Harbour", "Local", 1);
    f.session.import(vec![nas.clone(), local.clone()]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 3);
    let id_at = |s: &Session, path: &Path| s.library().track_at(path).unwrap();
    let (one, two, three) = (
        id_at(&f.session, &nas.join("one.flac")),
        id_at(&f.session, &nas.join("two.flac")),
        id_at(&f.session, &local.join("three.flac")),
    );

    // The share is mounted: TuneBox remembers it as a network place, in the library file.
    f.session.set_network_mounts(vec![nas.clone()]);
    assert_eq!(f.session.library().network_places(), std::slice::from_ref(&nas));

    // A local song deleted is missing.
    std::fs::remove_file(local.join("three.flac")).unwrap();
    f.session.folders_changed(vec![local.join("three.flac")]);
    f.session.folders_changed(Vec::new());
    tick_until(&mut f.session, |s| *s.file_status(three) == FileStatus::Missing);
    f.session.take_notices();

    // The share is unmounted. Its mount point stays behind as a folder (here, still full), yet
    // its songs are unavailable, not missing, and nothing under it is touched.
    f.session.set_network_mounts(Vec::new());
    assert!(matches!(f.session.file_status(one), FileStatus::Unavailable(place) if place.network && place.root == nas));
    assert!(matches!(f.session.file_status(two), FileStatus::Unavailable(_)));
    let notice = f.session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ");
    assert_eq!(notice, "“nas” is not connected. Its 2 songs show as unavailable until it is back.");

    // Playing one says why it cannot; playing the list skips what cannot play.
    f.session.play_tracks(&[one], 0);
    assert!(f.session.now_playing().is_none());
    let notice = f.session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ");
    assert!(notice.contains("cannot be played. It is on “nas”, which is not connected"), "{notice}");

    // A copy of a song from the share is not taken for the song having moved: the share is
    // only away. (Relinking it would strand the original.)
    std::fs::copy(nas.join("two.flac"), local.join("two copy.flac")).unwrap();
    f.session.import(vec![local.join("two copy.flac")]).unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty() && s.library().len() == 4);
    assert_eq!(f.session.track(two).unwrap().location, nas.join("two.flac"));

    // Mounted again: connected, and its songs are there.
    f.session.take_notices();
    f.session.set_network_mounts(vec![nas.clone()]);
    tick_until(&mut f.session, |s| *s.file_status(one) == FileStatus::Present);
    let notice = f.session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ");
    assert_eq!(notice, "“nas” is connected again.");
    assert_eq!(*f.session.file_status(three), FileStatus::Missing, "still missing: it really is gone");
}

#[test]
fn locating_one_moved_song_finds_the_others_that_moved_alike() {
    let mut f = fixture(1.0);
    let album = f.dir.path().join("Music/Lanterns");
    for (n, title) in [(1, "First Light"), (2, "Second Light"), (3, "Third Light")] {
        write_song(&album.join(format!("{n:02}.flac")), title, "Quiet Harbour", "Lanterns", n);
    }
    f.session.import(vec![album.clone()]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 3);
    let id = |s: &Session, n: u32| s.library().track_at(&album.join(format!("{n:02}.flac"))).unwrap();
    let (first, second, third) = (id(&f.session, 1), id(&f.session, 2), id(&f.session, 3));
    f.session.set_rating(&[first], Rating::from_stars(5));

    // The folder is renamed outside TuneBox, and a different song takes the third one's name.
    let renamed = f.dir.path().join("Music/Lanterns (1997)");
    std::fs::rename(&album, &renamed).unwrap();
    write_song(&renamed.join("03.flac"), "Something Else Entirely, Much Longer", "Other", "Other", 9);
    std::fs::write(renamed.join("notes.txt"), "not music").unwrap();
    f.session.check_files_now();
    assert_eq!(*f.session.file_status(first), FileStatus::Missing);
    f.session.take_notices();

    // Not a song, or another song's file: refused, nothing changes.
    assert!(f.session.locate(first, renamed.join("notes.txt")).is_err());
    assert_eq!(f.session.track(first).unwrap().location, album.join("01.flac"));

    f.session.locate(first, renamed.join("01.flac")).unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    f.session.tick();
    let notices = f.session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ");
    assert!(notices.contains("“First Light” is linked to its file again."), "{notices}");
    assert!(notices.contains("Found 1 other missing song in the same place"), "{notices}");

    let track = f.session.track(first).unwrap();
    assert_eq!(track.location, renamed.join("01.flac"));
    assert_eq!(track.stats.rating, Rating::from_stars(5), "everything TuneBox knew is kept");
    assert_eq!(*f.session.file_status(first), FileStatus::Present);
    assert_eq!(f.session.track(second).unwrap().location, renamed.join("02.flac"), "moved the same way");
    // Its namesake is a different file (another size): left missing, not mislinked.
    assert_eq!(f.session.track(third).unwrap().location, album.join("03.flac"));
    assert_eq!(*f.session.file_status(third), FileStatus::Missing);
    // And a file that already belongs to a song cannot be given to another.
    let error = f.session.locate(third, renamed.join("02.flac")).unwrap_err().to_string();
    assert!(error.contains("already belongs to “Second Light”"), "{error}");
}

/// A small PNG cover in `dir`, and its bytes.
fn cover_file(dir: &Path, name: &str, colour: [u8; 3]) -> (std::path::PathBuf, Vec<u8>) {
    let image = image::RgbImage::from_pixel(64, 64, image::Rgb(colour));
    let path = dir.join(name);
    std::fs::create_dir_all(dir).unwrap();
    image.save(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    (path, bytes)
}

fn notices_text(session: &mut Session) -> String {
    session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ")
}

#[test]
fn a_cover_goes_into_every_writable_song_and_is_kept_for_read_only_ones() {
    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    // A third song of the same album in a read-only collection.
    let collection = f.dir.path().join("Collection");
    write_song(&collection.join("c.flac"), "Third Light", "Quiet Harbour", "Lanterns", 3);
    let mut prefs = f.session.preferences().clone();
    prefs.add_library_folder(collection.clone(), true);
    f.session.set_preferences(prefs);
    f.session.import(vec![collection.clone()]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 3);
    let protected = f.session.library().track_at(&collection.join("c.flac")).unwrap();
    let all = f.session.query(&TrackQuery::new(Source::Music));
    let writable: Vec<_> = all.iter().copied().filter(|id| *id != protected).collect();
    let before = snapshot(&collection);
    f.session.take_notices();

    let (picture, png) = cover_file(&f.dir.path().join("Pictures"), "cover.png", [200, 40, 90]);
    f.session.set_artwork(&all, &picture).unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    let notice = notices_text(&mut f.session);
    assert!(notice.contains("Set the cover of 2 songs."), "{notice}");
    assert!(notice.contains("1 song is in a read-only library folder: TuneBox keeps its cover itself"), "{notice}");
    for id in &writable {
        let track = f.session.track(*id).unwrap();
        let cover = tunebox_tags::read_artwork(&track.location).unwrap().unwrap();
        assert_eq!((cover.mime.as_str(), &cover.data), ("image/png", &png), "the picture as it was chosen");
        assert_eq!(track.file.size_bytes, std::fs::metadata(&track.location).unwrap().len(), "the new size is shown");
    }
    assert_eq!(snapshot(&collection), before, "the read-only folder is byte-for-byte unchanged");
    let kept = f.session.cover_source(protected).unwrap().load().unwrap().unwrap();
    assert_eq!(kept.data, png, "the read-only song shows the cover TuneBox keeps for it");
    assert_eq!(f.session.take_artwork_changes(), [("Quiet Harbour".to_owned(), "Lanterns".to_owned())]);

    // The command line can save it, from a snapshot that takes no lock.
    f.session.save().unwrap();
    let out = f.dir.path().join("Exported.png");
    let view_snapshot = LibrarySnapshot::open(&paths_in(&f.dir)).unwrap();
    let saved = view_snapshot.view().export_cover(protected, &out).unwrap().unwrap();
    assert_eq!((saved.extension(), std::fs::read(&out).unwrap()), ("png", png));
    assert!(
        view_snapshot.view().export_cover(protected, &collection.join("x.png")).is_err(),
        "not into a read-only folder"
    );

    // Removing takes it out of the files and forgets the kept one.
    f.session.remove_artwork(&all).unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    let notice = notices_text(&mut f.session);
    assert!(notice.contains("Removed the cover of 2 songs."), "{notice}");
    assert!(notice.contains("A cover inside its file stays"), "{notice}");
    for id in &writable {
        assert_eq!(tunebox_tags::read_artwork(&f.session.track(*id).unwrap().location).unwrap(), None);
    }
    assert_eq!(f.session.cover_source(protected).unwrap().load().unwrap(), None);
    assert_eq!(snapshot(&collection), before);
}

#[test]
fn one_song_that_cannot_take_a_cover_does_not_stop_the_others() {
    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    let downloads = f.dir.path().join("Downloads");
    write_song(&downloads.join("gone.flac"), "Gone", "Quiet Harbour", "Lanterns", 3);
    f.session.import(vec![downloads.join("gone.flac")]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 3);
    let all = f.session.query(&TrackQuery::new(Source::Music));
    // One song's file is damaged, another's has gone.
    std::fs::write(downloads.join("b.flac"), b"no longer a song").unwrap();
    std::fs::remove_file(downloads.join("gone.flac")).unwrap();
    f.session.check_files_now();
    f.session.take_notices();

    let (picture, png) = cover_file(&f.dir.path().join("Pictures"), "cover.png", [10, 120, 60]);
    f.session.set_artwork(&all, &picture).unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    let notice = notices_text(&mut f.session);
    assert!(notice.contains("Set the cover of 1 song."), "{notice}");
    assert!(notice.contains("1 song was left as it is: its file is missing."), "{notice}");
    assert!(notice.contains("The cover of “b.flac” could not be changed, so it was left as it is"), "{notice}");
    assert_eq!(tunebox_tags::read_artwork(&downloads.join("a.flac")).unwrap().unwrap().data, png);
    assert_eq!(std::fs::read(downloads.join("b.flac")).unwrap(), b"no longer a song");
    assert!(!downloads.join("gone.flac").exists(), "a missing song's file is not created");
}

#[test]
fn a_file_that_is_not_a_picture_is_refused_before_anything_changes() {
    let mut f = fixture(1.0);
    import_two_songs(&mut f);
    let downloads = f.dir.path().join("Downloads");
    let before = snapshot(&downloads);
    let all = f.session.query(&TrackQuery::new(Source::Music));
    let notes = f.dir.path().join("notes.txt");
    std::fs::write(&notes, "not a picture").unwrap();
    let error = f.session.set_artwork(&all, &notes).unwrap_err().to_string();
    assert!(error.contains("not a picture"), "{error}");
    assert!(f.session.set_artwork(&all, &f.dir.path().join("nowhere.png")).is_err());
    // A song is not a picture either, even though it is a file TuneBox knows well.
    assert!(f.session.set_artwork(&all, &downloads.join("a.flac")).is_err());
    assert!(f.session.jobs().is_empty());
    assert_eq!(snapshot(&downloads), before);
}

// ----- Rename Files from Tags -----

/// A writable library folder `Music` holding the album "Lanterns" in an untidy folder, with a
/// lyrics file beside the first song and a cover in the folder.
fn untidy_album(f: &mut Fixture) -> (std::path::PathBuf, Vec<tunebox_engine::model::TrackId>) {
    let music = f.dir.path().join("Music");
    let untidy = music.join("downloads/lanterns rip");
    write_song(&untidy.join("track1.flac"), "First Light", "Quiet Harbour", "Lanterns", 1);
    write_song(&untidy.join("track2.flac"), "Second Light", "Quiet Harbour", "Lanterns", 2);
    std::fs::write(untidy.join("track1.lrc"), "[00:01.00]First words").unwrap();
    let mut prefs = f.session.preferences().clone();
    prefs.add_library_folder(music.clone(), false);
    f.session.set_preferences(prefs);
    f.session.import(vec![music.clone()]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 2 && s.jobs().is_empty());
    let ids = ["track1.flac", "track2.flac"].map(|n| f.session.library().track_at(&untidy.join(n)).unwrap());
    f.session.take_notices();
    (music, ids.to_vec())
}

fn request(tracks: &[tunebox_engine::model::TrackId], pattern: &str) -> RenameRequest {
    RenameRequest { tracks: tracks.to_vec(), pattern: pattern.into(), base: RenameBase::OwnFolder }
}

/// Renames as the preview says and waits; returns the notices.
fn rename(session: &mut Session, request: &RenameRequest) -> String {
    let preview = session.preview_rename(request).unwrap();
    session.rename_files(&preview, RenameConflicts::Skip).unwrap();
    tick_until(session, |s| s.jobs().is_empty());
    session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ")
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn renaming_files_an_album_by_its_tags_and_keeps_everything_the_library_knows() {
    let mut f = fixture(1.0);
    let (music, ids) = untidy_album(&mut f);
    f.session.set_rating(&[ids[0]], Rating::from_stars(5));
    let playlist = f.session.create_playlist("Evening", &ids).unwrap();
    let request = request(&ids, "{album-artist|artist}/{album}/{track:02} {title}");

    let preview = f.session.preview_rename(&request).unwrap();
    let album = music.join("Quiet Harbour/Lanterns");
    assert_eq!(
        preview.renames().map(|(_, _, to)| to.to_path_buf()).collect::<Vec<_>>(),
        [album.join("01 First Light.flac"), album.join("02 Second Light.flac")]
    );
    assert_eq!(preview.summary(), "2 files to rename.");
    assert!(music.join("downloads/lanterns rip/track1.flac").exists(), "a preview changes nothing");

    let notice = rename(&mut f.session, &request);
    assert_eq!(notice, "Renamed 2 files. 1 lyrics or cue file went with its song. Removed 2 folders left empty.");
    assert_eq!(names_in(&album), ["01 First Light.flac", "01 First Light.lrc", "02 Second Light.flac"]);
    assert_eq!(names_in(&music), ["Quiet Harbour"], "the emptied folders are gone, the library folder stays");
    let first = f.session.track(ids[0]).unwrap();
    assert_eq!(first.location, album.join("01 First Light.flac"));
    assert_eq!(first.stats.rating, Rating::from_stars(5));
    assert_eq!(f.session.library().playlist_tracks(playlist, chrono::Utc::now()).unwrap(), ids);
    assert_eq!(f.session.library().len(), 2);

    // Again: nothing to do.
    let notice = rename(&mut f.session, &request);
    assert_eq!(notice, "No files were renamed. 2 were already named so.");

    // A folder that still holds something stays.
    std::fs::write(album.join("cover.jpg"), b"jpeg").unwrap();
    rename(&mut f.session, &self::request(&ids, "{artist} - {title}"));
    assert_eq!(names_in(&album), ["cover.jpg"]);
    assert_eq!(
        names_in(&music),
        [
            "Quiet Harbour",
            "Quiet Harbour - First Light.flac",
            "Quiet Harbour - First Light.lrc",
            "Quiet Harbour - Second Light.flac"
        ]
    );

    // The library file says the same after a save.
    f.session.save().unwrap();
    let snapshot = LibrarySnapshot::open(&paths_in(&f.dir)).unwrap();
    assert_eq!(snapshot.view().track(ids[1]).unwrap().location, music.join("Quiet Harbour - Second Light.flac"));
}

#[test]
fn renaming_never_touches_a_read_only_folder() {
    let mut f = fixture(1.0);
    let (music, ids) = untidy_album(&mut f);
    let mut prefs = f.session.preferences().clone();
    prefs.add_library_folder(music.clone(), true);
    f.session.set_preferences(prefs);
    let before = snapshot(&music);

    let request = request(&ids, "{artist}/{title}");
    let preview = f.session.preview_rename(&request).unwrap();
    assert!(preview.items.iter().all(|i| i.planned == Planned::Skipped(SkipReason::ReadOnly)));
    let notice = rename(&mut f.session, &request);
    assert!(notice.starts_with("No files were renamed. 2 songs were left as they are."), "{notice}");

    // Nor may songs be moved into one.
    let mut media_request = request.clone();
    media_request.base = RenameBase::MediaFolder;
    let mut prefs = f.session.preferences().clone();
    prefs.media_folder = music.join("Media");
    f.session.set_preferences(prefs);
    rename(&mut f.session, &media_request);
    assert_eq!(snapshot(&music), before, "byte for byte unchanged");
    assert_eq!(f.session.track(ids[0]).unwrap().location, music.join("downloads/lanterns rip/track1.flac"));
}

#[test]
fn a_rename_that_only_changes_case() {
    let mut f = fixture(1.0);
    let (music, ids) = untidy_album(&mut f);
    rename(&mut f.session, &request(&ids, "{title|track}"));
    assert_eq!(names_in(&music), ["First Light.flac", "First Light.lrc", "Second Light.flac"]);
    // Retitled in lower case, then renamed by title again.
    let mut meta = f.session.track(ids[0]).unwrap().meta.clone();
    meta.title = "first light".into();
    f.session.edit_track(ids[0], meta).unwrap();
    let notice = rename(&mut f.session, &request(&ids[..1], "{title}"));
    assert_eq!(notice, "Renamed 1 file. 1 lyrics or cue file went with its song.");
    assert_eq!(names_in(&music), ["Second Light.flac", "first light.flac", "first light.lrc"]);
    assert_eq!(f.session.track(ids[0]).unwrap().location, music.join("first light.flac"));
}

#[test]
fn one_failed_rename_does_not_stop_the_others_and_the_library_matches_the_disk() {
    let mut f = fixture(1.0);
    let (music, ids) = untidy_album(&mut f);
    let request = request(&ids, "{track:02} {title}");
    let preview = f.session.preview_rename(&request).unwrap();
    // The first song's file goes away after the preview, before the library notices.
    let gone = music.join("downloads/lanterns rip/track1.flac");
    std::fs::remove_file(&gone).unwrap();
    f.session.rename_files(&preview, RenameConflicts::Refuse).unwrap();
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    let notice = f.session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ");
    assert!(notice.contains("Renamed 1 file."), "{notice}");
    assert!(notice.contains("“track1.flac” could not be renamed, so it is where it was"), "{notice}");

    let folder = music.join("downloads/lanterns rip");
    assert_eq!(names_in(&folder), ["track1.lrc"], "its lyrics stay where it was, and so does the folder");
    assert_eq!(names_in(&music), ["02 Second Light.flac", "downloads"]);
    assert_eq!(f.session.track(ids[0]).unwrap().location, gone);
    assert_eq!(f.session.track(ids[1]).unwrap().location, music.join("02 Second Light.flac"));
    for id in &ids {
        let location = &f.session.track(*id).unwrap().location;
        assert_eq!(location.exists(), *id == ids[1], "{}", location.display());
    }
}

#[test]
fn a_stale_preview_or_a_conflict_renames_nothing() {
    let mut f = fixture(1.0);
    let (music, ids) = untidy_album(&mut f);
    let before = snapshot(&music);

    // Two songs, one name.
    let clash = request(&ids, "{album} {track-total|title}");
    let preview = f.session.preview_rename(&clash).unwrap();
    assert_eq!(preview.conflicts(), 2);
    let error = f.session.rename_files(&preview, RenameConflicts::Refuse).unwrap_err().to_string();
    assert!(error.contains("2 songs would get names already taken"), "{error}");
    rename(&mut f.session, &clash);
    assert_eq!(snapshot(&music), before, "skipping the conflicts leaves nothing to do");

    // A file appears where the preview said a song would go.
    let request = request(&ids, "{title}");
    let preview = f.session.preview_rename(&request).unwrap();
    std::fs::write(music.join("First Light.flac"), b"someone else's").unwrap();
    let error = f.session.rename_files(&preview, RenameConflicts::Skip).unwrap_err().to_string();
    assert!(error.contains("changed since the preview"), "{error}");
    assert_eq!(std::fs::read(music.join("First Light.flac")).unwrap(), b"someone else's");

    // A bad pattern says what is wrong.
    let error = f.session.preview_rename(&self::request(&ids, "{artist}/{album}")).unwrap_err().to_string();
    assert!(error.contains("{title} or {track}"), "{error}");
}

#[test]
fn renamed_files_are_not_taken_for_new_songs_by_the_folder_watcher() {
    let mut f = fixture(1.0);
    let (music, ids) = untidy_album(&mut f);
    f.session.set_rating(&[ids[1]], Rating::from_stars(4));
    let request = request(&ids, "{artist}/{album}/{track:02} {title}");
    let preview = f.session.preview_rename(&request).unwrap();
    f.session.rename_files(&preview, RenameConflicts::Refuse).unwrap();
    // The files have moved (the job is done with the disk) but the session has not heard back
    // yet: the moment a watcher reports the new files as new and the old ones as gone.
    let album = music.join("Quiet Harbour/Lanterns");
    let old = music.join("downloads/lanterns rip");
    let start = Instant::now();
    while old.exists() || music.join("downloads").exists() {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
    let changes = vec![
        music.join("Quiet Harbour"),
        album.clone(),
        album.join("01 First Light.flac"),
        album.join("02 Second Light.flac"),
        album.join("01 First Light.lrc"),
        old.join("track1.flac"),
        old.join("track2.flac"),
        old.clone(),
    ];
    f.session.folders_changed(changes.clone());
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    // And the same again, as a watcher that settles late would.
    f.session.folders_changed(changes);
    tick_until(&mut f.session, |s| s.jobs().is_empty());
    f.session.tick();
    tick_until(&mut f.session, |s| s.jobs().is_empty());

    assert_eq!(f.session.library().len(), 2, "no duplicates");
    assert_eq!(f.session.track(ids[1]).unwrap().location, album.join("02 Second Light.flac"));
    assert_eq!(f.session.track(ids[1]).unwrap().stats.rating, Rating::from_stars(4));
    let notices = f.session.take_notices();
    let text = notices.iter().map(|n| n.text.as_str()).collect::<Vec<_>>().join(" ");
    assert!(notices.iter().all(|n| n.level == tunebox_engine::NoticeLevel::Info), "{text}");
    assert_eq!(text, "Renamed 2 files. 1 lyrics or cue file went with its song. Removed 2 folders left empty.");
    for id in &ids {
        assert_eq!(*f.session.file_status(*id), FileStatus::Present);
    }
}

#[test]
fn quitting_during_a_rename_saves_where_the_files_went() {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("Music");
    for n in 1..=40 {
        write_song(&music.join(format!("in/{n}.flac")), &format!("Song {n}"), "Quiet Harbour", "Lanterns", n);
    }
    let mut session = Session::open(paths_in(&dir), Player::with_output(NullOutput { speed: 1.0 })).unwrap();
    let mut prefs = session.preferences().clone();
    prefs.add_library_folder(music.clone(), false);
    session.set_preferences(prefs);
    session.import(vec![music.clone()]).unwrap();
    tick_until(&mut session, |s| s.library().len() == 40 && s.jobs().is_empty());
    let ids = session.query(&TrackQuery::new(Source::Music));
    let preview = session.preview_rename(&request(&ids, "out/{title}")).unwrap();
    session.rename_files(&preview, RenameConflicts::Refuse).unwrap();
    // Quit as soon as the first file has moved, without the session hearing of it.
    let start = Instant::now();
    while std::fs::read_dir(music.join("out")).map_or(true, |mut d| d.next().is_none()) {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    session.shutdown().unwrap();
    drop(session);

    let reopened = LibrarySnapshot::open(&paths_in(&dir)).unwrap();
    let view = reopened.view();
    assert_eq!(view.library().len(), 40);
    for id in ids {
        let location = &view.track(id).unwrap().location;
        assert!(location.exists(), "the library lost track of {}", location.display());
    }
}
