//! The session end to end, headless: import, browse, play, rip a CD, sync a player.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tunebox_audio::{LosslessFormat, PcmWriter};
use tunebox_engine::model::{Column, NumberOf, Rating, SortOrder, TagField, TrackMeta};
use tunebox_engine::{
    LibraryOnlyEdits, LibrarySnapshot, NullOutput, Player, Session, SessionPaths, Source, SyncMode, SyncSettings,
    TrackQuery, VirtualDrive, Volume,
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
