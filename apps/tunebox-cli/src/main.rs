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

mod audiobook;
mod details;
mod find_artwork;
mod plugins;
mod rename;

use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;
use tunebox_engine::Place;
use tunebox_engine::commands::COMMANDS;
use tunebox_engine::model::search::SearchScope;
use tunebox_engine::model::{Column, MediaKind, Rating, TrackId, format_clock};
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
    /// List the MusicBrainz releases (editions) of the album of the songs matching a search,
    /// with their ids. Asks musicbrainz.org; changes nothing.
    Find {
        search: String,
        /// Search for this artist instead of the songs' album artist.
        #[arg(long)]
        artist: Option<String>,
        /// Search for this album title instead of the songs' album.
        #[arg(long)]
        album: Option<String>,
    },
    /// Put the front cover of a release (an id from `artwork find`) from the Cover Art Archive
    /// into the songs matching a search. A release without a cover of its own gets its
    /// album's cover from another edition.
    Fetch {
        search: String,
        #[arg(long)]
        release: String,
        #[arg(long)]
        artist: Option<String>,
        #[arg(long)]
        album: Option<String>,
    },
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
        /// Only music, or only audiobooks ("music", "audiobook").
        #[arg(long, value_parser = parse_kind)]
        kind: Option<MediaKind>,
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
    /// Make an audiobook from several CDs: `audiobook start` with disc 1 in the drive, `rip` each
    /// disc in turn, edit the chapters, `done` after the last disc, then `build`.
    Audiobook {
        #[command(subcommand)]
        action: audiobook::AudiobookAction,
    },
    /// Set, remove or save album artwork.
    Artwork {
        #[command(subcommand)]
        action: ArtworkAction,
    },
    /// Rename the files of the songs matching a search by their tags, and file them in folders,
    /// e.g. `rename --search lanterns --pattern "{album-artist|artist}/{album}/{track:02} {title}"`.
    /// Shows what would happen; --apply renames. Fields are those `edit` takes; `{a|b}` uses the
    /// first that is set, `{track:02}` pads with zeros, and `/` makes folders. Files never
    /// replace one another, never leave their drive, and read-only folders are left alone.
    Rename {
        #[arg(long)]
        search: String,
        #[arg(long)]
        pattern: String,
        /// File them inside the library folder each song is in, or inside the media folder.
        #[arg(long, value_enum, default_value = "folder")]
        to: rename::RenameTo,
        /// Rename, rather than only show what would happen.
        #[arg(long)]
        apply: bool,
        /// Rename the others when some songs' new names are taken, leaving those as they are.
        #[arg(long)]
        skip_conflicts: bool,
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
        /// Then get the release's cover from the Cover Art Archive (needs --lookup).
        #[arg(long, requires = "lookup")]
        cover: bool,
    },
    /// Import the audio CD in the drive.
    CdImport {
        #[arg(long)]
        virtual_disc: bool,
        #[arg(long)]
        lookup: bool,
        /// Get the release's cover from the Cover Art Archive and put it into the imported
        /// songs (needs --lookup).
        #[arg(long, requires = "lookup")]
        cover: bool,
    },
    /// Show the read offset of the CD drive in use, or set it (in samples, e.g. --set=+667). Look
    /// the drive up in the AccurateRip drive list: https://www.accuraterip.com/driveoffsets.htm
    CdOffset {
        #[arg(long, allow_hyphen_values = true, value_name = "SAMPLES")]
        set: Option<i64>,
        #[arg(long)]
        virtual_disc: bool,
    },
    /// List connected players: mounted ones by folder, MTP ones by an id starting "mtp:".
    Devices,
    /// Set up the player at ROOT (its folder, or its "mtp:" id from `devices`) for syncing.
    SetupDevice {
        root: PathBuf,
        name: String,
        /// The player that will play the files: generic, rockbox, echo-mini or echo-nano.
        #[arg(long, default_value = "generic")]
        player: String,
    },
    /// Sync the player at ROOT (its folder or "mtp:" id; optionally only these playlists).
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
    /// Load the visualizer plug-ins and run each on made-up music: what they draw, how long they
    /// take, and any TuneBox would refuse or turn off.
    VisualizerPlugins {
        /// A folder of .wasm files instead of TuneBox's Visualizers folder.
        #[arg(long)]
        folder: Option<PathBuf>,
        /// Frames to draw with each plug-in.
        #[arg(long, default_value_t = 300)]
        frames: u32,
    },
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
            // The preview only reads; renaming needs the library open for changing.
            Command::Rename { apply, .. } => !apply,
            Command::Folders { action } => action.is_none(),
            Command::Artwork { action } => matches!(action, ArtworkAction::Export { .. }),
            _ => false,
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let paths = cli.library_dir.as_deref().map_or_else(tunebox_platform::default_paths, tunebox_platform::app_paths);
    // Plug-ins have nothing to do with the library, so this opens neither it nor a snapshot.
    if let Command::VisualizerPlugins { folder, frames } = &cli.command {
        return plugins::check(folder.as_deref().unwrap_or(&paths.visualizer_plugins_folder), *frames);
    }
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
            let all = view.query(&TrackQuery::new(Source::Library));
            println!("{}", view.summary(&all).label());
        }
        Command::List { playlist, search, kind } => {
            let source = match (playlist, kind) {
                (Some(name), _) => Source::Playlist(find_playlist(view, &name)?),
                (None, Some(MediaKind::Music)) => Source::Music,
                (None, Some(MediaKind::Audiobook)) => Source::Audiobooks,
                (None, None) => Source::Library,
            };
            let mut ids = search_ids(view, source, search.as_deref());
            if let Some(kind) = kind {
                ids.retain(|id| view.library().track(*id).is_some_and(|t| t.kind == kind));
            }
            let (one, many) = if kind == Some(MediaKind::Audiobook) { ("book", "books") } else { ("song", "songs") };
            print_tracks(view, &ids, one, many);
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
        Command::Rename { search, pattern, to, .. } => rename::preview(view, &search, &pattern, to)?,
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
            let ids = search_ids(session.view(), Source::Library, search.as_deref());
            session.create_playlist(&name, &ids).map_err(|e| e.to_string())?;
            println!("Created “{name}” with {} songs.", ids.len());
        }
        Command::Rate { search, stars } => {
            let ids = search_ids(session.view(), Source::Library, Some(&search));
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
        Command::Audiobook { action } => audiobook::run(session, action)?,
        Command::Rename { search, pattern, to, apply: true, skip_conflicts } => {
            rename::apply(session, &search, &pattern, to, skip_conflicts)?
        }
        Command::Play { search, shuffle } => play(session, search.as_deref(), shuffle),
        Command::CdInfo { virtual_disc: virtual_wanted, lookup, cover } => {
            read_cd(session, virtual_disc(virtual_wanted), lookup, cover)?;
            print_cd(session);
        }
        Command::CdImport { virtual_disc: virtual_wanted, lookup, cover } => {
            read_cd(session, virtual_disc(virtual_wanted), lookup, cover)?;
            print_cd(session);
            session.import_cd().map_err(|e| e.to_string())?;
            wait_for_jobs(session);
        }
        Command::CdOffset { set, virtual_disc: virtual_wanted } => {
            read_cd(session, virtual_disc(virtual_wanted), false, false)?;
            if let Some(samples) = set {
                session.set_cd_read_offset(samples).map_err(|e| e.to_string())?;
            }
            print_read_offset(session);
        }
        Command::Devices => {
            refresh_all_devices(session);
            for drive in session.drives() {
                println!("{:<24} {}  (a drive: `setup-device` it to sync)", drive.label, drive.root.display());
            }
            for device in session.devices() {
                let state = match (&device.problem, &device.device) {
                    (Some(problem), _) => problem.as_str(),
                    (None, Some(_)) => "set up",
                    (None, None) => "not set up",
                };
                println!("{:<24} {}  ({state})", device.name(), device.volume.root.display());
            }
        }
        Command::SetupDevice { root, name, player } => {
            let model = tunebox_engine::PlayerModel::from_id(&player).ok_or_else(|| {
                let known: Vec<&str> = tunebox_engine::PlayerModel::ALL.iter().map(|m| m.id()).collect();
                format!("Unknown player “{player}”. Choose one of: {}.", known.join(", "))
            })?;
            refresh_all_devices(session);
            let listed = session.device(&root).is_some() || session.drives().iter().any(|d| d.root == root);
            if !listed {
                // Not a drive the platform reports (a folder, for example): set it up anyway.
                session.refresh_devices(&[folder_volume(&root)]);
            }
            session.register_device(&root, &name, model).map_err(|e| e.to_string())?;
            println!("“{name}” is ready to sync, for {}.", model.label());
        }
        Command::Sync { root, playlists } => sync(session, &root, &playlists)?,
        Command::Folders { action } => folders(session, action)?,
        Command::VisualizerPlugins { folder, frames } => {
            plugins::check(folder.as_deref().unwrap_or(session.visualizer_plugins_folder()), frames)?
        }
        command @ (Command::Stats
        | Command::List { .. }
        | Command::Playlists
        | Command::Info { .. }
        | Command::Missing
        | Command::Rename { apply: false, .. }
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

pub(crate) fn search_ids(view: LibraryView<'_>, source: Source, search: Option<&str>) -> Vec<TrackId> {
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
    let matches: Vec<TrackId> = search_ids(session.view(), Source::Library, Some(search))
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
        ArtworkAction::Find { search, artist, album } => {
            return find_artwork::find(session, &find_artwork::Terms { search, artist, album });
        }
        ArtworkAction::Fetch { search, release, artist, album } => {
            return find_artwork::fetch(session, &find_artwork::Terms { search, artist, album }, &release);
        }
    };
    let ids = search_ids(session.view(), Source::Library, Some(&search));
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
    let ids = search_ids(view, Source::Library, Some(search));
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

/// Prints the tracks, then their count (as `one` or `many`), length and size.
fn print_tracks(view: LibraryView<'_>, ids: &[TrackId], one: &str, many: &str) {
    let columns = [Column::Name, Column::Time, Column::Artist, Column::Album, Column::Rating, Column::Plays];
    for id in ids {
        let Some(track) = view.track(*id) else { continue };
        let cells: Vec<String> = columns.iter().map(|c| c.text(track)).collect();
        println!(
            "{:<36.36} {:>7} {:<24.24} {:<28.28} {:<5} {:>4}",
            cells[0], cells[1], cells[2], cells[3], cells[4], cells[5]
        );
    }
    println!("{}", view.summary(ids).label_counting(one, many));
}

/// Ticks the session until its background work is done, printing progress.
pub(crate) fn wait_for_jobs(session: &mut Session) {
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

pub(crate) fn print_notices(session: &mut Session) {
    for notice in session.take_notices() {
        match notice.level {
            NoticeLevel::Info => println!("{}", notice.text),
            NoticeLevel::Warning | NoticeLevel::Error => eprintln!("{}", notice.text),
        }
    }
}

fn play(session: &mut Session, search: Option<&str>, shuffle: bool) {
    let ids = search_ids(session.view(), Source::Library, search);
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

/// The synthetic disc `--virtual-disc` reads.
fn virtual_disc(wanted: bool) -> Option<VirtualDrive> {
    wanted.then(|| VirtualDrive::new("Virtual Drive", vec![3, 4, 2, 5]))
}

/// Reads the disc in the drive, or `virtual_disc` instead.
pub(crate) fn read_cd(
    session: &mut Session,
    virtual_disc: Option<VirtualDrive>,
    lookup: bool,
    cover: bool,
) -> Result<(), String> {
    match virtual_disc {
        Some(disc) => {
            session.read_disc(Arc::new(disc));
        }
        None => {
            session.scan_drives();
        }
    }
    wait_for_jobs(session);
    if session.cd().is_none() {
        return Err("there is no audio CD in the drive".into());
    }
    if lookup {
        session.lookup_disc_names().map_err(|e| e.to_string())?;
        wait_for_jobs(session);
    }
    if cover {
        session.fetch_disc_cover().map_err(|e| e.to_string())?;
        wait_for_jobs(session);
    }
    Ok(())
}

fn print_cd(session: &Session) {
    let Some(cd) = session.cd() else { return };
    println!("{} — {}  (disc id {})", cd.info.artist, cd.info.album, cd.disc_id);
    if let Some(release) = &cd.info.release_id {
        let cover = match &cd.cover {
            tunebox_engine::DiscCover::Found { artwork, .. } => {
                format!(", cover found ({} kB)", artwork.data.len().div_ceil(1000))
            }
            tunebox_engine::DiscCover::NoneFound => ", no cover in the Cover Art Archive".to_owned(),
            _ => String::new(),
        };
        println!("MusicBrainz release {release}{cover}");
    }
    for (index, number) in cd.audio_numbers().iter().enumerate() {
        let (title, artist) = cd.info.tracks.get(index).cloned().unwrap_or_default();
        let length = cd.toc.duration(*number).map(format_clock).unwrap_or_default();
        println!("{number:>3}  {title:<40} {artist:<20} {length:>6}");
    }
    print_read_offset(session);
}

fn print_read_offset(session: &Session) {
    let Some((drive, offset)) = session.cd_read_offset() else { return };
    println!("Read offset: {offset:+} samples, for {}.", drive.describe());
}

/// A folder as a drive of unknown size, for setting up or syncing folders the platform does
/// not list.
fn folder_volume(root: &std::path::Path) -> tunebox_engine::Volume {
    tunebox_engine::Volume {
        root: root.to_path_buf(),
        label: String::new(),
        total_bytes: u64::MAX,
        free_bytes: u64::MAX,
        removable: true,
        access: tunebox_engine::Access::Folder,
    }
}

/// Mounted players and MTP players, as the app's source list shows them. MTP players are
/// opened to be read, which takes a moment.
fn refresh_all_devices(session: &mut Session) {
    session.refresh_devices(&tunebox_platform::volumes());
    session.refresh_mtp_devices(&tunebox_engine::find_mtp_players());
}

fn sync(session: &mut Session, root: &std::path::Path, playlists: &[String]) -> Result<(), String> {
    refresh_all_devices(session);
    if session.device(root).is_none() && root.to_string_lossy().starts_with("mtp:") {
        return Err("No MTP player with that id is connected. `tunebox-cli devices` lists them.".into());
    }
    if session.device(root).is_none() {
        // Not a removable volume the platform reports (a folder, for example): sync it anyway.
        session.refresh_devices(&[folder_volume(root)]);
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

/// Reads a media kind: "music" or "audiobook".
fn parse_kind(text: &str) -> Result<MediaKind, String> {
    MediaKind::from_name(text).ok_or_else(|| format!("“{text}” is not a kind: write music or audiobook"))
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
            &["rename", "--search", "x", "--pattern", "{title}"],
            &["rename", "--search", "x", "--pattern", "{title}", "--to", "media"],
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
            &["artwork", "find", "x"],
            &["artwork", "fetch", "x", "--release", "11111111-2222-3333-4444-555555555555"],
            &["cd-import", "--lookup", "--cover"],
            &["rename", "--search", "x", "--pattern", "{title}", "--apply"],
            &["play"],
        ] {
            assert!(!command(changes).reads_only(), "{changes:?}");
        }
    }
}
