//! One TuneBox per library. Two processes holding the same library in memory would each save
//! their own copy and silently undo the other's changes (a rating set from the command line
//! while the app runs, say). [`LibraryLock`] makes the second one stop with an error instead.
//!
//! The lock is the operating system's own lock on a small file next to the library
//! (`flock` / `LockFileEx`, through `File::try_lock`). The system releases it when the process
//! ends, however it ends, so a crash never leaves the library locked.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Why the library could not be locked.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error(
        "this library is already open in another TuneBox (the app or tunebox-cli). Close that one first, or open a different library with --library-dir. The library: {}",
        .0.display()
    )]
    InUse(PathBuf),
    #[error("could not lock the library ({}): {source}", path.display())]
    Io { path: PathBuf, source: std::io::Error },
}

/// Holds the library for this process until dropped.
#[derive(Debug)]
pub struct LibraryLock {
    // The lock lives as long as this handle is open.
    _file: File,
}

impl LibraryLock {
    /// Locks the library at `library_file`, through `<library file>.lock` beside it.
    pub fn acquire(library_file: &Path) -> Result<LibraryLock, LockError> {
        let path = library_file.with_extension("lock");
        let io = |source| LockError::Io { path: path.clone(), source };
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(io)?;
        }
        // Not truncated on open: the file of a TuneBox that holds the lock is left as it is.
        let mut file = OpenOptions::new().create(true).truncate(false).write(true).open(&path).map_err(io)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(LockError::InUse(library_file.to_path_buf())),
            Err(TryLockError::Error(e)) => return Err(io(e)),
        }
        // For a person wondering who holds it. The lock itself is what counts, so a note that
        // cannot be written changes nothing.
        let note = format!("Held by TuneBox, process {}\n", std::process::id());
        let _ = file.set_len(0).and_then(|()| file.write_all(note.as_bytes()));
        Ok(LibraryLock { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_lock_is_refused_until_the_first_is_released() {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("TuneBox/TuneBox Library.json");
        let first = LibraryLock::acquire(&library).unwrap();
        let second = LibraryLock::acquire(&library).unwrap_err();
        assert!(matches!(second, LockError::InUse(ref p) if *p == library), "{second}");
        assert!(second.to_string().contains("already open in another TuneBox"));
        drop(first);
        assert!(LibraryLock::acquire(&library).is_ok());
    }

    #[test]
    fn different_libraries_do_not_block_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let _a = LibraryLock::acquire(&dir.path().join("a/Library.json")).unwrap();
        assert!(LibraryLock::acquire(&dir.path().join("b/Library.json")).is_ok());
    }
}
