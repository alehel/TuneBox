//! The application: the frame loop, and carrying out what the listener asked for.

use crate::LaunchOptions;
use crate::actions::Action;
use crate::artwork::ArtworkCache;
use crate::demo::Demo;
use crate::dialogs::{
    ConfirmLocate, ConfirmRefresh, ConfirmRemove, Dialog, GetInfoDialog, PreferencesDialog, RenameFilesDialog,
    SmartPlaylistDialog,
};
use crate::library_watch::LibraryWatch;
use crate::panels::toasts::Toasts;
use crate::panels::{
    PanelContext, album_grid, browser, cd_view, device_view, menu_bar, sidebar, status_bar, toolbar, track_list,
};
use crate::screenshots::ScreenshotRunner;
use crate::state::{SourceItem, ViewMode, ViewState};
use crate::visualizer::VisualizerView;
use crate::{shortcuts, theme};
use egui::{Frame, Rect, pos2};
use std::time::{Duration, Instant};
use tunebox_engine::commands::{CommandId, Context};
use tunebox_engine::model::AudioFormat;
use tunebox_engine::model::{PlaylistKind, TrackId};
use tunebox_engine::{EngineError, FileStatus, LibraryOnlyEdits, Notice, Player, Session, Volume};

/// How often mounted volumes are checked for players.
const DEVICE_SCAN: Duration = Duration::from_secs(3);
/// How often drives are checked for a disc.
#[cfg(feature = "drives")]
const DISC_SCAN: Duration = Duration::from_secs(8);

/// Opens the library the options point at.
pub fn open_session(options: &LaunchOptions) -> Result<Session, String> {
    let paths = match (&options.demo, &options.library_dir) {
        (Some(demo), _) => tunebox_platform::app_paths(&demo.join("library")),
        (None, Some(dir)) => tunebox_platform::app_paths(dir),
        (None, None) => tunebox_platform::default_paths(),
    };
    let player = if options.silent || options.demo.is_some() {
        Player::with_output(tunebox_engine::NullOutput::default())
    } else {
        player()
    };
    Session::open(paths, player).map_err(|e| e.to_string())
}

#[cfg(feature = "sound")]
fn player() -> Player {
    Player::new()
}

#[cfg(not(feature = "sound"))]
fn player() -> Player {
    Player::with_output(tunebox_engine::NullOutput::default())
}

pub struct TuneBoxApp {
    pub(crate) session: Session,
    pub(crate) view: ViewState,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) artwork: ArtworkCache,
    pub(crate) visualizer: VisualizerView,
    pub(crate) demo: Option<Demo>,
    ctx: egui::Context,
    toasts: Toasts,
    screenshots: Option<ScreenshotRunner>,
    library_watch: LibraryWatch,
    network_mounts: tunebox_platform::NetworkMounts,
    /// MTP players, found on a thread of their own. None in the demo, which has its own player.
    mtp_players: Option<tunebox_platform::MtpPlayers>,
    last_device_scan: Option<Instant>,
    #[cfg(feature = "drives")]
    last_disc_scan: Option<Instant>,
}

impl TuneBoxApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        mut session: Session,
        options: LaunchOptions,
        extra_looks: Vec<Box<dyn tunebox_visualizer::Visualizer>>,
    ) -> Self {
        theme::install(&cc.egui_ctx);
        let demo = options.demo.as_deref().map(|dir| Demo::start(dir, &mut session));
        let screenshots = options.screenshots.clone().map(ScreenshotRunner::new);
        let mtp_players = demo.is_none().then(tunebox_platform::MtpPlayers::start);
        Self {
            session,
            view: ViewState::new(),
            dialog: None,
            artwork: ArtworkCache::new(),
            visualizer: VisualizerView::new(extra_looks),
            demo,
            ctx: cc.egui_ctx.clone(),
            toasts: Toasts::default(),
            screenshots,
            library_watch: LibraryWatch::default(),
            network_mounts: tunebox_platform::NetworkMounts::start(),
            mtp_players,
            last_device_scan: None,
            #[cfg(feature = "drives")]
            last_disc_scan: None,
        }
    }

    /// Engine bookkeeping that happens every frame.
    fn housekeeping(&mut self) {
        // Looked up on a thread of its own, so a share that stopped answering never stalls this.
        if let Some(mounts) = self.network_mounts.changed() {
            self.session.set_network_mounts(mounts);
        }
        for notice in self.library_watch.update(&mut self.session, &self.ctx) {
            self.toasts.push(notice);
        }
        self.session.tick();
        for notice in self.session.take_notices() {
            self.toasts.push(notice);
        }
        for album in self.session.take_artwork_changes() {
            self.artwork.forget(&album);
        }
        if self.artwork.poll(&self.ctx) {
            self.ctx.request_repaint();
        }
        if self.last_device_scan.is_none_or(|t| t.elapsed() >= DEVICE_SCAN) {
            let mut volumes: Vec<Volume> = tunebox_platform::volumes();
            if let Some(demo) = &self.demo {
                volumes.push(demo.player_volume());
            }
            self.session.refresh_devices(&volumes);
            self.last_device_scan = Some(Instant::now());
        }
        if let Some(found) = self.mtp_players.as_mut().and_then(|players| players.changed()) {
            self.session.refresh_mtp_devices(&found);
        }
        #[cfg(feature = "drives")]
        if self.demo.is_none() && self.last_disc_scan.is_none_or(|t| t.elapsed() >= DISC_SCAN) {
            self.session.scan_drives();
            self.last_disc_scan = Some(Instant::now());
        }
        if let Some(demo) = &mut self.demo {
            demo.name_disc(&mut self.session);
        }
        // A source that went away (an ejected player, a removed disc) falls back to the library.
        let gone = match &self.view.source {
            SourceItem::Cd => self.session.cd().is_none(),
            SourceItem::Device(root) => self.session.device(root).is_none(),
            SourceItem::Playlist(id) => self.session.library().playlist(*id).is_none(),
            SourceItem::Music => false,
        };
        if gone {
            self.view.select_source(SourceItem::Music);
        }
    }

    /// Takes down the notices on screen (for screenshots).
    pub(crate) fn clear_toasts(&mut self) {
        self.toasts = Toasts::default();
    }

    pub(crate) fn ctx(&self) -> &egui::Context {
        &self.ctx
    }

    /// Asks where a missing song's file is now, starting near where it was, and links the song.
    fn locate(&mut self, id: TrackId) {
        let Some(track) = self.session.track(id) else { return };
        let mut picker = rfd::FileDialog::new()
            .set_title(format!("Locate “{}”", track.title()))
            .add_filter("Audio", &AudioFormat::EXTENSIONS);
        // Only for a missing song is its place known to answer; a disconnected share is not
        // touched, since looking there could hang.
        if *self.session.file_status(id) == FileStatus::Missing
            && let Some(folder) = track.location.ancestors().skip(1).find(|a| a.is_dir())
        {
            picker = picker.set_directory(folder);
        }
        if let Some(file) = picker.pick_file() {
            let result = self.session.locate(id, file);
            self.report(result);
        }
    }

    /// Asks for a picture and makes it the cover of `tracks`. The engine does the work, in the
    /// background; the covers are shown anew when it is done.
    fn choose_artwork(&mut self, tracks: Vec<TrackId>) {
        if tracks.is_empty() {
            return;
        }
        let picked = rfd::FileDialog::new()
            .set_title("Choose Artwork")
            .add_filter("Images", &["jpg", "jpeg", "png", "gif", "bmp", "webp"])
            .pick_file();
        if let Some(image) = picked {
            self.apply(Action::SetArtwork(tracks, image));
        }
    }

    fn command_context(&self) -> Context {
        let selected = self.view.selected_tracks();
        Context {
            selected_tracks: self.view.selection.len(),
            selected_missing: selected
                .iter()
                .filter(|id| *self.session.file_status(**id) == FileStatus::Missing)
                .count(),
            device_selected: matches!(&self.view.source, SourceItem::Device(root) if self.session.device(root).is_some_and(|d| d.device.is_some())),
            cd_selected: self.view.source == SourceItem::Cd,
        }
    }

    fn report(&mut self, result: Result<impl Sized, EngineError>) {
        if let Err(e) = result {
            self.toasts.push(Notice::error(e.to_string()));
        }
    }

    // ----- applying actions -----

    fn apply(&mut self, action: Action) {
        match action {
            Action::Run(id) => self.run(id),
            Action::SelectSource(source) => self.view.select_source(source),
            Action::PlayTracks { tracks, start } => {
                // A missing song cannot play, but it can be found: offer that instead.
                let chosen =
                    tracks.get(start).copied().filter(|id| *self.session.file_status(*id) == FileStatus::Missing);
                match chosen.and_then(|id| self.session.track(id)) {
                    Some(track) => {
                        let dialog = ConfirmLocate { track: track.id, title: track.title().to_owned() };
                        self.dialog = Some(Dialog::ConfirmLocate(dialog));
                    }
                    None => self.session.play_tracks(&tracks, start),
                }
            }
            Action::Locate(id) => self.locate(id),
            Action::ChooseArtwork(tracks) => self.choose_artwork(tracks),
            Action::SetArtwork(tracks, image) => {
                let result = self.session.set_artwork(&tracks, &image);
                self.report(result);
            }
            Action::RemoveArtwork(tracks) => {
                let result = self.session.remove_artwork(&tracks);
                self.report(result);
            }
            Action::RenameFiles(dialog) => self.rename_files(*dialog),
            Action::SetRating(ids, rating) => self.session.set_rating(&ids, rating),
            Action::SetChecked(ids, checked) => self.session.set_checked(&ids, checked),
            Action::SortBy(column) => self.view.sort_by(column),
            Action::Seek(position) => self.session.seek(position),
            Action::SetVolume(volume) => self.session.set_volume(volume),
            Action::AddToPlaylist(id, tracks) => {
                let result = self.session.add_to_playlist(id, &tracks);
                self.report(result);
            }
            Action::NewPlaylistWith(tracks) => self.new_playlist(&tracks),
            Action::RenamePlaylist(id, name) => {
                let result = self.session.rename_playlist(id, &name);
                self.report(result);
            }
            Action::DeletePlaylist(id) => {
                let result = self.session.delete_playlist(id);
                self.report(result);
            }
            Action::EditSmartPlaylist(id) => self.edit_smart_playlist(id),
            Action::RegisterDevice { root, name } => {
                let result = self.session.register_device(&root, &name);
                self.report(result);
            }
            Action::SetDeviceSettings { root, settings } => {
                let result = self.session.set_device_settings(&root, settings);
                self.report(result);
            }
            Action::SyncDevice(root) => {
                let result = self.session.sync_device(&root);
                self.report(result);
            }
            Action::SetCdTrackIncluded(index, include) => self.session.set_cd_track_included(index, include),
            Action::SetDiscInfo(info) => self.session.set_disc_info(*info),
            Action::OpenDialog(dialog) => self.dialog = Some(*dialog),
            Action::CancelJob(id) => self.session.cancel_job(id),
            Action::ShowInFolder(id) => self.show_in_folder(id),
            Action::EditTracks(edits) => {
                for (id, meta) in edits {
                    let result = self.session.edit_track(id, meta);
                    self.report(result);
                }
                self.artwork.clear();
            }
            Action::RemoveTracks { tracks, delete_files } => {
                self.session.remove_tracks(&tracks, delete_files);
                self.view.selection.clear();
            }
            Action::RefreshFromFiles(tracks, edits) => {
                let result = self.session.refresh_from_files(&tracks, edits);
                self.report(result);
                // Covers are read from the files when shown, so forgetting them is the refresh.
                self.artwork.clear();
            }
            Action::SetPreferences(prefs) => self.session.set_preferences(*prefs),
            Action::AddToLibrary(paths) => {
                let result = self.session.import(paths);
                self.report(result);
            }
            Action::SaveSmartPlaylist { id, name, rules } => self.save_smart_playlist(id, &name, rules),
        }
    }

    /// Carries out a registry command.
    fn run(&mut self, id: CommandId) {
        if !self.session.is_enabled(id, self.command_context()) {
            return;
        }
        match id {
            CommandId::NewPlaylist => self.new_playlist(&[]),
            CommandId::NewPlaylistFromSelection => self.new_playlist(&self.view.selected_tracks()),
            CommandId::NewSmartPlaylist => {
                self.dialog = Some(Dialog::SmartPlaylist(Box::new(SmartPlaylistDialog::new_rules())))
            }
            CommandId::AddToLibrary => self.add_to_library(),
            CommandId::ScanLibraryFolders => {
                let result = self.session.scan_library_folders();
                self.report(result);
            }
            CommandId::Locate => {
                if let [id] = self.view.selected_tracks()[..] {
                    self.locate(id);
                }
            }
            CommandId::RefreshFromFiles => {
                let tracks = self.view.selected_tracks();
                match self.session.library_only_edits(&tracks) {
                    0 => self.apply(Action::RefreshFromFiles(tracks, LibraryOnlyEdits::Keep)),
                    with_edits => {
                        let dialog = ConfirmRefresh { tracks, with_edits, replace_edits: false };
                        self.dialog = Some(Dialog::ConfirmRefresh(dialog));
                    }
                }
            }
            CommandId::ImportCd => {
                let result = self.session.import_cd();
                self.report(result);
            }
            CommandId::GetTrackNames => self.get_track_names(),
            CommandId::SyncDevice => {
                if let SourceItem::Device(root) = &self.view.source {
                    let result = self.session.sync_device(&root.clone());
                    self.report(result);
                }
            }
            CommandId::Eject => self.eject(),
            CommandId::ShowInFolder => {
                if let Some(&id) = self.view.selected_tracks().first() {
                    self.show_in_folder(id);
                }
            }
            CommandId::SelectAll => {
                let rows = self.view.rows().to_vec();
                self.view.selection.select_all(&rows);
            }
            CommandId::Delete => self.delete_selection(),
            CommandId::ChooseArtwork => self.choose_artwork(self.view.selected_tracks()),
            CommandId::RemoveArtwork => self.apply(Action::RemoveArtwork(self.view.selected_tracks())),
            CommandId::GetInfo => {
                let tracks = self.view.selected_tracks();
                self.dialog = Some(Dialog::GetInfo(Box::new(GetInfoDialog::new(&self.session, tracks))));
            }
            CommandId::Preferences => {
                self.dialog = Some(Dialog::Preferences(Box::new(PreferencesDialog::new(
                    self.session.preferences(),
                    self.session.cd_read_offset().map(|(drive, _)| drive),
                ))))
            }
            CommandId::Find => self.view.focus_search = true,
            CommandId::ShowBrowser => {
                let mut prefs = self.session.preferences().clone();
                prefs.show_browser = !prefs.show_browser;
                self.session.set_preferences(prefs);
            }
            CommandId::AsList => {
                self.view.view_mode = ViewMode::List;
                self.view.show_visualizer = false;
            }
            CommandId::AsAlbums => {
                self.view.view_mode = ViewMode::Albums;
                self.view.show_visualizer = false;
            }
            CommandId::Visualizer => self.view.show_visualizer = !self.view.show_visualizer,
            CommandId::PlayPause => self.play_pause(),
            CommandId::NextTrack => self.session.next_track(),
            CommandId::PreviousTrack => self.session.previous_track(),
            CommandId::VolumeUp => self.session.set_volume(self.session.volume() + 0.1),
            CommandId::VolumeDown => self.session.set_volume(self.session.volume() - 0.1),
            CommandId::Shuffle => self.session.set_shuffle(!self.session.shuffle()),
            CommandId::Repeat => self.session.cycle_repeat(),
            CommandId::RenameFiles => {
                let dialog = RenameFilesDialog::new(self.view.selected_tracks());
                self.dialog = Some(Dialog::RenameFiles(Box::new(dialog)));
            }
            CommandId::About => self.dialog = Some(Dialog::About),
            CommandId::Quit => self.ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    /// Renames as the dialog's preview showed. If the engine refuses (the files changed since),
    /// the dialog comes back with a fresh preview, so the listener sees why.
    fn rename_files(&mut self, dialog: RenameFilesDialog) {
        let Some((plan, conflicts)) = dialog.confirmed() else { return };
        if let Err(e) = self.session.rename_files(plan, conflicts) {
            self.toasts.push(Notice::error(e.to_string()));
            self.dialog = Some(Dialog::RenameFiles(Box::new(dialog.reopened())));
        }
    }

    /// Play/Pause with nothing loaded plays the selection (or the whole list) from the top.
    fn play_pause(&mut self) {
        if self.session.play_pause() {
            return;
        }
        let rows = self.view.rows().to_vec();
        let start = self.view.selection.positions(&rows).first().copied().unwrap_or(0);
        self.session.play_tracks(&rows, start);
    }

    fn new_playlist(&mut self, tracks: &[TrackId]) {
        let taken: Vec<String> = self.session.library().playlists().iter().map(|p| p.name.clone()).collect();
        let name = (1..)
            .map(|n| if n == 1 { "untitled playlist".to_owned() } else { format!("untitled playlist {n}") })
            .find(|n| !taken.contains(n));
        let Some(name) = name else { return };
        match self.session.create_playlist(&name, tracks) {
            Ok(id) => {
                self.view.select_source(SourceItem::Playlist(id));
                self.dialog = Some(Dialog::rename_playlist(id, &name));
            }
            Err(e) => self.toasts.push(Notice::error(e.to_string())),
        }
    }

    fn edit_smart_playlist(&mut self, id: tunebox_engine::model::PlaylistId) {
        if let Some(playlist) = self.session.library().playlist(id)
            && let PlaylistKind::Smart(rules) = &playlist.kind
        {
            self.dialog =
                Some(Dialog::SmartPlaylist(Box::new(SmartPlaylistDialog::from_rules(Some(id), &playlist.name, rules))));
        }
    }

    fn save_smart_playlist(
        &mut self,
        id: Option<tunebox_engine::model::PlaylistId>,
        name: &str,
        rules: tunebox_engine::model::SmartRules,
    ) {
        let result = match id {
            Some(id) => self
                .session
                .set_smart_rules(id, rules)
                .and_then(|()| self.session.rename_playlist(id, name))
                .map(|()| id),
            None => self.session.create_smart_playlist(name, rules),
        };
        match result {
            Ok(id) => self.view.select_source(SourceItem::Playlist(id)),
            Err(e) => self.toasts.push(Notice::error(e.to_string())),
        }
    }

    fn add_to_library(&mut self) {
        let Some(folders) = rfd::FileDialog::new().set_title("Add to Library").pick_folders() else { return };
        let result = self.session.import(folders);
        self.report(result);
    }

    #[cfg(feature = "musicbrainz")]
    fn get_track_names(&mut self) {
        let result = self.session.lookup_disc_names();
        self.report(result);
    }

    #[cfg(not(feature = "musicbrainz"))]
    fn get_track_names(&mut self) {}

    fn eject(&mut self) {
        match self.view.source.clone() {
            SourceItem::Cd => {
                if self.demo.is_none() {
                    tunebox_platform::eject_disc();
                }
                self.session.eject_disc();
            }
            SourceItem::Device(root) => {
                // TuneBox lets go of an MTP player after every sync: there is nothing to unmount.
                let is_mtp = self.session.device(&root).is_some_and(|d| d.volume.is_mtp());
                if is_mtp {
                    self.toasts.push(Notice::info("You can unplug the player: TuneBox is not using it."));
                    return;
                }
                let ejected = if self.demo.is_some() { Ok(()) } else { tunebox_platform::eject(&root) };
                match ejected {
                    Ok(()) => self.session.forget_device(&root),
                    Err(e) => self.toasts.push(Notice::error(format!("The player could not be ejected: {e}"))),
                }
            }
            _ => return,
        }
        self.view.select_source(SourceItem::Music);
    }

    fn show_in_folder(&mut self, id: TrackId) {
        if let Some(track) = self.session.track(id)
            && let Err(e) = tunebox_platform::reveal(&track.location)
        {
            self.toasts.push(Notice::error(format!("Could not show the file: {e}")));
        }
    }

    /// Delete: out of the playlist when one is shown, else out of the library (after asking).
    fn delete_selection(&mut self) {
        let rows = self.view.rows().to_vec();
        match self.view.source.clone() {
            SourceItem::Playlist(id) => {
                let manual = self.session.library().playlist(id).is_some_and(|p| !p.is_smart());
                if manual && self.view.sort.is_none() && self.view.search.is_empty() && self.view.browse.is_empty() {
                    let positions = self.view.selection.positions(&rows);
                    let result = self.session.remove_from_playlist(id, &positions);
                    self.report(result);
                    self.view.selection.clear();
                } else if manual {
                    self.toasts.push(Notice::info("Clear the search and sorting to remove songs from the playlist."));
                } else {
                    self.toasts.push(Notice::info("Smart playlists choose their own songs. Edit its rules instead."));
                }
            }
            SourceItem::Music => {
                let tracks = self.view.selected_tracks();
                self.dialog = Some(Dialog::ConfirmRemove(ConfirmRemove { tracks, delete_files: false }));
            }
            SourceItem::Device(_) | SourceItem::Cd => {}
        }
    }
}

impl eframe::App for TuneBoxApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.housekeeping();
        self.view.refresh(&self.session);
        let mut actions = Vec::new();
        if self.dialog.is_none()
            && let Some(id) = shortcuts::pressed(&self.ctx)
        {
            actions.push(Action::Run(id));
        }
        let context = self.command_context();
        {
            let mut cx = PanelContext {
                session: &self.session,
                view: &mut self.view,
                artwork: &mut self.artwork,
                actions: &mut actions,
            };
            egui::Panel::top("menu-bar")
                .frame(Frame::new().fill(theme::BAR_TOP).inner_margin(egui::Margin::symmetric(6, 2)))
                .show(ui, |ui| {
                    menu_bar::show(ui, &mut cx, context);
                });
            egui::Panel::top("toolbar")
                .exact_size(toolbar::HEIGHT)
                .frame(Frame::NONE)
                .show(ui, |ui| toolbar::show(ui, &mut cx));
            egui::Panel::bottom("status-bar")
                .exact_size(status_bar::HEIGHT)
                .frame(Frame::NONE)
                .show(ui, |ui| status_bar::show(ui, &mut cx));
            egui::Panel::left("source-list")
                .resizable(true)
                .default_size(208.0)
                .size_range(160.0..=340.0)
                .frame(Frame::NONE)
                .show(ui, |ui| {
                    sidebar::show(ui, &mut cx);
                });
            let visualizer = &mut self.visualizer;
            egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| central(ui, &mut cx, visualizer));
        }
        if let Some(dialog) = &mut self.dialog
            && !dialog.show(&self.ctx, &self.session, &mut self.artwork, &mut actions)
        {
            self.dialog = None;
        }
        self.toasts.show(&self.ctx);
        for action in actions {
            self.apply(action);
        }
        if let Some(mut runner) = self.screenshots.take() {
            if runner.step(self) {
                self.screenshots = Some(runner);
            } else {
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        // Keep the status display moving while something plays or works.
        if self.session.now_playing().is_some() || !self.session.jobs().is_empty() {
            self.ctx.request_repaint_after(Duration::from_millis(200));
        } else {
            self.ctx.request_repaint_after(Duration::from_secs(1));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Err(e) = self.session.shutdown() {
            log::error!("could not save the library on exit: {e}");
        }
    }
}

/// The middle of the window: the visualizer, or whatever the selected source shows.
fn central(ui: &mut egui::Ui, cx: &mut PanelContext, visualizer: &mut VisualizerView) {
    if cx.view.show_visualizer {
        visualizer.show(ui, cx);
        return;
    }
    let rect = ui.max_rect();
    match cx.view.source.clone() {
        SourceItem::Cd => cd_view::show(ui, cx),
        SourceItem::Device(root) => device_view::show(ui, cx, &root),
        SourceItem::Music | SourceItem::Playlist(_) => {
            let show_browser = cx.session.preferences().show_browser && cx.view.view_mode == ViewMode::List;
            let browser_height = if show_browser { browser::HEIGHT } else { 0.0 };
            if show_browser {
                let area = Rect::from_min_size(rect.min, egui::vec2(rect.width(), browser_height));
                ui.scope_builder(egui::UiBuilder::new().max_rect(area).id_salt("browser"), |ui| browser::show(ui, cx));
            }
            let list = Rect::from_min_max(pos2(rect.left(), rect.top() + browser_height), rect.max);
            ui.scope_builder(egui::UiBuilder::new().max_rect(list).id_salt("tracks"), |ui| match cx.view.view_mode {
                ViewMode::List => track_list::show(ui, cx),
                ViewMode::Albums => album_grid::show(ui, cx),
            });
        }
    }
}
