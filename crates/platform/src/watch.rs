//! Watching the library folders. The operating system reports changes (inotify, FSEvents or
//! ReadDirectoryChangesW, through notify); [`FolderWatcher::settled`] hands each changed path
//! out once it has been quiet for a moment, so a file still being copied is not read half
//! written. The engine decides what the changes mean (`Session::folders_changed`).

use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// How long a path must be quiet before it is handed out.
pub const QUIET: Duration = Duration::from_secs(2);

/// Watches folders, recursively, for as long as it lives.
pub struct FolderWatcher {
    // Dropping the watcher stops the watching.
    _watcher: RecommendedWatcher,
    changes: Receiver<PathBuf>,
    settler: Settler,
    failures: Vec<(PathBuf, String)>,
}

impl FolderWatcher {
    /// Starts watching `folders`. `wake` is called from the watcher's thread on every change, so
    /// an interface can come back for the settled paths. A folder that cannot be watched (gone,
    /// or the system's limit on watches reached) is listed in [`FolderWatcher::failures`]; the
    /// others are still watched.
    pub fn start(folders: &[PathBuf], quiet: Duration, wake: impl Fn() + Send + 'static) -> Result<Self, String> {
        let (sender, changes) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            let Ok(event) = result else { return };
            if !matters(&event.kind) {
                return;
            }
            for path in event.paths {
                // The receiver is gone only when the watcher is being dropped.
                let _ = sender.send(path);
            }
            wake();
        })
        .map_err(|e| e.to_string())?;
        let mut failures = Vec::new();
        for folder in folders {
            if let Err(e) = watcher.watch(folder, RecursiveMode::Recursive) {
                log::warn!("not watching {}: {e}", folder.display());
                failures.push((folder.clone(), e.to_string()));
            }
        }
        Ok(FolderWatcher { _watcher: watcher, changes, settler: Settler::new(quiet), failures })
    }

    /// The folders that could not be watched, with why.
    pub fn failures(&self) -> &[(PathBuf, String)] {
        &self.failures
    }

    /// The changed paths that have been quiet long enough, each once, sorted.
    pub fn settled(&mut self) -> Vec<PathBuf> {
        let now = Instant::now();
        while let Ok(path) = self.changes.try_recv() {
            self.settler.note(path, now);
        }
        self.settler.take_settled(now)
    }

    /// Whether changes are waiting to settle, so the caller should look again soon.
    pub fn has_pending(&self) -> bool {
        !self.settler.is_empty()
    }
}

/// Whether an event can mean new or changed music. Reading a file (as TuneBox does to read
/// tags) and changes to permissions or times cannot, and reacting to them would make every
/// refresh trigger another.
fn matters(kind: &EventKind) -> bool {
    match kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) | EventKind::Modify(notify::event::ModifyKind::Metadata(_)) => false,
        _ => true,
    }
}

/// Holds changed paths until each has been quiet for a while.
#[derive(Debug)]
struct Settler {
    quiet: Duration,
    last_change: HashMap<PathBuf, Instant>,
}

impl Settler {
    fn new(quiet: Duration) -> Self {
        Settler { quiet, last_change: HashMap::new() }
    }

    fn note(&mut self, path: PathBuf, at: Instant) {
        self.last_change.insert(path, at);
    }

    fn take_settled(&mut self, now: Instant) -> Vec<PathBuf> {
        let quiet = self.quiet;
        let mut settled: Vec<PathBuf> = self
            .last_change
            .iter()
            .filter(|(_, at)| now.saturating_duration_since(**at) >= quiet)
            .map(|(path, _)| path.clone())
            .collect();
        for path in &settled {
            self.last_change.remove(path);
        }
        settled.sort();
        settled
    }

    fn is_empty(&self) -> bool {
        self.last_change.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn a_path_is_handed_out_once_it_has_been_quiet() {
        let start = Instant::now();
        let mut settler = Settler::new(Duration::from_secs(2));
        settler.note(PathBuf::from("/m/a.flac"), start);
        settler.note(PathBuf::from("/m/b.flac"), start);
        // b is still being written.
        settler.note(PathBuf::from("/m/b.flac"), start + Duration::from_secs(1));
        assert!(settler.take_settled(start + Duration::from_secs(1)).is_empty());
        assert_eq!(settler.take_settled(start + Duration::from_secs(2)), [PathBuf::from("/m/a.flac")]);
        assert_eq!(settler.take_settled(start + Duration::from_secs(3)), [PathBuf::from("/m/b.flac")]);
        assert!(settler.is_empty());
    }

    #[test]
    fn reading_a_file_is_not_a_change() {
        assert!(!matters(&EventKind::Access(AccessKind::Open(AccessMode::Read))));
        assert!(!matters(&EventKind::Access(AccessKind::Close(AccessMode::Read))));
        assert!(matters(&EventKind::Access(AccessKind::Close(AccessMode::Write))));
        assert!(matters(&EventKind::Create(notify::event::CreateKind::File)));
    }

    #[test]
    fn sees_a_file_appear_in_a_watched_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().canonicalize().unwrap();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        let mut watcher = FolderWatcher::start(std::slice::from_ref(&folder), Duration::from_millis(100), move || {
            counter.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();
        assert!(watcher.failures().is_empty());
        std::fs::create_dir(folder.join("Album")).unwrap();
        std::fs::write(folder.join("Album/song.flac"), b"not really audio").unwrap();

        // A file written into a brand-new folder can beat the watch on that folder, so either
        // the folder or the file is reported. The engine walks a new folder, so both work.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        while !seen.iter().any(|p| *p == folder.join("Album") || *p == folder.join("Album/song.flac")) {
            assert!(Instant::now() < deadline, "saw only {seen:?}");
            std::thread::sleep(Duration::from_millis(50));
            seen.extend(watcher.settled());
        }
        assert!(wakes.load(Ordering::Relaxed) > 0);
        assert!(FolderWatcher::start(&[folder.join("missing")], QUIET, || {}).unwrap().failures().len() == 1);
    }
}
