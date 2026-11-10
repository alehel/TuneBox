//! `tunebox --demo <folder> --screenshots <out>`: sets up each documented view in turn,
//! captures the window and saves it as a PNG, then quits. The README's pictures come from here.

use crate::app::TuneBoxApp;
use crate::dialogs::{ConfirmRefresh, Dialog, GetInfoDialog, PreferencesDialog, PreferencesTab, SmartPlaylistDialog};
use crate::state::{SourceItem, ViewMode};
use egui::ColorImage;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tunebox_engine::model::{PlaylistKind, TagField, TrackId};
use tunebox_engine::{Source, TrackQuery};

/// Frames to let a scene settle (layout, artwork, the player) before capturing it.
const SETTLE_FRAMES: u32 = 40;

struct Scene {
    name: &'static str,
    setup: fn(&mut TuneBoxApp),
}

const SCENES: [Scene; 20] = [
    Scene { name: "tunebox-library", setup: library },
    Scene { name: "tunebox-albums", setup: albums },
    Scene { name: "tunebox-unplayable", setup: unplayable },
    Scene { name: "tunebox-smart-playlist", setup: smart_playlist },
    Scene { name: "tunebox-smart-editor", setup: smart_editor },
    Scene { name: "tunebox-get-info", setup: get_info },
    Scene { name: "tunebox-get-info-details", setup: get_info_details },
    Scene { name: "tunebox-get-info-several", setup: get_info_several },
    Scene { name: "tunebox-get-info-pinned", setup: get_info_pinned },
    Scene { name: "tunebox-get-info-all-fields", setup: get_info_all_fields },
    Scene { name: "tunebox-refresh-from-files", setup: refresh_from_files },
    Scene { name: "tunebox-cd-import", setup: cd },
    Scene { name: "tunebox-device-sync", setup: device },
    Scene { name: "tunebox-preferences", setup: preferences },
    Scene { name: "tunebox-preferences-importing", setup: preferences_importing },
    Scene { name: "tunebox-preferences-columns", setup: preferences_columns },
    Scene { name: "tunebox-preferences-get-info", setup: preferences_get_info },
    Scene { name: "tunebox-visualizer-pulse", setup: visualizer_pulse },
    Scene { name: "tunebox-visualizer-aurora", setup: visualizer_aurora },
    Scene { name: "tunebox-visualizer-spectrum", setup: visualizer_spectrum },
];

type Shot = Arc<Mutex<Option<Arc<ColorImage>>>>;

pub struct ScreenshotRunner {
    dir: PathBuf,
    index: usize,
    frames: u32,
    requested: bool,
    shot: Shot,
}

impl ScreenshotRunner {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, index: 0, frames: 0, requested: false, shot: Arc::new(Mutex::new(None)) }
    }

    /// Advances one frame. Returns `false` when every scene is saved.
    pub fn step(&mut self, app: &mut TuneBoxApp) -> bool {
        let Some(scene) = SCENES.get(self.index) else { return false };
        let ctx = app.ctx().clone();
        ctx.request_repaint();
        if self.frames == 0 {
            app.dialog = None;
            app.view.browse = Default::default();
            app.view.show_visualizer = false;
            app.visualizer.controls_pinned = false;
            (scene.setup)(app);
        }
        self.frames += 1;
        if !self.requested && self.frames >= SETTLE_FRAMES && app.artwork.is_idle() {
            // Notices come and go; the pictures show the views themselves.
            app.clear_toasts();
            let slot = Arc::clone(&self.shot);
            let repaint = ctx.clone();
            ctx.request_screenshot(move |image| {
                *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(image);
                repaint.request_repaint();
            });
            self.requested = true;
        }
        let taken = self.shot.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(image) = taken {
            let path = self.dir.join(format!("{}.png", scene.name));
            match save_png(&image, &path) {
                Ok(()) => log::info!("saved {}", path.display()),
                Err(e) => log::error!("could not save {}: {e}", path.display()),
            }
            self.index += 1;
            self.frames = 0;
            self.requested = false;
        }
        true
    }
}

fn save_png(image: &ColorImage, path: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let [width, height] = image.size;
    let buffer =
        image::RgbaImage::from_raw(width as u32, height as u32, bytes).ok_or("the screenshot had the wrong size")?;
    buffer.save(path).map_err(|e| e.to_string())
}

// ----- scenes -----

fn find_track(app: &TuneBoxApp, title: &str) -> Option<(Vec<TrackId>, usize)> {
    let rows = app.session.query(&TrackQuery::new(Source::Music));
    let index = rows.iter().position(|id| app.session.track(*id).is_some_and(|t| t.title() == title))?;
    Some((rows, index))
}

fn library(app: &mut TuneBoxApp) {
    app.view.select_source(SourceItem::Music);
    app.view.view_mode = ViewMode::List;
    app.view.browse.genre = Some("Indie Folk".into());
    if let Some((rows, index)) = find_track(app, "Lanterns on the Water") {
        app.session.play_tracks(&rows, index);
        app.session.seek(Duration::from_secs(74));
        if let Some(id) = rows.get(index) {
            app.view.selection.set([*id]);
        }
    }
}

fn albums(app: &mut TuneBoxApp) {
    app.view.browse = Default::default();
    app.view.selection.clear();
    app.view.view_mode = ViewMode::Albums;
}

fn smart_playlist_id(app: &TuneBoxApp, name: &str) -> Option<tunebox_engine::model::PlaylistId> {
    app.session.library().playlists().iter().find(|p| p.name == name).map(|p| p.id)
}

fn smart_playlist(app: &mut TuneBoxApp) {
    app.view.view_mode = ViewMode::List;
    if let Some(id) = smart_playlist_id(app, "Top 25 Most Played") {
        app.view.select_source(SourceItem::Playlist(id));
    }
}

fn smart_editor(app: &mut TuneBoxApp) {
    let Some(id) = smart_playlist_id(app, "Road Trip Mix") else { return };
    app.view.select_source(SourceItem::Playlist(id));
    if let Some(playlist) = app.session.library().playlist(id)
        && let PlaylistKind::Smart(rules) = &playlist.kind
    {
        app.dialog =
            Some(Dialog::SmartPlaylist(Box::new(SmartPlaylistDialog::from_rules(Some(id), &playlist.name, rules))));
    }
}

fn get_info(app: &mut TuneBoxApp) {
    app.view.select_source(SourceItem::Music);
    if let Some((rows, index)) = find_track(app, "Paper Boats")
        && let Some(id) = rows.get(index)
    {
        app.view.selection.set([*id]);
        app.dialog = Some(Dialog::GetInfo(Box::new(GetInfoDialog::new(&app.session, vec![*id]))));
    }
}

fn cd(app: &mut TuneBoxApp) {
    app.view.select_source(SourceItem::Cd);
}

fn device(app: &mut TuneBoxApp) {
    if let Some(root) = app.demo.as_ref().map(|d| d.player_volume().root) {
        app.view.select_source(SourceItem::Device(root));
    }
}

fn show_visualizer(app: &mut TuneBoxApp, look: &str, track_info: bool) {
    let mut prefs = app.session.preferences().clone();
    prefs.visualizer_style = look.to_owned();
    prefs.visualizer_track_info = track_info;
    app.session.set_preferences(prefs);
    app.view.select_source(SourceItem::Music);
    app.view.show_visualizer = true;
}

fn visualizer_pulse(app: &mut TuneBoxApp) {
    show_visualizer(app, "pulse", true);
}

fn visualizer_aurora(app: &mut TuneBoxApp) {
    show_visualizer(app, "aurora", false);
    // Show the controls, as when the pointer has just moved.
    app.visualizer.controls_pinned = true;
}

fn visualizer_spectrum(app: &mut TuneBoxApp) {
    show_visualizer(app, "spectrum", true);
}

/// Preferences with a read-only collection and a writable imports folder inside it (shown, not
/// saved).
fn preferences(app: &mut TuneBoxApp) {
    app.view.select_source(SourceItem::Music);
    let mut prefs = app.session.preferences().clone();
    if let Some(home) = std::env::var_os("HOME") {
        let music = std::path::PathBuf::from(home).join("Music");
        prefs.add_library_folder(music.join("Collection"), true);
        prefs.add_library_folder(music.join("Collection/Imports"), false);
        prefs.add_library_folder(music.join("Vinyl Rips"), true);
        prefs.media_folder = music.join("Collection/Imports");
    }
    app.dialog = Some(Dialog::Preferences(Box::new(PreferencesDialog::new(&prefs))));
}

fn preferences_importing(app: &mut TuneBoxApp) {
    preferences(app);
    let prefs = app.session.preferences().clone();
    app.dialog = Some(Dialog::Preferences(Box::new(PreferencesDialog::on_tab(&prefs, PreferencesTab::Importing))));
}

fn preferences_columns(app: &mut TuneBoxApp) {
    preferences(app);
    let prefs = app.session.preferences().clone();
    app.dialog = Some(Dialog::Preferences(Box::new(PreferencesDialog::on_tab(&prefs, PreferencesTab::Columns))));
}

fn get_info_details(app: &mut TuneBoxApp) {
    get_info(app);
    if let Some(Dialog::GetInfo(dialog)) = app.dialog.take() {
        app.dialog = Some(Dialog::GetInfo(Box::new(dialog.on_details())));
    }
}

/// Get Info on a whole album.
fn get_info_several(app: &mut TuneBoxApp) {
    app.view.select_source(SourceItem::Music);
    let rows = app.session.query(&TrackQuery::new(Source::Music));
    let album: Vec<TrackId> =
        rows.into_iter().filter(|id| app.session.track(*id).is_some_and(|t| t.meta.album == "Paper Boats")).collect();
    app.view.selection.set(album.iter().copied());
    app.dialog = Some(Dialog::GetInfo(Box::new(GetInfoDialog::new(&app.session, album))));
}

/// Get Info with label and release date pinned, scrolled down to "Show all fields". The pins
/// stay for the scenes after it, so Preferences shows them too.
fn get_info_pinned(app: &mut TuneBoxApp) {
    let mut prefs = app.session.preferences().clone();
    prefs.pin_info_field(TagField::Label, true);
    prefs.pin_info_field(TagField::ReleaseDate, true);
    app.session.set_preferences(prefs);
    get_info_with_all_fields(app, false);
}

fn get_info_all_fields(app: &mut TuneBoxApp) {
    get_info_with_all_fields(app, true);
}

fn get_info_with_all_fields(app: &mut TuneBoxApp, open: bool) {
    get_info(app);
    if let Some(Dialog::GetInfo(dialog)) = app.dialog.take() {
        app.dialog = Some(Dialog::GetInfo(Box::new(dialog.on_details().with_all_fields(open))));
    }
}

fn preferences_get_info(app: &mut TuneBoxApp) {
    preferences(app);
    let prefs = app.session.preferences().clone();
    app.dialog = Some(Dialog::Preferences(Box::new(PreferencesDialog::on_tab(&prefs, PreferencesTab::GetInfo))));
}

/// Refresh from Files on an album, two of whose songs have edits kept in TuneBox only.
fn refresh_from_files(app: &mut TuneBoxApp) {
    get_info_several(app);
    let tracks = app.view.selected_tracks();
    app.dialog = Some(Dialog::ConfirmRefresh(ConfirmRefresh { tracks, with_edits: 2, replace_edits: false }));
}

/// The Jazz songs: one whose file is missing, and an album on a share that is not connected.
fn unplayable(app: &mut TuneBoxApp) {
    app.view.select_source(SourceItem::Music);
    app.view.view_mode = ViewMode::List;
    app.view.browse.genre = Some("Jazz".into());
}
