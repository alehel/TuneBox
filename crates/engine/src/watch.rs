//! Changes in the library folders, as a folder watcher reports them (the app and
//! `tunebox-cli watch` use the one in `tunebox-platform`). New songs are added, songs another
//! program changed are read again, and a song whose file was deleted stays in the library,
//! ratings and play counts included, for the listener to deal with: nothing is removed behind
//! their back. A song that moved is relinked by the import (see `MovedSongs`).
//!
//! Watching only ever reads. Changes that arrive while an import or refresh runs wait for it to
//! end and go into the next one.

use crate::LibraryOnlyEdits;
use crate::notices::Notice;
use crate::prefs::LibraryFolder;
use crate::session::{Origin, Session};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tunebox_library::Library;
use tunebox_model::TrackId;

/// What is at a changed path now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathKind {
    File,
    Folder,
}

/// What to do about a set of changed paths.
#[derive(Debug, Default, PartialEq, Eq)]
struct Plan {
    /// New files and folders to add. Folders are walked (known songs in them are skipped): a
    /// file copied into a brand-new folder can arrive before the watch on that folder does, so
    /// the folder's own event may be the only one.
    import: Vec<PathBuf>,
    /// Known songs whose files changed.
    refresh: Vec<TrackId>,
}

impl Session {
    /// Paths that changed in the library folders, as a folder watcher saw them. They are acted
    /// on now, or as soon as the running import or refresh ends.
    pub fn folders_changed(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.folder_changes.extend(paths);
        self.act_on_folder_changes();
    }

    pub(crate) fn act_on_folder_changes(&mut self) {
        let busy = self.jobs.is_running(crate::JobKind::Import) || self.jobs.is_running(crate::JobKind::Refresh);
        if self.folder_changes.is_empty() || busy {
            return;
        }
        let changed = std::mem::take(&mut self.folder_changes);
        let plan = plan(&changed, &self.prefs.library_folders, &self.library, path_kind);
        if !plan.import.is_empty()
            && let Err(e) = self.start_import(plan.import, Origin::Watcher)
        {
            self.notices.push(Notice::warning(format!("New songs in your library folders were not added: {e}")));
        }
        // Edits kept in TuneBox only are never undone without asking.
        if !plan.refresh.is_empty()
            && let Err(e) = self.start_refresh(&plan.refresh, LibraryOnlyEdits::Keep, Origin::Watcher)
        {
            self.notices.push(Notice::warning(format!("Changed songs were not read again: {e}")));
        }
    }
}

fn path_kind(path: &Path) -> Option<PathKind> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.is_dir() {
        Some(PathKind::Folder)
    } else if metadata.is_file() {
        Some(PathKind::File)
    } else {
        None
    }
}

/// Decides what each changed path means. `kind` says what is at a path now (`None`: nothing).
fn plan(
    changed: &BTreeSet<PathBuf>,
    folders: &[LibraryFolder],
    library: &Library,
    kind: impl Fn(&Path) -> Option<PathKind>,
) -> Plan {
    let mut plan = Plan::default();
    // Sorted, so a folder comes before what is inside it and covers it.
    for path in changed {
        if !folders.iter().any(|folder| is_watched(path, &folder.path)) {
            continue;
        }
        if plan.import.iter().any(|folder| path.starts_with(folder)) {
            continue;
        }
        match (library.track_at(path), kind(path)) {
            (Some(id), Some(PathKind::File)) => plan.refresh.push(id),
            // Gone: the song stays in the library with its history.
            (_, None) => {}
            (None, Some(PathKind::Folder)) => plan.import.push(path.clone()),
            (None, Some(PathKind::File)) if tunebox_import::is_audio_file(path) => plan.import.push(path.clone()),
            _ => {}
        }
    }
    plan
}

/// Inside `folder`, and not hidden or temporary: TuneBox's own `.tunebox-edit-…` copies and
/// `*.part` files come and go while it writes, as do other programs' hidden files.
fn is_watched(path: &Path, folder: &Path) -> bool {
    let Ok(inside) = path.strip_prefix(folder) else { return false };
    let hidden = inside.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
    let partial = path.extension().is_some_and(|e| e == "part");
    !hidden && !partial
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::time::Duration;
    use tunebox_library::NewTrack;
    use tunebox_model::{AudioFormat, FileInfo, TrackMeta};

    fn known(library: &mut Library, path: &str) -> TrackId {
        library.add_track(NewTrack {
            location: PathBuf::from(path),
            meta: TrackMeta::default(),
            file: FileInfo {
                format: AudioFormat::Flac,
                duration: Duration::from_secs(1),
                size_bytes: 1,
                bit_rate_kbps: None,
                sample_rate: None,
                channels: None,
            },
            date_added: Utc::now(),
        })
    }

    #[test]
    fn new_songs_are_added_changed_ones_read_and_the_rest_ignored() {
        let mut library = Library::default();
        let changed_song = known(&mut library, "/music/a.flac");
        known(&mut library, "/music/deleted.flac");
        let folders = [LibraryFolder { path: PathBuf::from("/music"), read_only: true }];
        let changed: BTreeSet<PathBuf> = [
            "/music/a.flac",
            "/music/deleted.flac",
            "/music/new.mp3",
            "/music/cover.jpg",
            "/music/New Album",
            "/music/New Album/01.flac",
            "/music/.tunebox-edit-a.flac",
            "/music/.Trash/x.flac",
            "/music/b.flac.part",
            "/elsewhere/c.flac",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        let kind = |path: &Path| match path.to_str() {
            Some("/music/deleted.flac") => None,
            Some("/music/New Album") => Some(PathKind::Folder),
            _ => Some(PathKind::File),
        };
        let plan = plan(&changed, &folders, &library, kind);
        assert_eq!(plan.refresh, [changed_song]);
        assert_eq!(plan.import, [PathBuf::from("/music/New Album"), PathBuf::from("/music/new.mp3")]);
    }
}
