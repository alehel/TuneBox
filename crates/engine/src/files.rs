//! The one place that decides whether TuneBox may write to a path.
//!
//! TuneBox changes files a listener already has in only two ways: writing edited details into
//! a song (Get Info), and deleting a song's file when asked. It creates files only in the media
//! folder (imported copies, CD rips) and on players it syncs. With a protected music folder
//! (Preferences ▸ General) none of that may touch the folder: edits stay in the library, deletes
//! only remove the song from the library, and copies and rips must go elsewhere.

use crate::prefs::Preferences;
use std::path::{Path, PathBuf};

/// The folder TuneBox must not write to, if the listener protected one.
pub(crate) fn protected_folder(prefs: &Preferences) -> Option<&Path> {
    prefs.music_folder.as_deref().filter(|_| prefs.music_folder_read_only)
}

/// Whether TuneBox may create, change or delete `path`.
pub(crate) fn may_write(prefs: &Preferences, path: &Path) -> bool {
    protected_folder(prefs).is_none_or(|folder| !is_inside(path, folder))
}

/// Whether `path` is `folder` or lies inside it. Both are resolved through symbolic links where
/// they exist, so a link into the protected folder is still protected.
pub(crate) fn is_inside(path: &Path, folder: &Path) -> bool {
    let folder = resolve(folder);
    resolve(path).starts_with(&folder) || path.starts_with(&folder)
}

/// `path` with links resolved: the path itself if it exists, else its nearest existing parent
/// with the rest appended (a media folder that does not exist yet still resolves correctly).
fn resolve(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        if let Ok(real) = current.canonicalize() {
            return missing.iter().rev().fold(real, |p, part| p.join(part));
        }
        match (current.parent(), current.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name.to_os_string());
                current = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_protected_folder_is_off_limits() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("Music");
        std::fs::create_dir_all(music.join("Album")).unwrap();
        let mut prefs = Preferences::with_media_folder(dir.path().join("TuneBox Media"));
        prefs.music_folder = Some(music.clone());
        assert!(!may_write(&prefs, &music.join("Album/song.flac")));
        assert!(!may_write(&prefs, &music.join("New Folder/not yet there.flac")));
        assert!(may_write(&prefs, &dir.path().join("TuneBox Media/x.flac")));
        assert!(may_write(&prefs, &dir.path().join("Musicals/x.flac")), "a sibling with a similar name is fine");
        prefs.music_folder_read_only = false;
        assert!(may_write(&prefs, &music.join("Album/song.flac")));
    }

    #[cfg(unix)]
    #[test]
    fn links_into_the_folder_are_protected_too() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("Music");
        std::fs::create_dir_all(&music).unwrap();
        std::os::unix::fs::symlink(&music, dir.path().join("Link")).unwrap();
        let mut prefs = Preferences::with_media_folder(dir.path().join("Media"));
        prefs.music_folder = Some(music);
        assert!(!may_write(&prefs, &dir.path().join("Link/song.flac")));
    }
}
