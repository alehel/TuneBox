//! The TuneBox desktop app.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
// No console window behind the app on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "tunebox", version, about = "Your music library, CD importing and player syncing")]
struct Args {
    /// A folder holding the library, preferences and media, instead of the usual places.
    #[arg(long)]
    library_dir: Option<PathBuf>,
    /// Play without a sound device.
    #[arg(long)]
    silent: bool,
    /// Draw with OpenGL instead of wgpu (for older graphics drivers).
    #[arg(long)]
    opengl: bool,
    /// Open a demo folder made by `cargo xtask demo` (library, virtual CD and player).
    #[arg(long, hide = true)]
    demo: Option<PathBuf>,
    /// Save the documentation screenshots to this folder and quit (needs --demo).
    #[arg(long, hide = true, requires = "demo")]
    screenshots: Option<PathBuf>,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let options = tunebox_ui::LaunchOptions {
        library_dir: args.library_dir,
        silent: args.silent,
        demo: args.demo,
        screenshots: args.screenshots,
        opengl: args.opengl,
    };
    match tunebox_ui::run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("tunebox: {message}");
            ExitCode::FAILURE
        }
    }
}
