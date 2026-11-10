//! tunebox-platform — the operating system, behind small functions.
//!
//! Where TuneBox keeps its files ([`app_paths`]), which volumes are mounted ([`volumes`]),
//! showing a file in the file manager ([`reveal`]) and ejecting a player or disc ([`eject`]).
//! Nothing here holds state; the engine is told the results.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::path::{Path, PathBuf};
use std::process::Command;
use tunebox_engine::{SessionPaths, Volume};

/// The folder TuneBox keeps its library in: `~/Music/TuneBox`, like a music app's own folder
/// next to the music. Falls back to the home folder, then the current folder.
pub fn tunebox_folder() -> PathBuf {
    dirs::audio_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Music")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuneBox")
}

/// The session's files inside `folder` (normally [`tunebox_folder`]).
pub fn app_paths(folder: &Path) -> SessionPaths {
    SessionPaths {
        library_file: folder.join("TuneBox Library.json"),
        preferences_file: folder.join("Preferences.json"),
        default_media_folder: folder.join("TuneBox Media"),
    }
}

/// The mounted volumes, excluding system and virtual file systems.
pub fn volumes() -> Vec<Volume> {
    let disks = match sysinfo::Disks::new_with_refreshed_list() {
        Ok(disks) => disks,
        Err(e) => {
            log::warn!("could not list mounted volumes: {e}");
            return Vec::new();
        }
    };
    disks
        .iter()
        .filter(|disk| is_user_volume(disk.mount_point()))
        .map(|disk| Volume {
            root: disk.mount_point().to_path_buf(),
            label: volume_label(disk.name().to_string_lossy().as_ref(), disk.mount_point()),
            total_bytes: disk.total_space(),
            free_bytes: disk.available_space(),
            removable: disk.is_removable() || is_removable_mount(disk.mount_point()),
        })
        .collect()
}

/// Mount points a person could plug a player into. System volumes are left out.
fn is_user_volume(mount: &Path) -> bool {
    if cfg!(target_os = "macos") {
        mount.starts_with("/Volumes")
    } else if cfg!(windows) {
        true
    } else {
        ["/media", "/run/media", "/mnt"].iter().any(|prefix| mount.starts_with(prefix))
    }
}

/// On Linux and the BSDs desktop environments mount removable media under these folders.
fn is_removable_mount(mount: &Path) -> bool {
    ["/media", "/run/media", "/Volumes"].iter().any(|prefix| mount.starts_with(prefix))
}

/// The volume's label, or the last part of its mount point.
fn volume_label(name: &str, mount: &Path) -> String {
    let from_mount = mount.file_name().map(|n| n.to_string_lossy().into_owned());
    if name.is_empty() || name.starts_with("/dev/") {
        from_mount.unwrap_or_else(|| name.to_owned())
    } else {
        name.to_owned()
    }
}

/// A path for display, with the home folder shortened to `~`.
pub fn display_path(path: &Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => path.display().to_string(),
    }
}

/// Shows `path` in the system file manager.
pub fn reveal(path: &Path) -> Result<(), String> {
    let status = if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(path).status()
    } else if cfg!(windows) {
        Command::new("explorer").arg(format!("/select,{}", path.display())).status()
    } else {
        Command::new("xdg-open").arg(path.parent().unwrap_or(path)).status()
    };
    status
        .map_err(|e| e.to_string())
        .and_then(|s| if s.success() || cfg!(windows) { Ok(()) } else { Err(format!("the file manager reported {s}")) })
}

/// Unmounts and ejects the volume mounted at `root`, so it can be unplugged safely.
pub fn eject(root: &Path) -> Result<(), String> {
    let output = if cfg!(target_os = "macos") {
        Command::new("diskutil").arg("eject").arg(root).output()
    } else if cfg!(windows) {
        return Err("Use “Safely Remove Hardware” in the taskbar to eject the player.".into());
    } else {
        Command::new("gio").arg("mount").arg("--eject").arg(root).output()
    };
    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()),
        Err(e) => Err(e.to_string()),
    }
}

/// Opens the system's CD tray, best effort.
pub fn eject_disc() {
    let result = if cfg!(target_os = "macos") {
        Command::new("drutil").arg("tray").arg("eject").status()
    } else if cfg!(windows) {
        return;
    } else {
        Command::new("eject").arg("/dev/cdrom").status()
    };
    if let Err(e) = result {
        log::info!("could not open the disc tray: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_live_in_the_folder() {
        let paths = app_paths(Path::new("/x/TuneBox"));
        assert_eq!(paths.library_file, Path::new("/x/TuneBox/TuneBox Library.json"));
        assert_eq!(paths.default_media_folder, Path::new("/x/TuneBox/TuneBox Media"));
    }

    #[test]
    fn labels_fall_back_to_the_mount_point() {
        assert_eq!(volume_label("/dev/sdb1", Path::new("/media/me/SANSA")), "SANSA");
        assert_eq!(volume_label("FIIO", Path::new("/media/me/x")), "FIIO");
    }

    #[test]
    fn listing_volumes_does_not_fail() {
        let _ = volumes();
    }
}
