//! tunebox-ui — the TuneBox desktop interface.
//!
//! The window is a source list on the left (library, devices, playlists), a toolbar with the
//! transport controls and status display on top, and the selected source in the middle: the
//! track list (with the column browser) or album grid, the CD page or a player's page.
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
mod demo;
mod dialogs;
mod glyphs;
mod panels;
mod screenshots;
mod shortcuts;
mod state;
mod theme;

use std::path::PathBuf;

/// How the app was started.
#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    /// The folder holding the library (default: the platform's music folder).
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
    let session = app::open_session(&options)?;
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
    eframe::run_native("TuneBox", native, Box::new(move |cc| Ok(Box::new(app::TuneBoxApp::new(cc, session, options)))))
        .map_err(|e| e.to_string())
}
