//! The one place that decides whether TuneBox may write to a path.
//!
//! TuneBox changes files a listener already has in only three ways: writing edited details into
//! a song (Get Info), deleting a song's file when asked, and renaming or moving it when asked
//! (Rename Files from Tags, through [`move_file`] and [`remove_empty_folder`] here). It creates
//! files only in the media folder (imported copies, CD rips) and on players it syncs.
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

/// Why [`move_file`] did not move a file. Nothing was changed in any of these cases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MoveError {
    /// The file or its new place is in a read-only library folder.
    Protected,
    /// Something is already at the new place; it is never replaced.
    Exists,
    /// The new place is on another drive. Moving there would mean copying the music, which
    /// renaming never does.
    OtherDrive,
    Io(String),
}

impl std::fmt::Display for MoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveError::Protected => write!(f, "it or its new folder is in a read-only library folder"),
            MoveError::Exists => write!(f, "a file with the new name is already there"),
            MoveError::OtherDrive => {
                write!(f, "the new folder is on another drive, and renaming never copies music between drives")
            }
            MoveError::Io(reason) => write!(f, "{reason}"),
        }
    }
}

/// Renames or moves the file at `from` to `to`, creating folders as needed. It never replaces
/// a file already at `to`, never copies between drives, and never touches a read-only library
/// folder. A rename that only changes case (`song.flac` → `Song.flac`) goes through a hidden
/// name in between, since on a case-insensitive disk both names are the same file.
pub(crate) fn move_file(prefs: &Preferences, from: &Path, to: &Path) -> Result<(), MoveError> {
    if !may_write(prefs, from) || !may_write(prefs, to) {
        return Err(MoveError::Protected);
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_error(&e))?;
    }
    if !tunebox_model::organize::differ_only_in_case(from, to) {
        return move_without_replacing(from, to);
    }
    let name = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let between = from.with_file_name(format!(".tunebox-rename-{name}"));
    move_without_replacing(from, &between)?;
    move_without_replacing(&between, to).inspect_err(|_| {
        // Put it back as it was; failing that, it is still safe under the hidden name.
        let _ = move_without_replacing(&between, from);
    })
}

/// `from` to `to`, failing if `to` exists. A hard link claims the new name only if it is free,
/// with no gap between looking and taking; disks without hard links (FAT, many shares) fall
/// back to a check just before the rename.
fn move_without_replacing(from: &Path, to: &Path) -> Result<(), MoveError> {
    match std::fs::hard_link(from, to) {
        Ok(()) => std::fs::remove_file(from).map_err(|e| {
            // Undo the link, so the file keeps its old name only.
            let _ = std::fs::remove_file(to);
            io_error(&e)
        }),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(MoveError::Exists),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => Err(MoveError::OtherDrive),
        Err(_) => {
            if std::fs::symlink_metadata(to).is_ok() {
                return Err(MoveError::Exists);
            }
            std::fs::rename(from, to).map_err(|e| io_error(&e))
        }
    }
}

fn io_error(error: &std::io::Error) -> MoveError {
    if error.kind() == std::io::ErrorKind::CrossesDevices {
        MoveError::OtherDrive
    } else {
        MoveError::Io(error.to_string())
    }
}

/// Removes `folder` if it is truly empty, writable, and strictly inside one of `roots` (never a
/// root itself), then its parents the same way. Returns how many folders went.
pub(crate) fn remove_empty_folder(prefs: &Preferences, folder: &Path, roots: &[PathBuf]) -> usize {
    let mut removed = 0;
    let mut current = folder;
    loop {
        let inside_a_root = roots.iter().any(|root| current.starts_with(root) && current != root.as_path());
        let empty = std::fs::read_dir(current).is_ok_and(|mut entries| entries.next().is_none());
        // `remove_dir` itself refuses a folder that is not empty, so a file that arrived since
        // the look above is safe too.
        if !inside_a_root || !empty || !may_write(prefs, current) || std::fs::remove_dir(current).is_err() {
            return removed;
        }
        removed += 1;
        match current.parent() {
            Some(parent) => current = parent,
            None => return removed,
        }
    }
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

    #[test]
    fn moves_never_replace_and_never_touch_read_only_folders() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.flac"), dir.path().join("New/b.flac"));
        std::fs::write(&a, b"a").unwrap();
        let prefs = Preferences::default();
        move_file(&prefs, &a, &b).unwrap();
        assert!(!a.exists() && std::fs::read(&b).unwrap() == b"a");

        std::fs::write(&a, b"other").unwrap();
        assert_eq!(move_file(&prefs, &a, &b), Err(MoveError::Exists));
        assert_eq!(std::fs::read(&b).unwrap(), b"a", "the file already there is untouched");
        assert_eq!(std::fs::read(&a).unwrap(), b"other");

        let protected = self::prefs(&[(dir.path(), true)]);
        assert_eq!(move_file(&protected, &a, &dir.path().join("c.flac")), Err(MoveError::Protected));
        assert!(a.exists());
    }

    #[test]
    fn a_case_only_rename_goes_through_a_hidden_name() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("song.flac"), dir.path().join("Song.flac"));
        std::fs::write(&from, b"x").unwrap();
        move_file(&Preferences::default(), &from, &to).unwrap();
        let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(names, [std::ffi::OsString::from("Song.flac")]);
    }

    #[test]
    fn only_empty_folders_inside_a_root_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Music");
        let deep = root.join("Artist/Album");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::create_dir_all(root.join("Other")).unwrap();
        let roots = [root.clone()];
        assert_eq!(remove_empty_folder(&Preferences::default(), &deep, &roots), 2);
        assert!(root.is_dir(), "a root is never removed, even when empty");
        assert_eq!(remove_empty_folder(&Preferences::default(), &root.join("Other"), &roots), 1);
        assert_eq!(remove_empty_folder(&Preferences::default(), &root, &roots), 0);

        let kept = root.join("Kept");
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::write(kept.join(".DS_Store"), b"").unwrap();
        assert_eq!(remove_empty_folder(&Preferences::default(), &kept, &roots), 0, "hidden files count");
        let outside = dir.path().join("Elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        assert_eq!(remove_empty_folder(&Preferences::default(), &outside, &roots), 0);
        let protected = prefs(&[(&root, true)]);
        std::fs::create_dir_all(root.join("Empty")).unwrap();
        assert_eq!(remove_empty_folder(&protected, &root.join("Empty"), &roots), 0);
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
