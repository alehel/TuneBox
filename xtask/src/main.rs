//! `cargo xtask <task>`: TuneBox's build tooling.
//!
//! - `ci`: every quality gate (format, Clippy, tests, layering, dependency licences).
//! - `layers`: checks that crates only depend on lower layers.
//! - `demo [DIR]`: builds a demo library, virtual player and artwork from scratch.
//! - `screenshots`: regenerates the README screenshots from the demo.
//! - `plugins [--install]`: builds the example visualizer plug-ins to WebAssembly.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

mod covers;
mod demo;
mod layers;
mod plugins;
mod screenshots;

#[derive(Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Format check, Clippy with warnings as errors, tests, the layering check and the example
    /// plug-ins' WebAssembly build.
    Ci,
    /// Check the crate layering.
    Layers,
    /// Build the demo library (default: ~/Music/TuneBox Demo).
    Demo { dir: Option<PathBuf> },
    /// Regenerate docs/images from the demo (uses Xvfb when there is no display).
    Screenshots {
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Build the example visualizer plug-ins (needs `rustup target add wasm32-unknown-unknown`).
    Plugins {
        /// Also copy them into TuneBox's Visualizers folder.
        #[arg(long)]
        install: bool,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().task {
        Task::Ci => ci(),
        Task::Layers => layers::check(),
        Task::Demo { dir } => demo::build(&dir.unwrap_or_else(demo::default_dir)),
        Task::Screenshots { dir } => screenshots::regenerate(dir.unwrap_or_else(demo::default_dir)),
        Task::Plugins { install } => plugins::build(install),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

pub(crate) fn cargo(args: &[&str]) -> Result<(), String> {
    eprintln!("$ cargo {}", args.join(" "));
    let status = Command::new(env!("CARGO")).args(args).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("`cargo {}` failed", args.join(" "))) }
}

fn ci() -> Result<(), String> {
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])?;
    cargo(&["test", "--workspace"])?;
    layers::check()?;
    // The example plug-ins must still build for WebAssembly. Skipped, with a note, when the
    // target is not installed (the tests themselves never need it).
    if plugins::target_installed() {
        plugins::build(false)?;
    } else {
        eprintln!("note: the wasm32-unknown-unknown target is not installed; the example plug-ins were not built");
    }
    // Dependency licences and sources (deny.toml). Skipped, with a note, when cargo-deny is not
    // installed: `cargo install cargo-deny`.
    match Command::new(env!("CARGO")).args(["deny", "--version"]).output() {
        Ok(out) if out.status.success() => cargo(&["deny", "check"]),
        _ => {
            eprintln!("note: cargo-deny is not installed; dependency licences were not checked");
            Ok(())
        }
    }
}
