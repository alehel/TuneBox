//! `cargo xtask screenshots`: regenerates the documentation pictures in `docs/images`.
//!
//! The app runs the demo and saves each scene as a lossless PNG into a staging folder under
//! `target/`; this module then encodes them as lossy WebP for the docs. The PNGs would be
//! hundreds of kilobytes each; a WebP at quality 85 is about an eighth of that, with no visible
//! change to text or artwork. The encoder lives here rather than in the app so the app needs no
//! WebP encoder of its own.

use crate::{cargo, demo};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the docs link the pictures from.
const OUT_DIR: &str = "docs/images";
/// Where the app saves its PNGs before they are encoded.
const STAGING_DIR: &str = "target/screenshots";
/// libwebp's quality scale (0–100). 85 keeps small interface text crisp.
const QUALITY: f32 = 85.0;
/// libwebp's effort (0–6): the slowest, smallest files. Encoding is a one-off, so speed is free.
const METHOD: i32 = 6;

pub fn regenerate(demo_dir: PathBuf) -> Result<(), String> {
    if !demo_dir.join("library").exists() {
        demo::build(&demo_dir)?;
    }
    cargo(&["build", "--release", "--package", "tunebox"])?;
    let staging = PathBuf::from(STAGING_DIR);
    // A fresh folder, so a scene that was removed does not come back from an earlier run.
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|e| format!("could not clear {}: {e}", staging.display()))?;
    }
    take(&demo_dir, &staging)?;
    let out = PathBuf::from(OUT_DIR);
    std::fs::create_dir_all(&out).map_err(|e| format!("could not create {}: {e}", out.display()))?;
    let mut count = 0;
    for png in pngs(&staging)? {
        let Some(name) = png.file_stem() else { continue };
        let target = out.join(name).with_extension("webp");
        encode(&png, &target)?;
        count += 1;
    }
    if count == 0 {
        return Err(format!("the app saved no screenshots in {}", staging.display()));
    }
    eprintln!("{count} screenshots saved in {}", out.display());
    Ok(())
}

/// Runs the app on the demo, saving its PNGs into `staging`.
fn take(demo_dir: &Path, staging: &Path) -> Result<(), String> {
    let app = PathBuf::from("target/release/tunebox");
    let args = ["--demo".into(), demo_dir.display().to_string(), "--screenshots".into(), staging.display().to_string()];
    let headless = cfg!(target_os = "linux")
        && std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none();
    let status = if headless {
        Command::new("xvfb-run")
            .args(["-a", "-s", "-screen 0 1440x900x24", &app.display().to_string(), "--opengl"])
            .args(&args)
            .status()
    } else {
        Command::new(&app).args(&args).status()
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("the app exited with {s}")),
        Err(e) => Err(format!("could not start the app: {e}")),
    }
}

/// The PNGs in `dir`, in name order.
fn pngs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("could not read {}: {e}", dir.display()))?;
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
        .collect();
    found.sort();
    Ok(found)
}

/// Encodes the PNG at `source` as a lossy WebP at `target`.
fn encode(source: &Path, target: &Path) -> Result<(), String> {
    // The screenshots are opaque: dropping alpha saves the encoder an alpha plane.
    let image = image::open(source).map_err(|e| format!("could not read {}: {e}", source.display()))?.into_rgb8();
    let mut config = webp::WebPConfig::new().map_err(|()| "libwebp could not set up its encoder".to_owned())?;
    config.quality = QUALITY;
    config.method = METHOD;
    // Sharper colour edges: keeps coloured text (selected rows, links) from smudging.
    config.use_sharp_yuv = 1;
    let encoded = webp::Encoder::from_rgb(image.as_raw(), image.width(), image.height())
        .encode_advanced(&config)
        .map_err(|e| format!("could not encode {}: {e:?}", source.display()))?;
    std::fs::write(target, &*encoded).map_err(|e| format!("could not write {}: {e}", target.display()))
}
