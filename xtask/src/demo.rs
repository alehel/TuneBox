//! `cargo xtask demo`: a demo library of made-up albums (quiet tones with original procedural
//! covers), playlists and play history, plus a virtual player already synced. The app opens it
//! with `--demo`, and the README screenshots are taken from it.

use crate::covers::{Style, cover};
use chrono::{TimeDelta, Utc};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::UNIX_EPOCH;
use tunebox_audio::{LosslessFormat, PcmWriter};
use tunebox_library::{Library, NewTrack};
use tunebox_model::smart::{Limit, LimitUnit, NumberField, NumberOp, Rule, SelectedBy, TextField, TextOp};
use tunebox_model::{NumberOf, Rating, SmartRules, TrackId, TrackMeta, organize};
use tunebox_sync::{Device, PlaylistFile, SyncItem, SyncMode};

struct Album {
    artist: &'static str,
    title: &'static str,
    genre: &'static str,
    year: u32,
    style: Style,
    palette: [[u8; 3]; 3],
    /// Days since it was added to the library.
    added_days_ago: i64,
    tracks: &'static [(&'static str, u32)],
}

const ALBUMS: [Album; 8] = [
    Album {
        artist: "The Harbour Lanterns",
        title: "Paper Boats",
        genre: "Indie Folk",
        year: 1997,
        style: Style::Sunset,
        palette: [[48, 30, 70], [242, 132, 70], [255, 200, 90]],
        added_days_ago: 400,
        tracks: &[
            ("Lanterns on the Water", 241),
            ("Driftwood", 198),
            ("Paper Boats", 224),
            ("Gulls", 176),
            ("Low Tide Letters", 263),
            ("The Lighthouse Is Out", 287),
        ],
    },
    Album {
        artist: "Aurora Fields",
        title: "Northern Lights",
        genre: "Ambient",
        year: 2009,
        style: Style::Aurora,
        palette: [[10, 30, 60], [60, 220, 160], [140, 90, 230]],
        added_days_ago: 380,
        tracks: &[
            ("Polar Night", 372),
            ("Magnetic North", 315),
            ("Solar Wind", 298),
            ("Ice Shelf", 401),
            ("Midnight Sun", 344),
        ],
    },
    Album {
        artist: "Neon Parade",
        title: "Midnight Arcade",
        genre: "Synthpop",
        year: 1986,
        style: Style::Grid,
        palette: [[60, 20, 110], [255, 60, 170], [40, 230, 255]],
        added_days_ago: 300,
        tracks: &[
            ("High Score", 212),
            ("Insert Coin", 189),
            ("Laser Hearts", 236),
            ("Pixel Rain", 203),
            ("Game Over (Again)", 251),
            ("Continue?", 198),
        ],
    },
    Album {
        artist: "Delta Mile",
        title: "Copper & Rust",
        genre: "Blues",
        year: 1972,
        style: Style::Stripes,
        palette: [[90, 40, 20], [190, 100, 50], [230, 190, 120]],
        added_days_ago: 260,
        tracks: &[
            ("Crossroads Diner", 254),
            ("Rust Belt Shuffle", 221),
            ("Copper Wire", 239),
            ("Muddy Water Morning", 287),
            ("Last Train South", 312),
        ],
    },
    Album {
        artist: "Juniper Vale",
        title: "Glass Garden",
        genre: "Pop",
        year: 2010,
        style: Style::Dots,
        palette: [[240, 236, 225], [80, 170, 120], [240, 110, 130]],
        added_days_ago: 200,
        tracks: &[
            ("Glass Garden", 214),
            ("Paper Hearts", 198),
            ("Greenhouse", 231),
            ("Butterfly Effect", 205),
            ("Sunroom", 187),
            ("Wildflowers", 243),
        ],
    },
    Album {
        artist: "Saltwater Quartet",
        title: "Low Tide Sessions",
        genre: "Jazz",
        year: 2004,
        style: Style::Waves,
        palette: [[20, 50, 80], [70, 140, 190], [230, 220, 190]],
        added_days_ago: 120,
        tracks: &[
            ("Blue Hour", 367),
            ("Brackish", 298),
            ("Undertow", 412),
            ("Sea Glass", 335),
            ("Last Call at the Pier", 389),
        ],
    },
    Album {
        artist: "Aldersgate Chamber Ensemble",
        title: "Concerto in Grey",
        genre: "Classical",
        year: 1998,
        style: Style::Rings,
        palette: [[226, 224, 218], [110, 110, 120], [170, 40, 40]],
        added_days_ago: 6,
        tracks: &[
            ("I. Allegro moderato", 512),
            ("II. Adagio", 634),
            ("III. Rondo", 456),
            ("Nocturne for Strings", 388),
        ],
    },
    Album {
        artist: "Velvet Antenna",
        title: "Static Bloom",
        genre: "Rock",
        year: 1994,
        style: Style::Shards,
        palette: [[30, 30, 34], [200, 40, 60], [240, 220, 80]],
        added_days_ago: 3,
        tracks: &[
            ("Static Bloom", 223),
            ("Feedback Loop", 245),
            ("Antenna", 198),
            ("Signal Lost", 267),
            ("Broadcast", 231),
            ("Radio Silence", 301),
        ],
    },
];

/// `~/Music/TuneBox Demo`.
pub fn default_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("Music").join("TuneBox Demo")
}

pub fn build(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("could not clear {}: {e}", dir.display()))?;
    }
    let library_dir = dir.join("library");
    let media = library_dir.join("TuneBox Media");
    let mut library = Library::with_default_playlists();
    let now = Utc::now();
    let mut ids: Vec<TrackId> = Vec::new();
    for (album_index, album) in ALBUMS.iter().enumerate() {
        eprintln!("demo: {} — {}", album.artist, album.title);
        let art = cover(album.style, album.palette)?;
        for (track_index, (title, seconds)) in album.tracks.iter().enumerate() {
            let meta = TrackMeta {
                title: (*title).into(),
                artist: album.artist.into(),
                album_artist: album.artist.into(),
                album: album.title.into(),
                genre: album.genre.into(),
                year: Some(album.year),
                track: NumberOf::new(track_index as u32 + 1, album.tracks.len() as u32),
                disc: NumberOf::new(1, 1),
                ..TrackMeta::default()
            };
            let path = media.join(organize::relative_path(&meta, title, "flac"));
            write_audio(&path, *seconds, album_index * 10 + track_index)?;
            tunebox_tags::write(&path, &meta).map_err(|e| e.to_string())?;
            let artwork = tunebox_tags::Artwork { mime: "image/jpeg".into(), data: art.clone() };
            tunebox_tags::write_artwork(&path, &artwork).map_err(|e| e.to_string())?;
            let file = tunebox_tags::read(&path).map_err(|e| e.to_string())?.file;
            let added = now - TimeDelta::days(album.added_days_ago) + TimeDelta::minutes(track_index as i64);
            ids.push(library.add_track(NewTrack { location: path, meta, file, date_added: added }));
        }
    }
    add_history(&mut library, &ids, now)?;
    add_playlists(&mut library, &ids)?;
    tunebox_library::store::save(&library, &library_dir.join("TuneBox Library.json")).map_err(|e| e.to_string())?;
    sync_player(&library, &dir.join("player"))?;
    eprintln!("demo: {} tracks in {}", library.len(), dir.display());
    Ok(())
}

/// A quiet three-note figure over a soft kick drum, different for every track. Mono content in a
/// stereo file, so FLAC stores it compactly.
fn write_audio(path: &Path, seconds: u32, seed: usize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    const RATE: u32 = 44_100;
    let roots = [220.0, 246.94, 261.63, 293.66, 329.63, 349.23, 392.0];
    let root = roots[seed % roots.len()];
    let mut writer = PcmWriter::create(path, LosslessFormat::Flac, RATE, 2).map_err(|e| e.to_string())?;
    let block = RATE as usize;
    for second in 0..seconds as usize {
        let note = root * [1.0, 1.25, 1.5][second % 3];
        let samples: Vec<f32> = (0..block)
            .map(|i| {
                let t = (second * block + i) as f32 / RATE as f32;
                let envelope = 1.0 - (i as f32 / block as f32) * 0.6;
                let tone: f32 = [(1.0, 1.0), (2.0, 0.5), (3.0, 0.3), (4.0, 0.2), (6.0, 0.1)]
                    .iter()
                    .map(|(harmonic, weight)| (t * note * harmonic * std::f32::consts::TAU).sin() * weight)
                    .sum();
                let beat = (i % (block / 2)) as f32 / RATE as f32;
                let kick = (beat * 55.0 * std::f32::consts::TAU).sin() * (-beat * 14.0).exp();
                tone * 0.03 * envelope + kick * 0.06
            })
            .collect();
        writer.write(&[samples.clone(), samples]).map_err(|e| e.to_string())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Ratings, play counts and last-played dates that make the smart playlists interesting.
fn add_history(library: &mut Library, ids: &[TrackId], now: chrono::DateTime<Utc>) -> Result<(), String> {
    const RATINGS: [u8; 11] = [5, 4, 3, 0, 4, 5, 2, 0, 3, 4, 5];
    for (i, id) in ids.iter().enumerate() {
        library.set_rating(*id, Rating::from_stars(RATINGS[i % RATINGS.len()])).map_err(|e| e.to_string())?;
        let plays = (i * 7 + 3) % 23;
        for play in 0..plays {
            let when =
                now - TimeDelta::days(((i * 13 + play * 5) % 40) as i64) - TimeDelta::hours((plays - play) as i64);
            library.record_play(*id, when).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn add_playlists(library: &mut Library, ids: &[TrackId]) -> Result<(), String> {
    let by_genre = |library: &Library, genres: &[&str]| -> Vec<TrackId> {
        ids.iter()
            .copied()
            .filter(|id| library.track(*id).is_some_and(|t| genres.contains(&t.meta.genre.as_str())))
            .collect()
    };
    let lists = [
        ("Sunday Morning", by_genre(library, &["Indie Folk", "Jazz", "Ambient"])),
        ("Workout", by_genre(library, &["Synthpop", "Rock"])),
        ("Dinner Party", by_genre(library, &["Jazz", "Classical", "Blues"])),
    ];
    for (name, tracks) in lists {
        let id = library.create_playlist(name).map_err(|e| e.to_string())?;
        library.add_to_playlist(id, &tracks).map_err(|e| e.to_string())?;
    }
    let road_trip = SmartRules {
        rules: vec![
            Rule::Text { field: TextField::Genre, op: TextOp::IsNot, value: "Classical".into() },
            Rule::Text { field: TextField::Genre, op: TextOp::IsNot, value: "Ambient".into() },
            Rule::Number { field: NumberField::Rating, op: NumberOp::GreaterThan(2) },
        ],
        limit: Some(Limit { amount: 90, unit: LimitUnit::Minutes, selected_by: SelectedBy::Random }),
        only_checked: true,
        random_seed: 7,
        ..SmartRules::default()
    };
    library.create_smart_playlist("Road Trip Mix", road_trip).map_err(|e| e.to_string())?;
    Ok(())
}

/// A folder standing in for a player, set up and synced with two playlists.
fn sync_player(library: &Library, root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root.join("Music")).map_err(|e| e.to_string())?;
    let mut device = Device::register(root, "Pocket Player").map_err(|e| e.to_string())?;
    let chosen: Vec<_> = library
        .playlists()
        .iter()
        .filter(|p| p.name == "Sunday Morning" || p.name == "Workout")
        .map(|p| p.id)
        .collect();
    device.manifest.settings.mode = SyncMode::Playlists(chosen.clone());
    let now = Utc::now();
    let playlists: Vec<PlaylistFile> = chosen
        .iter()
        .filter_map(|id| {
            Some(PlaylistFile {
                name: library.playlist(*id)?.name.clone(),
                tracks: library.playlist_tracks(*id, now).ok()?,
            })
        })
        .collect();
    let mut wanted: Vec<TrackId> = playlists.iter().flat_map(|p| p.tracks.clone()).collect();
    wanted.sort();
    wanted.dedup();
    let items: Vec<SyncItem> = wanted
        .iter()
        .filter_map(|id| library.track(*id))
        .map(|t| {
            let modified = std::fs::metadata(&t.location)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok());
            SyncItem::from_track(t, "Music", modified.map_or(0, |d| d.as_secs()))
        })
        .collect();
    let plan = tunebox_sync::plan(&items, playlists, &device.manifest);
    tunebox_sync::execute(&mut device, &plan, &AtomicBool::new(false), &mut |_| {}).map_err(|e| e.to_string())?;
    Ok(())
}
