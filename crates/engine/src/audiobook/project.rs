//! The audiobook in progress on disk: one folder per book, beside the library file.
//!
//! ```text
//! <app folder>/Audiobooks/<id>/
//!     Project.json      the book (details, discs, edits, output), written atomically
//!     Cover             the cover's bytes, when there is one
//!     disc-01/track-01.flac …   the staged rip
//! ```
//!
//! The folder belongs to TuneBox, like the library file, so it is not written through
//! `files.rs`. A damaged `Project.json` is moved aside and the staged audio kept, so nothing has
//! to be ripped again by hand.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tunebox_model::audiobook::{Book, TrackRef};
use tunebox_tags::Artwork;

const FORMAT: &str = "tunebox-audiobook";
const VERSION: u32 = 1;
const PROJECT_FILE: &str = "Project.json";
const COVER_FILE: &str = "Cover";

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    format: String,
    version: u32,
    book: Book,
    /// The cover's type ("image/jpeg"), when there is a cover.
    #[serde(default)]
    cover_mime: Option<String>,
}

/// A book read back from its folder.
pub(crate) struct Loaded {
    pub folder: PathBuf,
    pub book: Book,
    pub cover: Option<Artwork>,
}

/// Why the book in progress could not be read.
pub(crate) struct Damaged {
    pub folder: PathBuf,
    pub reason: String,
    /// Where the damaged file was moved.
    pub moved_to: Option<PathBuf>,
}

/// Where books in progress live, for the library at `library_file`.
pub(crate) fn books_folder(library_file: &Path) -> PathBuf {
    library_file.with_file_name("Audiobooks")
}

/// A new, empty folder for a book.
pub(crate) fn new_folder(books: &Path, now: chrono::DateTime<chrono::Utc>) -> std::io::Result<PathBuf> {
    let stamp = now.format("%Y%m%d-%H%M%S").to_string();
    let folder = (0..1000)
        .map(|n| books.join(if n == 0 { stamp.clone() } else { format!("{stamp}-{n}") }))
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| books.join(format!("{stamp}-x")));
    std::fs::create_dir_all(&folder)?;
    Ok(folder)
}

/// The book in progress, if there is one. With several (which TuneBox does not make), the
/// newest.
pub(crate) fn load(books: &Path) -> Result<Option<Loaded>, Damaged> {
    let Ok(entries) = std::fs::read_dir(books) else { return Ok(None) };
    let mut folders: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.join(PROJECT_FILE).is_file()).collect();
    folders.sort();
    let Some(folder) = folders.pop() else { return Ok(None) };
    read(&folder).map(Some).map_err(|reason| {
        let file = folder.join(PROJECT_FILE);
        let aside = folder.join("Project.damaged.json");
        let moved_to = std::fs::rename(&file, &aside).ok().map(|()| aside);
        Damaged { folder, reason, moved_to }
    })
}

fn read(folder: &Path) -> Result<Loaded, String> {
    let bytes = std::fs::read(folder.join(PROJECT_FILE)).map_err(|e| e.to_string())?;
    let file: ProjectFile = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if file.format != FORMAT {
        return Err(format!("expected a {FORMAT} file, found \"{}\"", file.format));
    }
    if file.version > VERSION {
        return Err(format!("it was written by a newer TuneBox (version {})", file.version));
    }
    let cover = match file.cover_mime {
        Some(mime) => std::fs::read(folder.join(COVER_FILE)).ok().map(|data| Artwork { mime, data }),
        None => None,
    };
    Ok(Loaded { folder: folder.to_path_buf(), book: file.book, cover })
}

/// Saves the book, and its cover when `cover_changed`.
pub(crate) fn save(folder: &Path, book: &Book, cover: Option<&Artwork>, cover_changed: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    if cover_changed {
        match cover {
            Some(cover) => craft_atomic::atomic_write(&folder.join(COVER_FILE), &cover.data)?,
            None => remove_if_there(&folder.join(COVER_FILE))?,
        }
    }
    let file = ProjectFile {
        format: FORMAT.to_owned(),
        version: VERSION,
        book: book.clone(),
        cover_mime: cover.map(|c| c.mime.clone()),
    };
    let bytes = serde_json::to_vec_pretty(&file).map_err(std::io::Error::other)?;
    craft_atomic::atomic_write(&folder.join(PROJECT_FILE), &bytes)?;
    Ok(())
}

/// A disc's staging folder.
pub(crate) fn disc_folder(folder: &Path, disc: u16) -> PathBuf {
    folder.join(format!("disc-{disc:02}"))
}

/// Where track `track` of the book is staged.
pub(crate) fn staged_path(folder: &Path, track: TrackRef) -> PathBuf {
    tunebox_rip::staged_path(&disc_folder(folder, track.disc), track.track)
}

/// Deletes the book's folder: its project file, cover and staged rip.
pub(crate) fn delete(folder: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(folder) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn remove_if_there(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunebox_model::audiobook::BookDetails;

    #[test]
    fn a_book_is_saved_and_read_back_with_its_cover() {
        let dir = tempfile::tempdir().unwrap();
        let books = dir.path().join("Audiobooks");
        let folder = new_folder(&books, chrono::Utc::now()).unwrap();
        let book = Book::new(BookDetails { title: "The Long Road Home".into(), ..BookDetails::default() }, vec![]);
        let cover = Artwork { mime: "image/png".into(), data: vec![1, 2, 3] };
        save(&folder, &book, Some(&cover), true).unwrap();
        let loaded = load(&books).ok().flatten().unwrap();
        assert_eq!(loaded.book, book);
        assert_eq!(loaded.cover, Some(cover));
        assert_eq!(loaded.folder, folder);
        assert_eq!(staged_path(&folder, TrackRef::new(2, 7)), folder.join("disc-02/track-07.flac"));
    }

    #[test]
    fn a_damaged_project_is_moved_aside_and_its_audio_kept() {
        let dir = tempfile::tempdir().unwrap();
        let folder = new_folder(dir.path(), chrono::Utc::now()).unwrap();
        std::fs::create_dir_all(disc_folder(&folder, 1)).unwrap();
        std::fs::write(staged_path(&folder, TrackRef::new(1, 1)), b"audio").unwrap();
        std::fs::write(folder.join(PROJECT_FILE), b"{ not json").unwrap();
        let Err(damaged) = load(dir.path()) else { panic!("a damaged file is reported") };
        assert_eq!(damaged.moved_to, Some(folder.join("Project.damaged.json")));
        assert!(staged_path(&folder, TrackRef::new(1, 1)).exists());
        assert!(load(dir.path()).ok().flatten().is_none(), "nothing left to load");
    }

    #[test]
    fn no_folder_is_no_book() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(&dir.path().join("none")).ok().flatten().is_none());
    }
}
