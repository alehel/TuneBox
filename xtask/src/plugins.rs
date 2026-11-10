//! `cargo xtask plugins [--install]`: builds the example visualizer plug-ins in `plugins/` to
//! WebAssembly, into `target/visualizer-plugins/<name>.wasm`, and with `--install` copies them
//! into TuneBox's Visualizers folder.

use std::path::{Path, PathBuf};
use std::process::Command;

const TARGET: &str = "wasm32-unknown-unknown";
/// Where the built plug-ins go.
const OUT: &str = "target/visualizer-plugins";

/// Whether the WebAssembly target is installed (`rustup target add wasm32-unknown-unknown`).
pub fn target_installed() -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).lines().any(|line| line.trim() == TARGET))
}

/// The plug-in crates: `(package name, short name)`, from the folders in `plugins/`.
fn crates() -> Result<Vec<(String, String)>, String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir("plugins").map_err(|e| format!("plugins/: {e}"))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let Some(short) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else { continue };
        if path.join("Cargo.toml").exists() {
            found.push((format!("tunebox-plugin-{short}"), short));
        }
    }
    found.sort();
    Ok(found)
}

pub fn build(install: bool) -> Result<(), String> {
    if !target_installed() {
        return Err(format!("the {TARGET} target is not installed: `rustup target add {TARGET}`"));
    }
    let crates = crates()?;
    let mut args = vec!["build", "--release", "--target", TARGET];
    for (package, _) in &crates {
        args.extend(["--package", package.as_str()]);
    }
    crate::cargo(&args)?;
    std::fs::create_dir_all(OUT).map_err(|e| format!("{OUT}: {e}"))?;
    let destination = install.then(|| tunebox_platform::default_paths().visualizer_plugins_folder);
    if let Some(folder) = &destination {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    for (package, short) in &crates {
        let built = PathBuf::from(format!("target/{TARGET}/release/{}.wasm", package.replace('-', "_")));
        let out = Path::new(OUT).join(format!("{short}.wasm"));
        copy(&built, &out)?;
        if let Some(folder) = &destination {
            copy(&built, &folder.join(format!("{short}.wasm")))?;
        }
    }
    eprintln!("plug-ins built in {OUT}");
    if let Some(folder) = destination {
        eprintln!("and installed in {} (choose Reload Visualizer Plug-ins in TuneBox)", folder.display());
    }
    Ok(())
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to).map(|_| ()).map_err(|e| format!("copying {} to {}: {e}", from.display(), to.display()))
}
