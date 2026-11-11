//! `tunebox-cli visualizer-plugins`: loads the visualizer plug-ins and runs each for a while on
//! made-up music, the way the visualizer would, reporting what each draws, how long it takes,
//! and any that would be turned off. For plug-in authors and for checking a folder headlessly.

use std::path::Path;
use std::time::{Duration, Instant};
use tunebox_engine::plugins::{FrameData, Plugin, load_folder};

/// The visualizer's size the plug-ins draw at, in points.
const SIZE: (f32, f32) = (1280.0, 720.0);

pub fn check(folder: &Path, frames: u32) -> Result<(), String> {
    println!("Visualizer plug-ins in {}", tunebox_platform::display_path(folder));
    let report = load_folder(folder);
    if report.plugins.is_empty() && report.problems.is_empty() {
        println!("  (none: put .wasm files in this folder)");
    }
    let mut failures = 0;
    for (file, problem) in &report.problems {
        println!("  {file}: not loaded: {problem}.");
        failures += 1;
    }
    for mut plugin in report.plugins {
        let label = format!("{}: “{}” ({})", plugin.file(), plugin.info().name, plugin.info().id);
        match run(&mut plugin, frames) {
            Ok(summary) => println!("  {label}: {summary}"),
            Err(message) => {
                println!("  {label}: {message}");
                failures += 1;
            }
        }
    }
    if failures == 0 { Ok(()) } else { Err(format!("{failures} plug-in(s) cannot be used")) }
}

/// Draws `frames` frames at 60 a second; a summary, or the notice the listener would see.
fn run(plugin: &mut Plugin, frames: u32) -> Result<String, String> {
    let mut shapes = 0usize;
    let mut fuel = 0.0f32;
    let mut slowest = Duration::ZERO;
    let started = Instant::now();
    for n in 0..frames {
        let time = n as f32 / 60.0;
        let bands: Vec<f32> = (0..64).map(|i| 0.5 + 0.45 * (time * 3.0 + i as f32 * 0.2).sin()).collect();
        let wave: Vec<f32> = (0..256).map(|i| (time * 20.0 + i as f32 * 0.1).sin()).collect();
        let waveforms = vec![wave; 6];
        let data = FrameData {
            bands: &bands,
            peaks: &bands,
            level: 0.6,
            bass: 0.5 + 0.5 * (time * 8.0).sin(),
            waveforms: &waveforms,
            time,
            dt: 1.0 / 60.0,
            playing: true,
        };
        let frame_started = Instant::now();
        let drawing = plugin.draw(&data, SIZE.0, SIZE.1).map_err(|fault| fault.notice(&plugin.info().name))?;
        slowest = slowest.max(frame_started.elapsed());
        shapes += drawing.len();
        fuel = fuel.max(plugin.last_fuel_share());
    }
    let count = frames.max(1);
    Ok(format!(
        "works: {} shapes a frame, {:.2} ms a frame on average ({:.2} ms at most), at most {:.0}% of its time budget",
        shapes / count as usize,
        started.elapsed().as_secs_f64() * 1000.0 / f64::from(count),
        slowest.as_secs_f64() * 1000.0,
        fuel * 100.0
    ))
}
