//! Keeps a folder watcher on the library folders while Preferences says to, and hands what it
//! sees to the session (`Session::folders_changed`). The watcher is restarted when the list of
//! folders changes and stopped when watching is turned off.

use std::path::PathBuf;
use std::time::Duration;
use tunebox_engine::{Notice, Session};
use tunebox_platform::{FolderWatcher, QUIET};

#[derive(Default)]
pub struct LibraryWatch {
    watcher: Option<FolderWatcher>,
    /// The folders `watcher` was started on.
    watching: Vec<PathBuf>,
}

impl LibraryWatch {
    /// Matches the watcher to the preferences, then passes settled changes to the session.
    /// Returns notices for folders that could not be watched.
    pub fn update(&mut self, session: &mut Session, ctx: &egui::Context) -> Vec<Notice> {
        let mut notices = Vec::new();
        let prefs = session.preferences();
        let wanted: Vec<PathBuf> = if prefs.watch_library_folders {
            prefs.library_folders.iter().map(|f| f.path.clone()).collect()
        } else {
            Vec::new()
        };
        if wanted != self.watching {
            self.watcher = None;
            if !wanted.is_empty() {
                let waker = ctx.clone();
                // Come back once the change has had time to settle.
                let wake = move || waker.request_repaint_after(QUIET + Duration::from_millis(100));
                match FolderWatcher::start(&wanted, QUIET, wake) {
                    Ok(watcher) => {
                        notices.extend(watcher.failures().iter().map(|(folder, reason)| cannot_watch(folder, reason)));
                        self.watcher = Some(watcher);
                    }
                    Err(reason) => notices.push(cannot_watch(&PathBuf::from("your library folders"), &reason)),
                }
            }
            self.watching = wanted;
        }
        if let Some(watcher) = &mut self.watcher {
            let changed = watcher.settled();
            if !changed.is_empty() {
                session.folders_changed(changed);
            }
            if watcher.has_pending() {
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
        notices
    }
}

fn cannot_watch(folder: &std::path::Path, reason: &str) -> Notice {
    Notice::warning(format!(
        "TuneBox cannot watch {} for changes ({reason}). Use File ▸ Scan Library Folders to pick them up.",
        tunebox_platform::display_path(folder)
    ))
}
