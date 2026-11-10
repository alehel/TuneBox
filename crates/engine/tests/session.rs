//! The session end to end, headless: import, browse, play, rip a CD, sync a player.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tunebox_audio::{LosslessFormat, PcmWriter};
use tunebox_engine::model::{Column, NumberOf, Rating, SortOrder, TagField, TrackMeta};
use tunebox_engine::{
    NullOutput, Player, Session, SessionPaths, Source, SyncMode, SyncSettings, TrackQuery, VirtualDrive, Volume,
};

struct Fixture {
    dir: tempfile::TempDir,
    session: Session,
}

fn fixture(speed: f64) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let paths = SessionPaths {
        library_file: dir.path().join("TuneBox/Library.json"),
        preferences_file: dir.path().join("TuneBox/Preferences.json"),
        default_media_folder: dir.path().join("TuneBox/Media"),
    };
    let session = Session::open(paths, Player::with_output(NullOutput { speed })).unwrap();
    Fixture { dir, session }
}

/// A short tagged FLAC.
fn write_song(path: &Path, title: &str, artist: &str, album: &str, track: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut writer = PcmWriter::create(path, LosslessFormat::Flac, 8_000, 1).unwrap();
    writer.write(&[vec![0.1; 4_000]]).unwrap();
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
    f.session.shutdown().unwrap();
    let reopened = fixture_at(&f.dir);
    assert_eq!(reopened.library().len(), 2);
    assert!(reopened.library().playlist(playlist).is_some());
}

fn fixture_at(dir: &tempfile::TempDir) -> Session {
    let paths = SessionPaths {
        library_file: dir.path().join("TuneBox/Library.json"),
        preferences_file: dir.path().join("TuneBox/Preferences.json"),
        default_media_folder: dir.path().join("TuneBox/Media"),
    };
    Session::open(paths, Player::with_output(NullOutput::default())).unwrap()
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
    };
    f.session.refresh_devices(&[volume]);
    assert_eq!(f.session.devices().len(), 1);
    f.session.register_device(&player_root, "Pocket").unwrap();

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
