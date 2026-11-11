//! Audiobooks end to end, headless: a book ripped from several discs in a virtual changer,
//! edited, built into the library, and kept across a restart.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tunebox_engine::model::audiobook::{
    BookDetails, DiscCheck, Grouping, OutputFormat, OutputLayout, OutputSettings, ReleaseMedium, TrackRef,
};
use tunebox_engine::model::{MediaKind, NumberOf};
use tunebox_engine::{
    DiscQuestion, EngineError, JobKind, NullOutput, Player, Session, SessionPaths, Source, VirtualChanger, VirtualDrive,
};

fn paths_in(dir: &Path) -> SessionPaths {
    SessionPaths {
        library_file: dir.join("TuneBox/Library.json"),
        preferences_file: dir.join("TuneBox/Preferences.json"),
        default_media_folder: dir.join("TuneBox/Media"),
        visualizer_plugins_folder: dir.join("TuneBox/Visualizers"),
    }
}

fn open(dir: &Path) -> Session {
    Session::open(paths_in(dir), Player::with_output(NullOutput { speed: 1.0 })).unwrap()
}

fn tick_until(session: &mut Session, mut done: impl FnMut(&Session) -> bool) {
    let start = Instant::now();
    while !done(session) {
        assert!(start.elapsed() < Duration::from_secs(60), "timed out; notices: {:?}", session.take_notices());
        session.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Puts `disc` in the changer and lets the session read it.
fn insert(session: &mut Session, changer: &Arc<VirtualChanger>, disc: VirtualDrive) {
    changer.insert(disc);
    session.read_disc(changer.clone());
    tick_until(session, |s| !s.is_busy(JobKind::ReadDisc));
}

fn rip(session: &mut Session) {
    session.rip_audiobook_disc().unwrap();
    tick_until(session, |s| !s.is_busy(JobKind::RipBookDisc));
}

fn details() -> BookDetails {
    BookDetails {
        title: "The Long Road Home".into(),
        author: "Margaret Ellison".into(),
        narrator: "David Ashcombe".into(),
        ..BookDetails::default()
    }
}

fn t(disc: u16, track: u8) -> TrackRef {
    TrackRef::new(disc, track)
}

#[test]
fn a_book_is_ripped_disc_by_disc_edited_and_built_into_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = open(dir.path());
    let changer = Arc::new(VirtualChanger::new("Changer"));
    rip_two_discs(&mut session, &changer);
    edit_across_the_discs(&mut session);
    session.finish_audiobook_ripping(false).unwrap();
    assert_eq!(session.audiobook_waiting_for(), None);
    session.build_audiobook().unwrap();
    tick_until(&mut session, |s| !s.is_busy(JobKind::BuildBook));
    let notices: Vec<String> = session.take_notices().into_iter().map(|n| n.text).collect();
    assert!(notices.iter().any(|n| n.contains("under Audiobooks")), "{notices:?}");
    check_the_built_book(&mut session, dir.path());
}

/// Disc 1 (its last track an announcement, removed), then disc 2, then disc 1 again by mistake.
fn rip_two_discs(session: &mut Session, changer: &Arc<VirtualChanger>) {
    insert(session, changer, VirtualDrive::new("Disc 1", vec![3, 4, 1]));
    session.start_audiobook(details()).unwrap();
    let book = session.audiobook().unwrap();
    assert_eq!(book.discs.len(), 1);
    assert_eq!(book.chapters().len(), 1, "a disc starts as one chapter");
    assert_eq!(session.audiobook_waiting_for(), None, "disc 1 waits for Rip Disc");

    // Editing works before and while a disc rips.
    session.toggle_audiobook_tracks(&[t(1, 3)]).unwrap();
    session.rip_audiobook_disc().unwrap();
    session.split_audiobook_chapter(&[t(1, 2)]).unwrap();
    tick_until(session, |s| !s.is_busy(JobKind::RipBookDisc));
    let book = session.audiobook().unwrap();
    assert!(book.discs[0].tracks.iter().all(|t| t.staged), "a removed track is ripped too");
    assert_eq!(book.chapters().len(), 2);
    assert_eq!(changer.ejections(), 1, "the disc comes out after ripping");
    assert!(session.cd().is_none());
    assert_eq!(session.audiobook_waiting_for(), Some(2), "Insert disc 2");

    // Disc 2 joins as one more chapter, waits for Rip Disc, and is ripped.
    insert(session, changer, VirtualDrive::new("Disc 2", vec![2, 5]));
    assert_eq!(session.audiobook().unwrap().discs.len(), 2);
    assert_eq!(session.audiobook_waiting_for(), None);
    rip(session);
    assert_eq!(session.audiobook_waiting_for(), Some(3));

    // Disc 1 again by mistake: it is ejected with a word about the disc wanted.
    insert(session, changer, VirtualDrive::new("Disc 1", vec![3, 4, 1]));
    assert_eq!(changer.ejections(), 3);
    let notices: Vec<String> = session.take_notices().into_iter().map(|n| n.text).collect();
    assert!(notices.iter().any(|n| n.contains("already ripped") && n.contains("Insert disc 3")), "{notices:?}");
    assert_eq!(session.audiobook().unwrap().discs.len(), 2);
}

/// The chapter runs on from disc 1 into disc 2; undo and redo walk the edits.
fn edit_across_the_discs(session: &mut Session) {
    session.join_audiobook_chapters(&[t(1, 2), t(2, 1)]).unwrap();
    session.rename_audiobook_chapter(t(1, 2), "The Funeral").unwrap();
    assert_eq!(session.audiobook().unwrap().chapters()[1].name, "The Funeral");
    assert!(session.undo_audiobook().unwrap());
    assert_eq!(session.audiobook().unwrap().chapters()[1].name, "Chapter 2");
    assert!(session.redo_audiobook().unwrap());
    assert_eq!(session.audiobook().unwrap().chapters()[1].discs, (1, 2));
}

/// One .m4b, an audiobook, with its two chapters; the staged rip is gone.
fn check_the_built_book(session: &mut Session, dir: &Path) {
    assert!(session.audiobook().is_none());
    let books = session.source_tracks(&Source::Audiobooks);
    assert_eq!(books.len(), 1);
    assert!(session.source_tracks(&Source::Music).is_empty(), "books stay out of Music");
    let track = session.track(books[0]).unwrap();
    assert_eq!(track.kind, MediaKind::Audiobook);
    assert_eq!(track.meta.composer, "David Ashcombe");
    assert_eq!(track.meta.track, NumberOf::default());
    let chapters = tunebox_audio::read_mp4_chapters(&track.location).unwrap();
    let names: Vec<&str> = chapters.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, ["Chapter 1", "The Funeral"]);
    assert_eq!(chapters[1].0, Duration::from_secs(3));
    // 3 + 4 seconds of disc 1 (its last track removed) and 2 + 5 of disc 2.
    assert_eq!(track.file.duration.as_secs(), 14);
    let audiobooks = dir.join("TuneBox/Audiobooks");
    assert_eq!(std::fs::read_dir(&audiobooks).unwrap().count(), 0, "the staged rip is deleted");

    // A book made music by the listener leaves Audiobooks, in the library and in the file.
    session.set_media_kind(&books, MediaKind::Music);
    assert_eq!(session.source_tracks(&Source::Music), books);
    assert_eq!(tunebox_tags::read(&track_path(session, books[0])).unwrap().kind, MediaKind::Music);
}

fn track_path(session: &Session, id: tunebox_engine::model::TrackId) -> std::path::PathBuf {
    session.track(id).unwrap().location.clone()
}

#[test]
fn the_book_survives_quitting_between_discs() {
    let dir = tempfile::tempdir().unwrap();
    let changer = Arc::new(VirtualChanger::new("Changer"));
    {
        let mut session = open(dir.path());
        insert(&mut session, &changer, VirtualDrive::new("Disc 1", vec![2, 3]));
        session.start_audiobook(details()).unwrap();
        session.rename_audiobook_chapter(t(1, 1), "Saltmarsh").unwrap();
        rip(&mut session);
        session.shutdown().unwrap();
    }
    let mut session = open(dir.path());
    let book = session.audiobook().expect("the book is back");
    assert_eq!(book.chapters()[0].name, "Saltmarsh");
    assert!(book.discs[0].is_ripped());
    assert_eq!(session.audiobook_waiting_for(), Some(2), "and asks for the same disc");
    // Per-chapter FLAC, while at it.
    let output =
        OutputSettings { layout: OutputLayout::PerChapter, format: OutputFormat::Flac, ..OutputSettings::default() };
    session.set_audiobook_output(output).unwrap();
    session.finish_audiobook_ripping(false).unwrap();
    session.build_audiobook().unwrap();
    tick_until(&mut session, |s| !s.is_busy(JobKind::BuildBook));
    let books = session.source_tracks(&Source::Audiobooks);
    assert_eq!(books.len(), 1);
    let location = track_path(&session, books[0]);
    assert!(location.ends_with("Margaret Ellison/The Long Road Home/01 Saltmarsh.flac"), "{location:?}");
}

#[test]
fn musicbrainz_tells_which_disc_comes_next() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = open(dir.path());
    let changer = Arc::new(VirtualChanger::new("Changer"));
    let discs = [vec![2, 2], vec![3], vec![4, 1]].map(|lengths| VirtualDrive::new("Disc", lengths));
    let ids: Vec<String> =
        discs.iter().map(|d| tunebox_rip::disc_id(&tunebox_rip::CdDrive::read_toc(d).unwrap())).collect();
    insert(&mut session, &changer, discs[0].clone());
    // As if looked up: the release has three discs.
    let mut info = session.cd().unwrap().info.clone();
    info.release_media = (0..3)
        .map(|i| ReleaseMedium {
            position: i as u32 + 1,
            disc_ids: vec![ids[i].clone()],
            track_titles: vec![format!("Disc {} opening", i + 1)],
        })
        .collect();
    session.set_disc_info(info);
    session.start_audiobook(details()).unwrap();
    assert_eq!(session.audiobook().unwrap().discs[0].tracks[0].title, "Disc 1 opening");
    rip(&mut session);

    // Disc 3 before disc 2: asked about, and ejected on "no".
    insert(&mut session, &changer, discs[2].clone());
    assert_eq!(session.audiobook_question(), Some(&DiscQuestion::OtherDisc { says: 3, expected: 2 }));
    assert_eq!(session.audiobook().unwrap().check_disc(&ids[2]), DiscCheck::OtherDisc { says: 3, expected: 2 });
    session.answer_audiobook_question(false).unwrap();
    assert!(!changer.has_disc());
    assert_eq!(session.audiobook().unwrap().discs.len(), 1);

    // Disc 2 is taken without a question, with its own names.
    insert(&mut session, &changer, discs[1].clone());
    assert_eq!(session.audiobook_question(), None);
    assert_eq!(session.audiobook().unwrap().discs[1].tracks[0].title, "Disc 2 opening");
    rip(&mut session);

    // Two of three discs: finishing asks first.
    let refused = session.finish_audiobook_ripping(false).unwrap_err();
    assert!(matches!(refused, EngineError::MoreDiscsListed { listed: 3, ripped: 2 }), "{refused}");
    session.finish_audiobook_ripping(true).unwrap();
    assert!(session.audiobook().unwrap().ripping_done);
    session.regroup_audiobook(Grouping::PerTrack).unwrap();
    assert_eq!(session.audiobook().unwrap().chapters().len(), 3);
    session.discard_audiobook().unwrap();
    assert!(session.audiobook().is_none());
    assert_eq!(std::fs::read_dir(dir.path().join("TuneBox/Audiobooks")).unwrap().count(), 0);
}

#[test]
fn building_into_a_read_only_media_folder_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = open(dir.path());
    let changer = Arc::new(VirtualChanger::new("Changer"));
    insert(&mut session, &changer, VirtualDrive::new("Disc 1", vec![1]));
    session.start_audiobook(details()).unwrap();
    assert!(session.build_audiobook().is_err(), "not ripped yet");
    rip(&mut session);
    session.finish_audiobook_ripping(false).unwrap();
    let mut prefs = session.preferences().clone();
    prefs.library_folders.push(tunebox_engine::LibraryFolder { path: prefs.media_folder.clone(), read_only: true });
    session.set_preferences(prefs);
    assert!(matches!(session.build_audiobook(), Err(EngineError::Protected(_))));
}
