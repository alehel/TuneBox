//! `tunebox-cli`: every library feature without the GUI, for scripts, servers and agents.
//!
//! It opens the same library as the app (or the one given with `--library-dir`) through the
//! same engine, so what it does is exactly what the app would do.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod details;

use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;
use tunebox_engine::Place;
use tunebox_engine::commands::COMMANDS;
use tunebox_engine::model::search::SearchScope;
use tunebox_engine::model::{Column, Rating, TrackId, format_clock};
use tunebox_engine::{
    FileStatus, LibrarySnapshot, LibraryView, NoticeLevel, Player, Session, Source, SyncMode, TrackQuery, VirtualDrive,
};

#[derive(Parser)]
#[command(name = "tunebox-cli", version, about = "TuneBox from the command line")]
struct Cli {
    /// A folder holding the library, preferences and media, instead of the usual places.
    #[arg(long, global = true)]
    library_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum FolderAction {
    /// List a folder (read only unless --writable) and add its songs to the library.
    Add {
        path: PathBuf,
        /// Let TuneBox write in it: edit tags, delete files, put CD imports there.
        #[arg(long)]
        writable: bool,
    },
    /// Stop listing a folder (its songs stay in the library).
    Remove { path: PathBuf },
    /// Add songs that appeared in the folders since they were last scanned.
    Scan,
    /// Watch the folders and keep the library up to date as songs appear or change, until
    /// stopped with Ctrl-C.
    Watch,
    /// Put CD imports and copies in this folder (it must not be read only).
    Imports { path: PathBuf },
}

#[derive(Subcommand)]
enum ArtworkAction {
    /// Set the cover of the songs matching a search to an image. JPEG and PNG are used as they
    /// are; GIF, BMP and WebP are converted, and pictures over 1400 pixels scaled down. Songs in
    /// read-only library folders keep the cover in TuneBox, their files untouched.
    Set { search: String, image: PathBuf },
    /// Remove the cover of the songs matching a search.
    Remove { search: String },
    /// Save the cover of the first song matching a search that has one to a file.
    Export { search: String, file: PathBuf },
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
    /// List songs whose files are missing, or on a drive or share that is not connected.
    Missing,
    /// Link a missing song to its file where it is now. The search must match exactly one
    /// missing song (see `missing`). Other missing songs that moved the same way are linked too.
    Locate { search: String, file: PathBuf },
    /// Show every detail of the songs matching a search.
    Info { search: String },
    /// Change details of the songs matching a search, e.g.
    /// `edit "nocturne" --set conductor="C. Baton" --set release-date=2011-04-02`.
    /// An empty value clears the field. `info-fields` lists the additional field names.
    Edit {
        search: String,
        #[arg(long = "set", value_name = "FIELD=VALUE", required = true)]
        assignments: Vec<String>,
    },
    /// Read the details of songs from their files again, after another program changed them
    /// (all songs without --search). Songs with changes kept in TuneBox only are left as they
    /// are unless --replace-library-edits is given.
    Refresh {
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        replace_library_edits: bool,
    },
    /// Set, remove or save album artwork.
    Artwork {
        #[command(subcommand)]
        action: ArtworkAction,
    },
    /// List the additional fields the app's Get Info always shows, or pin and unpin them.
    InfoFields {
        #[arg(long, value_name = "FIELD")]
        pin: Vec<String>,
        #[arg(long, value_name = "FIELD")]
        unpin: Vec<String>,
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
    /// List your library folders, or add, remove or scan them.
    Folders {
        #[command(subcommand)]
        action: Option<FolderAction>,
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

impl Command {
    /// Whether the command only reads the library. Those run on a snapshot, which takes no lock,
    /// so they work while the app or another tunebox-cli has the library open.
    fn reads_only(&self) -> bool {
        match self {
            Command::Stats
            | Command::List { .. }
            | Command::Playlists
            | Command::Info { .. }
            | Command::Missing
            | Command::Commands => true,
            Command::InfoFields { pin, unpin } => pin.is_empty() && unpin.is_empty(),
            Command::Folders { action } => action.is_none(),
            Command::Artwork { action } => matches!(action, ArtworkAction::Export { .. }),
            _ => false,
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let paths = cli.library_dir.as_deref().map_or_else(tunebox_platform::default_paths, tunebox_platform::app_paths);
    if cli.command.reads_only() {
        let snapshot = LibrarySnapshot::open(&paths).map_err(|e| e.to_string())?;
        return show(snapshot.view(), cli.command);
    }
    let mut session = Session::open(paths, Player::new()).map_err(|e| e.to_string())?;
    let result = change(&mut session, cli.command);
    session.shutdown().map_err(|e| e.to_string())?;
    print_notices(&mut session);
    result
}

/// The commands that only read.
fn show(view: LibraryView<'_>, command: Command) -> Result<(), String> {
    match command {
        Command::Stats => {
            let all = view.query(&TrackQuery::new(Source::Music));
            println!("{}", view.summary(&all).label());
        }
        Command::List { playlist, search } => {
            let source = match playlist {
                Some(name) => Source::Playlist(find_playlist(view, &name)?),
                None => Source::Music,
            };
            let ids = search_ids(view, source, search.as_deref());
            print_tracks(view, &ids);
        }
        Command::Playlists => {
            for playlist in view.library().playlists() {
                let count = view.source_tracks(&Source::Playlist(playlist.id)).len();
                let kind = if playlist.is_smart() { "smart" } else { "" };
                println!("{:<30} {:>6} songs  {kind}", playlist.name, count);
            }
        }
        Command::Info { search } => details::show(view, &search)?,
        Command::Missing => print_unplayable(view),
        Command::InfoFields { .. } => details::print_info_fields(view.preferences()),
        Command::Folders { .. } => print_folders(view.preferences()),
        Command::Artwork { action: ArtworkAction::Export { search, file } } => export_artwork(view, &search, &file)?,
        Command::Commands => {
            for command in COMMANDS {
                let shortcut = command.shortcut.map(|s| s.label(cfg!(target_os = "macos"))).unwrap_or_default();
                println!("{:<10} {:<32} {shortcut}", command.menu.title(), command.label);
            }
        }
        // `run` sends only reading commands here; this keeps a mistake from doing harm.
        _ => return Err("this command changes the library and needs it open for changing".into()),
    }
    Ok(())
}

/// The commands that change something, on the session (which holds the library's lock).
fn change(session: &mut Session, command: Command) -> Result<(), String> {
    match command {
        Command::Import { paths } => {
            session.import(paths).map_err(|e| e.to_string())?;
            wait_for_jobs(session);
        }
        Command::NewPlaylist { name, search } => {
            let ids = search_ids(session.view(), Source::Music, search.as_deref());
            session.create_playlist(&name, &ids).map_err(|e| e.to_string())?;
            println!("Created “{name}” with {} songs.", ids.len());
        }
        Command::Rate { search, stars } => {
            let ids = search_ids(session.view(), Source::Music, Some(&search));
            session.set_rating(&ids, Rating::from_stars(stars));
            println!("Rated {} songs.", ids.len());
        }
        Command::Edit { search, assignments } => details::edit(session, &search, &assignments)?,
        Command::Locate { search, file } => locate(session, &search, file)?,
        Command::Refresh { search, replace_library_edits } => {
            details::refresh(session, search.as_deref(), replace_library_edits)?;
            wait_for_jobs(session);
        }
        Command::InfoFields { pin, unpin } => details::info_fields(session, &pin, &unpin)?,
        Command::Artwork { action } => artwork(session, action)?,
        Command::Play { search, shuffle } => play(session, search.as_deref(), shuffle),
        Command::CdInfo { virtual_disc, lookup } => {
            read_cd(session, virtual_disc, lookup)?;
            print_cd(session);
        }
        Command::CdImport { virtual_disc, lookup } => {
            read_cd(session, virtual_disc, lookup)?;
            print_cd(session);
            session.import_cd().map_err(|e| e.to_string())?;
            wait_for_jobs(session);
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
        Command::Sync { root, playlists } => sync(session, &root, &playlists)?,
        Command::Folders { action } => folders(session, action)?,
        command @ (Command::Stats
        | Command::List { .. }
        | Command::Playlists
        | Command::Info { .. }
        | Command::Missing
        | Command::Commands) => show(session.view(), command)?,
    }
    Ok(())
}

fn find_playlist(view: LibraryView<'_>, name: &str) -> Result<tunebox_engine::model::PlaylistId, String> {
    view.library()
        .playlists()
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .map(|p| p.id)
        .ok_or_else(|| format!("there is no playlist called “{name}” (see `tunebox-cli playlists`)"))
}

fn search_ids(view: LibraryView<'_>, source: Source, search: Option<&str>) -> Vec<TrackId> {
    let mut query = TrackQuery::new(source);
    query.search = search.unwrap_or_default().to_owned();
    query.scope = SearchScope::All;
    view.query(&query)
}

/// Links the one missing song matching `search` to `file`, then waits for the search for others.
fn locate(session: &mut Session, search: &str, file: PathBuf) -> Result<(), String> {
    // A fresh look, waited for: the songs found missing are the ones missing now.
    session.set_network_mounts(tunebox_platform::network_mounts());
    session.check_files_now();
    let matches: Vec<TrackId> = search_ids(session.view(), Source::Music, Some(search))
        .into_iter()
        .filter(|id| *session.file_status(*id) == FileStatus::Missing)
        .collect();
    let id = match matches[..] {
        [id] => id,
        [] => return Err(format!("no missing song matches “{search}” (see `tunebox-cli missing`)")),
        _ => return Err(format!("{} missing songs match “{search}”; narrow the search to one", matches.len())),
    };
    session.locate(id, file).map_err(|e| e.to_string())?;
    wait_for_jobs(session);
    Ok(())
}

/// Sets or removes the cover of the songs matching a search, and waits for it.
fn artwork(session: &mut Session, action: ArtworkAction) -> Result<(), String> {
    let (search, image) = match action {
        ArtworkAction::Set { search, image } => (search, Some(image)),
        ArtworkAction::Remove { search } => (search, None),
        export @ ArtworkAction::Export { .. } => {
            return show(session.view(), Command::Artwork { action: export });
        }
    };
    let ids = search_ids(session.view(), Source::Music, Some(&search));
    if ids.is_empty() {
        return Err(format!("no song matches “{search}”"));
    }
    // A fresh look, waited for, so missing songs and disconnected shares are skipped, not tried.
    session.set_network_mounts(tunebox_platform::network_mounts());
    session.check_files_now();
    match image {
        Some(image) => session.set_artwork(&ids, &image),
        None => session.remove_artwork(&ids),
    }
    .map_err(|e| e.to_string())?;
    wait_for_jobs(session);
    Ok(())
}

/// Saves the cover of the first song matching `search` that has one. Songs whose files cannot
/// be reached now are passed over, so a disconnected share cannot stall the command.
fn export_artwork(view: LibraryView<'_>, search: &str, file: &std::path::Path) -> Result<(), String> {
    let ids = search_ids(view, Source::Music, Some(search));
    if ids.is_empty() {
        return Err(format!("no song matches “{search}”"));
    }
    let unplayable: Vec<TrackId> =
        view.find_unplayable_among(&ids, &tunebox_platform::network_mounts()).into_iter().map(|(id, _)| id).collect();
    for id in ids.iter().filter(|id| !unplayable.contains(id)) {
        if let Some(cover) = view.export_cover(*id, file).map_err(|e| e.to_string())? {
            let title = view.track(*id).map(|t| t.title().to_owned()).unwrap_or_default();
            let kind = if cover.extension() == "png" { "PNG" } else { "JPEG" };
            println!("Saved the cover of “{title}” ({kind}) to {}.", file.display());
            return Ok(());
        }
    }
    Err(format!("none of the {} songs matching “{search}” that can be reached has a cover", ids.len()))
}

/// Songs whose files are missing, then those on each drive or share that is not connected.
fn print_unplayable(view: LibraryView<'_>) {
    let found = view.find_unplayable(&tunebox_platform::network_mounts());
    let missing: Vec<TrackId> = found.iter().filter(|(_, s)| *s == FileStatus::Missing).map(|(id, _)| *id).collect();
    let mut away: BTreeMap<Place, Vec<TrackId>> = BTreeMap::new();
    for (id, status) in &found {
        if let FileStatus::Unavailable(place) = status {
            away.entry(place.clone()).or_default().push(*id);
        }
    }
    if found.is_empty() {
        println!("Every song's file is where the library says.");
    }
    if !missing.is_empty() {
        println!("Missing ({}): moved, renamed or deleted", missing.len());
        for id in &missing {
            if let Some(track) = view.track(*id) {
                println!("  {:<36.36} {}", track.title(), track.location.display());
            }
        }
    }
    for (place, ids) in away {
        let state = if place.network { "not connected" } else { "cannot be found" };
        println!("On “{}” ({}), {state} ({} songs)", place.name(), place.root.display(), ids.len());
        for id in ids {
            if let Some(track) = view.track(id) {
                println!("  {:<36.36} {}", track.title(), track.meta.artist);
            }
        }
    }
}

fn print_tracks(view: LibraryView<'_>, ids: &[TrackId]) {
    let columns = [Column::Name, Column::Time, Column::Artist, Column::Album, Column::Rating, Column::Plays];
    for id in ids {
        let Some(track) = view.track(*id) else { continue };
        let cells: Vec<String> = columns.iter().map(|c| c.text(track)).collect();
        println!(
            "{:<36.36} {:>7} {:<24.24} {:<28.28} {:<5} {:>4}",
            cells[0], cells[1], cells[2], cells[3], cells[4], cells[5]
        );
    }
    println!("{}", view.summary(ids).label());
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
    let ids = search_ids(session.view(), Source::Music, search);
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
        let volume = tunebox_engine::Volume {
            root: root.to_path_buf(),
            label: String::new(),
            total_bytes: u64::MAX,
            free_bytes: u64::MAX,
            removable: true,
        };
        session.refresh_devices(&[volume]);
    }
    if !playlists.is_empty() {
        let ids = playlists.iter().map(|name| find_playlist(session.view(), name)).collect::<Result<Vec<_>, _>>()?;
        let mut settings = session
            .device(root)
            .and_then(|d| d.device.as_ref())
            .map(|d| d.manifest.settings.clone())
            .unwrap_or_default();
        settings.mode = SyncMode::Playlists(ids);
        session.set_device_settings(root, settings).map_err(|e| e.to_string())?;
    }
    session.sync_device(root).map_err(|e| e.to_string())?;
    wait_for_jobs(session);
    Ok(())
}

fn folders(session: &mut Session, action: Option<FolderAction>) -> Result<(), String> {
    let mut prefs = session.preferences().clone();
    match action {
        Some(FolderAction::Add { path, writable }) => {
            prefs.add_library_folder(path.clone(), !writable);
            session.set_preferences(prefs);
            session.import(vec![path]).map_err(|e| e.to_string())?;
            wait_for_jobs(session);
        }
        Some(FolderAction::Remove { path }) => {
            prefs.remove_library_folder(&path);
            session.set_preferences(prefs);
        }
        Some(FolderAction::Imports { path }) => {
            if !tunebox_engine::files::may_write(&prefs, &path) {
                return Err(format!(
                    "{} is in a read-only library folder; list it as --writable first",
                    path.display()
                ));
            }
            prefs.media_folder = path;
            session.set_preferences(prefs);
        }
        Some(FolderAction::Watch) => watch(session)?,
        Some(FolderAction::Scan) => {
            session.scan_library_folders().map_err(|e| e.to_string())?;
            wait_for_jobs(session);
        }
        None => {}
    }
    print_folders(session.preferences());
    Ok(())
}

fn print_folders(prefs: &tunebox_engine::Preferences) {
    if prefs.library_folders.is_empty() {
        println!("No library folders are listed. Add one with `tunebox-cli folders add <path>`.");
    }
    for folder in &prefs.library_folders {
        let rule = if folder.read_only { "read only" } else { "writable" };
        println!("{:<10} {}", rule, folder.path.display());
    }
    let imports = if tunebox_engine::files::may_write(prefs, &prefs.media_folder) {
        ""
    } else {
        "  (in a read-only folder: CD imports are blocked)"
    };
    println!("CD imports and copies go to {}{imports}", prefs.media_folder.display());
}

/// Feeds the folder watcher's changes to the session until the process is stopped. The library
/// is saved after every change, since Ctrl-C ends the process without a chance to save.
fn watch(session: &mut Session) -> Result<(), String> {
    let folders: Vec<PathBuf> = session.preferences().library_folders.iter().map(|f| f.path.clone()).collect();
    if folders.is_empty() {
        return Err("there are no library folders to watch; add one with `tunebox-cli folders add`".into());
    }
    let mut watcher = tunebox_platform::FolderWatcher::start(&folders, tunebox_platform::QUIET, || {})?;
    for (folder, reason) in watcher.failures() {
        eprintln!("cannot watch {}: {reason}", folder.display());
    }
    println!("Watching {} folders. Press Ctrl-C to stop.", folders.len());
    loop {
        let changed = watcher.settled();
        if !changed.is_empty() {
            session.folders_changed(changed);
        }
        let was_busy = !session.jobs().is_empty();
        session.tick();
        print_notices(session);
        if was_busy && session.jobs().is_empty() {
            session.save().map_err(|e| e.to_string())?;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(args: &[&str]) -> Command {
        let mut all = vec!["tunebox-cli"];
        all.extend_from_slice(args);
        Cli::try_parse_from(all).unwrap().command
    }

    #[test]
    fn only_commands_that_cannot_change_anything_skip_the_lock() {
        for reads in [
            &["stats"][..],
            &["list", "--search", "x"],
            &["playlists"],
            &["info", "x"],
            &["info-fields"],
            &["artwork", "export", "x", "/cover.jpg"],
            &["missing"],
            &["folders"],
            &["commands"],
        ] {
            assert!(command(reads).reads_only(), "{reads:?}");
        }
        for changes in [
            &["rate", "x", "3"][..],
            &["edit", "x", "--set", "mood=Calm"],
            &["locate", "x", "/x.flac"],
            &["info-fields", "--pin", "mood"],
            &["folders", "scan"],
            &["folders", "watch"],
            &["import", "/x"],
            &["refresh"],
            &["artwork", "set", "x", "/cover.jpg"],
            &["artwork", "remove", "x"],
            &["play"],
        ] {
            assert!(!command(changes).reads_only(), "{changes:?}");
        }
    }
}
