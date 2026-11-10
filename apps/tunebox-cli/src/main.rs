//! `tunebox-cli`: every library feature without the GUI, for scripts, servers and agents.
//!
//! It opens the same library as the app (or the one given with `--library-dir`) through the
//! same engine, so what it does is exactly what the app would do.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;
use tunebox_engine::commands::COMMANDS;
use tunebox_engine::model::search::SearchScope;
use tunebox_engine::model::{Column, Rating, TrackId, format_clock};
use tunebox_engine::{NoticeLevel, Player, Session, Source, SyncMode, TrackQuery, VirtualDrive};

#[derive(Parser)]
#[command(name = "tunebox-cli", version, about = "TuneBox from the command line")]
struct Cli {
    /// The folder holding the library (default: ~/Music/TuneBox).
    #[arg(long, global = true)]
    library_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Song count, total time and size.
    Stats,
    /// List tracks.
    List {
        /// Only tracks in this playlist.
        #[arg(long)]
        playlist: Option<String>,
        /// Only tracks matching these words.
        #[arg(long)]
        search: Option<String>,
    },
    /// Add files or folders to the library.
    Import { paths: Vec<PathBuf> },
    /// List playlists.
    Playlists,
    /// Create a playlist from the tracks matching a search.
    NewPlaylist {
        name: String,
        #[arg(long)]
        search: Option<String>,
    },
    /// Rate the tracks matching a search (0–5 stars).
    Rate { search: String, stars: u8 },
    /// Play the tracks matching a search (all tracks without one) until they finish.
    Play {
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        shuffle: bool,
    },
    /// Show the audio CD in the drive.
    CdInfo {
        /// Use a synthetic disc instead of a drive (for trying TuneBox out).
        #[arg(long)]
        virtual_disc: bool,
        /// Look the names up on MusicBrainz.
        #[arg(long)]
        lookup: bool,
    },
    /// Import the audio CD in the drive.
    CdImport {
        #[arg(long)]
        virtual_disc: bool,
        #[arg(long)]
        lookup: bool,
    },
    /// List connected players.
    Devices,
    /// Set up the player mounted at ROOT for syncing.
    SetupDevice { root: PathBuf, name: String },
    /// Sync the player mounted at ROOT (optionally only these playlists).
    Sync {
        root: PathBuf,
        #[arg(long = "playlist")]
        playlists: Vec<String>,
    },
    /// List the app's commands and shortcuts.
    Commands,
}

fn main() -> ExitCode {
    env_logger::init();
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("tunebox-cli: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let folder = cli.library_dir.unwrap_or_else(tunebox_platform::tunebox_folder);
    let mut session = Session::open(tunebox_platform::app_paths(&folder), Player::new()).map_err(|e| e.to_string())?;
    match cli.command {
        Command::Stats => {
            let all = session.query(&TrackQuery::new(Source::Music));
            println!("{}", session.summary(&all).label());
        }
        Command::List { playlist, search } => {
            let source = match playlist {
                Some(name) => Source::Playlist(find_playlist(&session, &name)?),
                None => Source::Music,
            };
            let ids = search_ids(&session, source, search.as_deref());
            print_tracks(&session, &ids);
        }
        Command::Import { paths } => {
            session.import(paths).map_err(|e| e.to_string())?;
            wait_for_jobs(&mut session);
        }
        Command::Playlists => {
            for playlist in session.library().playlists() {
                let count = session.source_tracks(&Source::Playlist(playlist.id)).len();
                let kind = if playlist.is_smart() { "smart" } else { "" };
                println!("{:<30} {:>6} songs  {kind}", playlist.name, count);
            }
        }
        Command::NewPlaylist { name, search } => {
            let ids = search_ids(&session, Source::Music, search.as_deref());
            session.create_playlist(&name, &ids).map_err(|e| e.to_string())?;
            println!("Created “{name}” with {} songs.", ids.len());
        }
        Command::Rate { search, stars } => {
            let ids = search_ids(&session, Source::Music, Some(&search));
            session.set_rating(&ids, Rating::from_stars(stars));
            println!("Rated {} songs.", ids.len());
        }
        Command::Play { search, shuffle } => play(&mut session, search.as_deref(), shuffle),
        Command::CdInfo { virtual_disc, lookup } => {
            read_cd(&mut session, virtual_disc, lookup)?;
            print_cd(&session);
        }
        Command::CdImport { virtual_disc, lookup } => {
            read_cd(&mut session, virtual_disc, lookup)?;
            print_cd(&session);
            session.import_cd().map_err(|e| e.to_string())?;
            wait_for_jobs(&mut session);
        }
        Command::Devices => {
            session.refresh_devices(&tunebox_platform::volumes());
            for device in session.devices() {
                let managed = if device.device.is_some() { "set up" } else { "not set up" };
                println!("{:<24} {}  ({managed})", device.name(), device.volume.root.display());
            }
        }
        Command::SetupDevice { root, name } => {
            session.refresh_devices(&tunebox_platform::volumes());
            tunebox_engine::Device::register(&root, &name).map_err(|e| e.to_string())?;
            println!("“{name}” is ready to sync.");
        }
        Command::Sync { root, playlists } => sync(&mut session, &root, &playlists)?,
        Command::Commands => {
            for command in COMMANDS {
                let shortcut = command.shortcut.map(|s| s.label(cfg!(target_os = "macos"))).unwrap_or_default();
                println!("{:<10} {:<32} {shortcut}", command.menu.title(), command.label);
            }
        }
    }
    session.shutdown().map_err(|e| e.to_string())?;
    print_notices(&mut session);
    Ok(())
}

fn find_playlist(session: &Session, name: &str) -> Result<tunebox_engine::model::PlaylistId, String> {
    session
        .library()
        .playlists()
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .map(|p| p.id)
        .ok_or_else(|| format!("there is no playlist called “{name}” (see `tunebox-cli playlists`)"))
}

fn search_ids(session: &Session, source: Source, search: Option<&str>) -> Vec<TrackId> {
    let mut query = TrackQuery::new(source);
    query.search = search.unwrap_or_default().to_owned();
    query.scope = SearchScope::All;
    session.query(&query)
}

fn print_tracks(session: &Session, ids: &[TrackId]) {
    let columns = [Column::Name, Column::Time, Column::Artist, Column::Album, Column::Rating, Column::Plays];
    for id in ids {
        let Some(track) = session.track(*id) else { continue };
        let cells: Vec<String> = columns.iter().map(|c| c.text(track)).collect();
        println!("{:<36.36} {:>7} {:<24.24} {:<28.28} {:<5} {:>4}", cells[0], cells[1], cells[2], cells[3], cells[4], cells[5]);
    }
    println!("{}", session.summary(ids).label());
}

/// Ticks the session until its background work is done, printing progress.
fn wait_for_jobs(session: &mut Session) {
    loop {
        session.tick();
        print_notices(session);
        let jobs = session.jobs();
        let Some(job) = jobs.first() else { break };
        let percent = job.progress.map(|p| format!("{:>3.0}%", p * 100.0)).unwrap_or_else(|| "   …".into());
        eprint!("\r\x1b[2K{percent}  {}  {}", job.title, job.message);
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!();
}

fn print_notices(session: &mut Session) {
    for notice in session.take_notices() {
        match notice.level {
            NoticeLevel::Info => println!("{}", notice.text),
            NoticeLevel::Warning | NoticeLevel::Error => eprintln!("{}", notice.text),
        }
    }
}

fn play(session: &mut Session, search: Option<&str>, shuffle: bool) {
    let ids = search_ids(session, Source::Music, search);
    session.set_shuffle(shuffle);
    session.play_tracks(&ids, 0);
    let mut last = None;
    while let Some(now) = session.now_playing() {
        if last != Some(now.track) {
            if let Some(track) = session.track(now.track) {
                println!("▶ {} — {} ({})", track.title(), track.meta.artist, format_clock(now.duration));
            }
            last = Some(now.track);
        }
        session.tick();
        print_notices(session);
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn read_cd(session: &mut Session, virtual_disc: bool, lookup: bool) -> Result<(), String> {
    if virtual_disc {
        session.read_disc(Arc::new(VirtualDrive::new("Virtual Drive", vec![3, 4, 2, 5])));
    } else {
        session.scan_drives();
    }
    wait_for_jobs(session);
    if session.cd().is_none() {
        return Err("there is no audio CD in the drive".into());
    }
    if lookup {
        session.lookup_disc_names().map_err(|e| e.to_string())?;
        wait_for_jobs(session);
    }
    Ok(())
}

fn print_cd(session: &Session) {
    let Some(cd) = session.cd() else { return };
    println!("{} — {}  (disc id {})", cd.info.artist, cd.info.album, cd.disc_id);
    for (index, number) in cd.audio_numbers().iter().enumerate() {
        let (title, artist) = cd.info.tracks.get(index).cloned().unwrap_or_default();
        let length = cd.toc.duration(*number).map(format_clock).unwrap_or_default();
        println!("{number:>3}  {title:<40} {artist:<20} {length:>6}");
    }
}

fn sync(session: &mut Session, root: &std::path::Path, playlists: &[String]) -> Result<(), String> {
    session.refresh_devices(&tunebox_platform::volumes());
    if session.device(root).is_none() {
        // Not a removable volume the platform reports (a folder, for example): sync it anyway.
        let volume = tunebox_engine::Volume { root: root.to_path_buf(), label: String::new(), total_bytes: u64::MAX, free_bytes: u64::MAX, removable: true };
        session.refresh_devices(&[volume]);
    }
    if !playlists.is_empty() {
        let ids = playlists.iter().map(|name| find_playlist(session, name)).collect::<Result<Vec<_>, _>>()?;
        let mut settings = session.device(root).and_then(|d| d.device.as_ref()).map(|d| d.manifest.settings.clone()).unwrap_or_default();
        settings.mode = SyncMode::Playlists(ids);
        session.set_device_settings(root, settings).map_err(|e| e.to_string())?;
    }
    session.sync_device(root).map_err(|e| e.to_string())?;
    wait_for_jobs(session);
    Ok(())
}
