//! The demo's audiobooks: three finished books in the library, and one in progress, ripped
//! from two spoken virtual discs through the engine, edited, and waiting for disc 3. The app's
//! audiobook page and its screenshots are shown from it.

use crate::covers::{Style, cover};
use chrono::{DateTime, TimeDelta, Utc};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tunebox_audio::{Mp4AudioWriter, Mp4Chapter, Mp4Codec, Mp4Settings};
use tunebox_engine::model::audiobook::{BookDetails, Grouping, ReleaseMedium, TrackRef, Trim, samples};
use tunebox_engine::model::{MediaKind, TrackMeta};
use tunebox_engine::{JobKind, NullOutput, Player, Session, VirtualChanger, VirtualDrive};
use tunebox_library::{Library, NewTrack};

/// The book in progress: its discs' tracks, as MusicBrainz would name them, and their lengths.
const DISCS: [&[(&str, u32)]; 3] = [
    &[
        ("Opening Credits", 38),
        ("Chapter 1 (1)", 252),
        ("Chapter 1 (2)", 237),
        ("Chapter 1 (3)", 281),
        ("Chapter 2 (1)", 245),
        ("Chapter 2 (2)", 228),
        ("Chapter 2 (3)", 260),
        ("Chapter 3 (1)", 273),
        ("Chapter 3 (2)", 231),
        ("Chapter 3 (3)", 242),
        ("Chapter 4 (1)", 255),
        ("Chapter 4 (2)", 224),
        ("Chapter 4 (3)", 179),
        ("End of Disc One", 9),
    ],
    &[
        ("Disc Two", 6),
        ("Chapter 4 (4)", 216),
        ("Chapter 4 (5)", 250),
        ("Chapter 5 (1)", 262),
        ("Chapter 5 (2)", 238),
        ("Chapter 5 (3)", 287),
        ("Chapter 6 (1)", 243),
        ("Chapter 6 (2)", 251),
        ("Chapter 6 (3)", 229),
        ("Chapter 7 (1)", 248),
        ("Chapter 7 (2)", 233),
        ("Chapter 7 (3)", 262),
    ],
    &[("Disc Three", 6), ("Chapter 7 (4)", 231), ("Chapter 8 (1)", 244), ("Chapter 8 (2)", 252)],
];

/// The finished books: title, author, narrator, series, length in seconds.
const BOOKS: [(&str, &str, &str, &str, u32); 3] = [
    ("Salt and Tallow", "Margaret Ellison", "David Ashcombe", "The Harbour Trilogy, 1", 33_164),
    ("Notes from a Small Observatory", "Priya Ramaswamy", "Priya Ramaswamy", "", 24_510),
    ("The Clockmaker's Year", "Tobias Wren", "Helen Marsh", "", 39_797),
];

fn disc(number: usize) -> VirtualDrive {
    let lengths = DISCS.get(number).map(|d| d.iter().map(|(_, s)| *s).collect()).unwrap_or_default();
    VirtualDrive::new(format!("Audiobook Disc {}", number + 1), lengths).spoken()
}

fn disc_id(drive: &VirtualDrive) -> Result<String, String> {
    let toc = tunebox_rip::CdDrive::read_toc(drive).map_err(|e| e.to_string())?;
    Ok(tunebox_rip::disc_id(&toc))
}

/// Adds the finished books: short real files, listed at their books' lengths.
pub fn add_finished_books(library: &mut Library, media: &Path, now: DateTime<Utc>) -> Result<(), String> {
    for (index, (title, author, narrator, series, seconds)) in BOOKS.iter().enumerate() {
        let path = media.join(author).join(format!("{title}.m4b"));
        std::fs::create_dir_all(media.join(author)).map_err(|e| e.to_string())?;
        let frames = 44_100 * 6;
        let chapters = (0..3).map(|i| Mp4Chapter { start: i * 88_200, title: format!("Chapter {}", i + 1) }).collect();
        let settings = Mp4Settings {
            codec: Mp4Codec::Aac { bitrate_kbps: 64 },
            sample_rate: 44_100,
            channels: 1,
            frames,
            chapters,
        };
        let mut writer = Mp4AudioWriter::create(&path, &settings).map_err(|e| e.to_string())?;
        let tone: Vec<f32> = (0..frames).map(|i| ((i as f32) * 0.02).sin() * 0.05).collect();
        writer.write(&[tone]).map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
        let meta = TrackMeta {
            title: (*title).into(),
            artist: (*author).into(),
            album_artist: (*author).into(),
            album: (*title).into(),
            composer: (*narrator).into(),
            grouping: (*series).into(),
            genre: "Audiobook".into(),
            year: Some(2015 + index as u32 * 3),
            ..TrackMeta::default()
        };
        tunebox_tags::write(&path, &meta).map_err(|e| e.to_string())?;
        tunebox_tags::write_media_kind(&path, MediaKind::Audiobook).map_err(|e| e.to_string())?;
        let art =
            cover([Style::Waves, Style::Rings, Style::Shards][index], [[30, 50, 80], [210, 160, 90], [250, 230, 200]])?;
        let artwork = tunebox_tags::Artwork { mime: "image/jpeg".into(), data: art };
        tunebox_tags::write_artwork(&path, &artwork).map_err(|e| e.to_string())?;
        let mut file = tunebox_tags::read(&path).map_err(|e| e.to_string())?.file;
        // A demo: the file is a few seconds, the book it stands for many hours.
        file.duration = Duration::from_secs(u64::from(*seconds));
        let date_added = now - TimeDelta::days(90 - index as i64 * 30);
        library.add_track(NewTrack { location: path, meta, file, date_added, kind: MediaKind::Audiobook });
    }
    Ok(())
}

/// Makes the book in progress in the library at `library_dir`, as a listener would: two discs
/// ripped and edited, the book waiting for disc 3.
pub fn build_book_in_progress(library_dir: &Path) -> Result<(), String> {
    eprintln!("demo: the audiobook in progress (ripping two virtual discs)");
    let paths = tunebox_platform::app_paths(library_dir);
    let mut session = Session::open(paths, Player::with_output(NullOutput::default())).map_err(|e| e.to_string())?;
    let changer = Arc::new(VirtualChanger::new("Virtual Drive"));
    let drives: Vec<VirtualDrive> = (0..DISCS.len()).map(disc).collect();
    let mut media = Vec::new();
    for (index, drive) in drives.iter().enumerate() {
        media.push(ReleaseMedium {
            position: index as u32 + 1,
            disc_ids: vec![disc_id(drive)?],
            track_titles: DISCS[index].iter().map(|(title, _)| (*title).to_owned()).collect(),
        });
    }

    insert(&mut session, &changer, drives[0].clone())?;
    let mut info = session.cd().map(|cd| cd.info.clone()).ok_or("the first disc was not read")?;
    info.album = "The Long Road Home".into();
    info.artist = "Margaret Ellison".into();
    info.release_media = media;
    session.set_disc_info(info);
    let details = BookDetails {
        title: "The Long Road Home".into(),
        author: "Margaret Ellison".into(),
        narrator: "David Ashcombe".into(),
        series: "The Harbour Trilogy".into(),
        series_number: Some(2),
        year: Some(2019),
        publisher: "Harbourside Audio".into(),
        description: "Twenty years after leaving Saltmarsh, Ruth Cawley comes home for her father's funeral.".into(),
        ..BookDetails::default()
    };
    session.start_audiobook(details).map_err(|e| e.to_string())?;
    let art = cover(Style::Sunset, [[43, 58, 103], [196, 107, 90], [242, 182, 109]])?;
    session
        .set_audiobook_cover(Some(tunebox_tags::Artwork { mime: "image/jpeg".into(), data: art }))
        .map_err(|e| e.to_string())?;
    rip(&mut session)?;
    insert(&mut session, &changer, drives[1].clone())?;
    rip(&mut session)?;

    let error = |e: tunebox_engine::EngineError| e.to_string();
    session.regroup_audiobook(Grouping::FromNames).map_err(error)?;
    for (track, name) in
        [((1, 2), "Saltmarsh"), ((1, 5), "The Letter"), ((1, 8), "Low Water"), ((1, 11), "The Funeral")]
    {
        session.rename_audiobook_chapter(TrackRef::new(track.0, track.1), name).map_err(error)?;
    }
    session.set_audiobook_tracks_removed(&[TrackRef::new(2, 1)], true).map_err(error)?;
    session.trim_audiobook_track(TrackRef::new(1, 13), Trim { start: 0, end: samples(6.5) }).map_err(error)?;
    session.shutdown().map_err(|e| e.to_string())?;
    Ok(())
}

fn insert(session: &mut Session, changer: &Arc<VirtualChanger>, drive: VirtualDrive) -> Result<(), String> {
    changer.insert(drive);
    session.read_disc(changer.clone());
    wait(session, JobKind::ReadDisc)
}

fn rip(session: &mut Session) -> Result<(), String> {
    session.rip_audiobook_disc().map_err(|e| e.to_string())?;
    wait(session, JobKind::RipBookDisc)
}

/// Ticks the session until a job of `kind` is done (or a generous time has passed).
fn wait(session: &mut Session, kind: JobKind) -> Result<(), String> {
    let start = Instant::now();
    loop {
        session.tick();
        for notice in session.take_notices() {
            eprintln!("demo: {}", notice.text);
        }
        if !session.is_busy(kind) {
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(900) {
            return Err(format!("{} took too long", kind.noun()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
