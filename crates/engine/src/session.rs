//! The session: what it owns, opening and saving, and the per-frame [`Session::tick`].
//!
//! Its methods are grouped by subject in sibling modules (`library_ops`, `playback`, `sources`,
//! `devices`, `cd`), each adding an `impl Session` block.

use crate::EngineError;
use crate::cd::CdState;
use crate::devices::DeviceEntry;
use crate::jobs::{JobId, JobKind, JobStatus, Jobs};
use crate::notices::Notice;
use crate::playback::PlaybackState;
use crate::prefs::Preferences;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tunebox_import::ImportReport;
use tunebox_library::store::{self, StoreError};
use tunebox_library::{Library, LibraryLock};
use tunebox_playback::Player;
#[cfg(feature = "musicbrainz")]
use tunebox_rip::DiscInfo;
use tunebox_rip::RipReport;
use tunebox_sync::{Device, SyncReport};

/// How long library changes wait before being saved, so a burst of edits is one write.
const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);

/// Where the session keeps its files.
/// Who started an import or refresh. The folder watcher's work reports only what changed, so
/// it never chatters about songs that were already up to date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Listener,
    Watcher,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionPaths {
    pub library_file: PathBuf,
    pub preferences_file: PathBuf,
    /// The media folder used until the listener picks another.
    pub default_media_folder: PathBuf,
}

/// What a finished job hands back.
pub(crate) enum Outcome {
    Imported(ImportReport, Origin),
    DiscRead(Result<Option<CdState>, String>),
    #[cfg(feature = "musicbrainz")]
    NamesFound {
        disc_id: String,
        result: Result<Vec<DiscInfo>, String>,
    },
    Ripped(RipReport),
    Refreshed(crate::refresh::RefreshReport, Origin),
    LocatedOthers(Vec<(tunebox_model::TrackId, PathBuf)>),
    ArtworkChanged(crate::artwork::ArtworkReport),
    Renamed(crate::rename::RenameReport),
    Synced {
        root: PathBuf,
        result: Result<(Device, SyncReport), String>,
    },
    DeviceSaved {
        root: PathBuf,
        result: Result<(), String>,
    },
}

/// One listener's TuneBox: the library and everything acting on it.
pub struct Session {
    pub(crate) paths: SessionPaths,
    pub(crate) library: Library,
    pub(crate) prefs: Preferences,
    pub(crate) jobs: Jobs<Outcome>,
    pub(crate) playback: PlaybackState,
    pub(crate) devices: Vec<DeviceEntry>,
    /// MTP players whose settings changed while another save was running: saved when it ends.
    pub(crate) unsaved_devices: Vec<PathBuf>,
    pub(crate) cd: Option<CdState>,
    pub(crate) notices: Vec<Notice>,
    /// When the library first changed since it was last saved.
    unsaved_since: Option<Instant>,
    /// Bumped whenever anything a view shows may have changed (see [`Session::revision`]).
    revision: u64,
    /// Whether the listener has been told edits to protected songs stay in the library.
    pub(crate) protected_edit_noted: bool,
    /// Paths the folder watcher reported, waiting for the running import or refresh to end.
    pub(crate) folder_changes: std::collections::BTreeSet<PathBuf>,
    /// Which songs' files can be found.
    pub(crate) availability: crate::availability::Availability,
    /// Albums whose covers changed, for the frontend (see `Session::take_artwork_changes`).
    pub(crate) artwork_changes: Vec<crate::AlbumKey>,
    /// The rename in progress, if any.
    pub(crate) renaming: Option<crate::rename::RenameRun>,
    /// This process's hold on the library, for as long as the session lives.
    _lock: LibraryLock,
}

impl Session {
    /// Opens the library and preferences at `paths`, playing through `player`.
    ///
    /// A damaged library file is moved aside (never overwritten) and a new library started, with
    /// a notice saying where the old file went.
    ///
    /// Fails if another TuneBox (the app or `tunebox-cli`) has the same library open: each
    /// would otherwise save over the other's changes.
    pub fn open(paths: SessionPaths, player: Player) -> Result<Session, EngineError> {
        let lock = LibraryLock::acquire(&paths.library_file)?;
        let prefs = Preferences::load(&paths.preferences_file, &paths.default_media_folder);
        let mut notices = Vec::new();
        let library = match store::load(&paths.library_file) {
            Ok(library) => library,
            Err(StoreError::Damaged(reason)) => {
                let aside = paths.library_file.with_extension("damaged.json");
                std::fs::rename(&paths.library_file, &aside).map_err(StoreError::Io)?;
                notices.push(Notice::error(format!(
                    "The library file was damaged ({reason}). It was moved to {} and a new library was started.",
                    aside.display()
                )));
                Library::with_default_playlists()
            }
            Err(e) => return Err(e.into()),
        };
        let playback = PlaybackState::new(player, &prefs);
        let mut session = Session {
            paths,
            library,
            prefs,
            jobs: Jobs::new(),
            playback,
            devices: Vec::new(),
            unsaved_devices: Vec::new(),
            cd: None,
            notices,
            unsaved_since: None,
            revision: 0,
            protected_edit_noted: false,
            folder_changes: std::collections::BTreeSet::new(),
            availability: crate::availability::Availability::start(),
            artwork_changes: Vec::new(),
            renaming: None,
            _lock: lock,
        };
        session.check_all_files();
        Ok(session)
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    pub fn preferences(&self) -> &Preferences {
        &self.prefs
    }

    /// Replaces the preferences and saves them.
    pub fn set_preferences(&mut self, prefs: Preferences) {
        self.playback.apply_preferences(&prefs);
        let folders_changed = prefs.library_folders != self.prefs.library_folders;
        self.prefs = prefs;
        self.save_preferences();
        if folders_changed {
            // Library folders are places: which songs are missing and which unavailable may differ.
            self.check_all_files();
        }
    }

    pub(crate) fn save_preferences(&mut self) {
        if let Err(e) = self.prefs.save(&self.paths.preferences_file) {
            self.notices.push(Notice::error(format!("Could not save preferences: {e}")));
        }
    }

    /// Marks the library as changed; it is saved shortly after.
    pub(crate) fn library_changed(&mut self) {
        self.unsaved_since.get_or_insert_with(Instant::now);
        self.content_changed();
    }

    /// Notes that something views show (tracks, playlists, devices, the disc) changed.
    pub(crate) fn content_changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// A number that changes whenever the library, the devices or the disc change, so a
    /// frontend can cache what it derives from them (sorted rows, the browser) until it does.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.unsaved_since.is_some()
    }

    /// Saves the library now.
    pub fn save(&mut self) -> Result<(), EngineError> {
        store::save(&self.library, &self.paths.library_file)?;
        self.unsaved_since = None;
        Ok(())
    }

    /// Does the session's housekeeping; call it every frame (or every few hundred milliseconds
    /// from a command-line loop). Applies finished jobs, follows the player, and saves the
    /// library once changes have settled.
    pub fn tick(&mut self) {
        for finished in self.jobs.poll() {
            match finished.outcome {
                Ok(outcome) => self.apply_outcome(outcome),
                Err(message) => {
                    if finished.kind == JobKind::Rename {
                        // The moves it made before stopping are logged: the library follows them.
                        self.finish_rename(None);
                    }
                    self.notices.push(Notice::error(format!("{} failed. {message}", finished.kind.noun())));
                }
            }
        }
        self.act_on_folder_changes();
        self.follow_file_checks();
        self.follow_player();
        if self.unsaved_since.is_some_and(|since| since.elapsed() >= AUTOSAVE_DELAY)
            && let Err(e) = self.save()
        {
            self.notices.push(Notice::error(format!("Could not save the library: {e}")));
            // Try again after another delay rather than every frame.
            self.unsaved_since = Some(Instant::now());
        }
    }

    fn apply_outcome(&mut self, outcome: Outcome) {
        self.content_changed();
        match outcome {
            Outcome::Imported(report, origin) => self.finish_import(report, origin),
            Outcome::DiscRead(result) => self.finish_disc_read(result),
            #[cfg(feature = "musicbrainz")]
            Outcome::NamesFound { disc_id, result } => self.finish_lookup(&disc_id, result),
            Outcome::Ripped(report) => self.finish_rip(report),
            Outcome::Refreshed(report, origin) => self.finish_refresh(report, origin),
            Outcome::LocatedOthers(found) => self.finish_locating_others(found),
            Outcome::ArtworkChanged(report) => self.finish_artwork(report),
            Outcome::Renamed(report) => self.finish_rename(Some(report)),
            Outcome::Synced { root, result } => self.finish_sync(&root, result),
            Outcome::DeviceSaved { root, result } => self.finish_device_save(&root, result),
        }
    }

    // ----- jobs and notices -----

    pub fn jobs(&self) -> Vec<JobStatus> {
        self.jobs.statuses()
    }

    pub fn is_busy(&self, kind: JobKind) -> bool {
        self.jobs.is_running(kind)
    }

    pub fn cancel_job(&mut self, id: JobId) {
        self.jobs.cancel(id);
    }

    /// Messages since the last call, oldest first.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    /// Stops playback and background work and saves. Call before quitting.
    pub fn shutdown(&mut self) -> Result<(), EngineError> {
        self.jobs.cancel_all();
        self.playback.stop();
        self.finish_rename_before_quitting();
        self.save()
    }
}
