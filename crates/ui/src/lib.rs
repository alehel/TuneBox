//! tunebox-ui — the TuneBox desktop interface.
//!
//! The window is a source list on the left (library, devices, playlists), a toolbar with the
//! transport controls and status display on top, and the selected source in the middle: the
//! track list (with the column browser) or album grid, the CD page, the page of the audiobook
//! being ripped, or a player's page.
//!
//! Everything shown is read from the engine's [`Session`](tunebox_engine::Session); everything
//! the listener does becomes an [`actions::Action`], applied after the frame is drawn.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod actions;
mod app;
mod artwork;
mod book_commands;
mod book_state;
mod demo;
mod demo_web;
mod dialogs;
mod fonts;
mod full_screen;
mod glyphs;
mod library_watch;
mod panels;
mod screenshots;
mod shortcuts;
mod state;
mod theme;
mod visualizer;

use std::path::PathBuf;

/// The visualizer API, for looks written outside TuneBox (see [`run_with`]).
pub use tunebox_visualizer;

/// How the app was started.
#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    /// A folder holding the library, preferences and media (default: the library and
    /// preferences in the platform's application data folder).
    pub library_dir: Option<PathBuf>,
    /// Play without a sound device (the player runs silently in real time).
    pub silent: bool,
    /// A demo folder made by `cargo xtask demo`: its library, a virtual CD and a virtual player.
    pub demo: Option<PathBuf>,
    /// Take the documentation screenshots into this folder, then quit.
    pub screenshots: Option<PathBuf>,
    /// Use OpenGL instead of wgpu.
    pub opengl: bool,
}

/// Opens the main window and runs until it is closed.
pub fn run(options: LaunchOptions) -> Result<(), String> {
    run_with(options, Vec::new())
}

/// Like [`run`], with extra visualizer looks from other crates (compile-time plug-ins). A look
/// with the id of a built-in replaces it.
pub fn run_with(
    options: LaunchOptions,
    extra_looks: Vec<Box<dyn tunebox_visualizer::Visualizer>>,
) -> Result<(), String> {
    let session = app::open_session(&options).inspect_err(|message| show_startup_error(message))?;
    let renderer = if options.opengl { eframe::Renderer::Glow } else { eframe::Renderer::Wgpu };
    let native = eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_title("TuneBox")
            .with_app_id("tunebox")
            .with_inner_size([1280.0, 780.0])
            .with_min_inner_size([860.0, 520.0]),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "TuneBox",
        native,
        Box::new(move |cc| Ok(Box::new(app::TuneBoxApp::new(cc, session, options, extra_looks)))),
    )
    .map_err(|e| e.to_string())
}

/// Says why TuneBox could not start (another TuneBox has the library open, say) in a dialog:
/// launched from a desktop there is no terminal to read the error in.
fn show_startup_error(message: &str) {
    let mut letters = message.chars();
    let sentence: String =
        letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default();
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("TuneBox could not open")
        .set_description(sentence)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
