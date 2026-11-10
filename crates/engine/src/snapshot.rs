//! A read-only copy of a library, for looking without taking the library over. It takes no
//! lock, so it works while the app (or another `tunebox-cli`) has the library open, and it
//! cannot write: it has no methods that change anything, and it does not even move a damaged
//! library file aside the way `Session::open` does.
//!
//! The library file is always replaced whole (`craft-atomic`), so a snapshot reads a complete
//! library: the one last saved, at most a couple of seconds behind an app that is running.

use crate::error::EngineError;
use crate::prefs::Preferences;
use crate::session::SessionPaths;
use crate::view::LibraryView;
use tunebox_library::Library;
use tunebox_library::store::{self, StoreError};

pub struct LibrarySnapshot {
    library: Library,
    prefs: Preferences,
}

impl LibrarySnapshot {
    /// Reads the library and preferences at `paths` as they were last saved.
    pub fn open(paths: &SessionPaths) -> Result<Self, EngineError> {
        let prefs = Preferences::load(&paths.preferences_file, &paths.default_media_folder);
        let library = store::load(&paths.library_file).map_err(|e| match e {
            // Repairing it is a write, which is the session's to do.
            StoreError::Damaged(reason) => EngineError::Store(StoreError::Damaged(format!(
                "{reason}. Open TuneBox, or run a tunebox-cli command that changes the library, to set it aside and start afresh"
            ))),
            other => other.into(),
        })?;
        Ok(LibrarySnapshot { library, prefs })
    }

    pub fn view(&self) -> LibraryView<'_> {
        LibraryView { library: &self.library, prefs: &self.prefs, devices: &[] }
    }
}
