//! The one place that decides whether TuneBox may write to a path.
//!
//! TuneBox changes files a listener already has in only two ways: writing edited details into
//! a song (Get Info), and deleting a song's file when asked. It creates files only in the media
//! folder (imported copies, CD rips) and on players it syncs.
//!
//! The listener lists their library folders in Preferences, each read-only or writable. A path
//! is governed by the deepest listed folder that contains it, so a writable `~/Music/Imports`
//! can sit inside a read-only `~/Music`. In a read-only folder nothing is created, changed or
//! deleted: edits stay in the library, deletes only remove songs from the library, and copies
//! and rips must go elsewhere. Paths in no listed folder are writable, as before.

use crate::prefs::{LibraryFolder, Preferences};
use std::path::{Path, PathBuf};

/// The library folder whose rule applies to `path`: the deepest one containing it.
pub fn governing_folder<'a>(prefs: &'a Preferences, path: &Path) -> Option<&'a LibraryFolder> {
    let path = resolve(path);
    prefs
        .library_folders
        .iter()
        .map(|folder| (folder, resolve(&folder.path)))
        .filter(|(_, root)| path.starts_with(root))
        .max_by_key(|(_, root)| root.components().count())
        .map(|(folder, _)| folder)
}

/// Whether TuneBox may create, change or delete `path`.
pub fn may_write(prefs: &Preferences, path: &Path) -> bool {
    governing_folder(prefs, path).is_none_or(|folder| !folder.read_only)
}

/// `path` with links resolved, so a link into a read-only folder is still read-only: the path
/// itself if it exists, else its nearest existing parent with the rest appended (a media folder
/// that does not exist yet still resolves correctly).
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

    fn prefs(folders: &[(&Path, bool)]) -> Preferences {
        let mut prefs = Preferences::default();
        for (path, read_only) in folders {
            prefs.add_library_folder(path.to_path_buf(), *read_only);
        }
        prefs
    }

    #[test]
    fn read_only_folders_are_off_limits_and_others_are_not() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("Music");
        std::fs::create_dir_all(music.join("Album")).unwrap();
        let mut prefs = prefs(&[(&music, true)]);
        assert!(!may_write(&prefs, &music.join("Album/song.flac")));
        assert!(!may_write(&prefs, &music.join("New Folder/not yet there.flac")));
        assert!(may_write(&prefs, &dir.path().join("Downloads/x.flac")), "unlisted paths are writable");
        assert!(may_write(&prefs, &dir.path().join("Musicals/x.flac")), "a sibling with a similar name is not inside");
        prefs.library_folders[0].read_only = false;
        assert!(may_write(&prefs, &music.join("Album/song.flac")));
    }

    #[test]
    fn the_deepest_folder_decides() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("Music");
        let imports = music.join("Imports");
        let archive = imports.join("Archive");
        std::fs::create_dir_all(&archive).unwrap();
        let prefs = prefs(&[(&imports, false), (&music, true), (&archive, true)]);
        assert!(!may_write(&prefs, &music.join("Album/song.flac")));
        assert!(may_write(&prefs, &imports.join("Rip/track.flac")), "a writable folder inside a read-only one");
        assert!(!may_write(&prefs, &archive.join("old.flac")), "and a read-only one inside that");
        assert_eq!(governing_folder(&prefs, &imports.join("x")).map(|f| f.path.clone()), Some(imports));
    }

    #[cfg(unix)]
    #[test]
    fn links_into_a_read_only_folder_are_read_only_too() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("Music");
        std::fs::create_dir_all(&music).unwrap();
        std::os::unix::fs::symlink(&music, dir.path().join("Link")).unwrap();
        let prefs = prefs(&[(&music, true)]);
        assert!(!may_write(&prefs, &dir.path().join("Link/song.flac")));
    }
}
