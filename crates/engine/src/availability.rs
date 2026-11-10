//! Which songs' files can be found, kept up to date in the background (see `file_check`). A
//! song is *missing* when its place is there and its file is not, and *unavailable* when the
//! place itself cannot be reached. Network shares get special care: the library remembers
//! them, and one that is not mounted is not connected, without any file system call, even if
//! its mount point lingers as an empty folder.
//!
//! Nothing here changes a song or removes it from the library. A missing file is shown, so the
//! listener can deal with it; an unavailable one waits for its place to come back.

use crate::file_check::{Answer, CheckItem, FileChecker, FileStatus, Request, check_now};
use crate::notices::Notice;
use crate::places::{Place, place_of};
use crate::session::Session;
use crate::view::LibraryView;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tunebox_model::{Track, TrackId};

/// How often places are asked whether they are still there.
const PROBE_EVERY: Duration = Duration::from_secs(30);

static PRESENT: FileStatus = FileStatus::Present;

/// What the session knows about where songs' files are.
#[derive(Default)]
pub(crate) struct Availability {
    checker: Option<FileChecker>,
    /// Songs that are not present. Absent means present, or not checked yet.
    statuses: HashMap<TrackId, FileStatus>,
    /// Whether each place answered when last asked.
    reachable: BTreeMap<Place, bool>,
    /// Network shares mounted now; `None` until the frontend has said. Until then songs on
    /// remembered shares are not checked, since an unmounted share's empty mount point would
    /// make all of them look missing.
    mounts: Option<Vec<PathBuf>>,
    last_probe: Option<Instant>,
}

impl Availability {
    pub fn start() -> Self {
        Availability { checker: FileChecker::start(), ..Availability::default() }
    }

    /// Forgets what was known about a song's file, as when it was just found somewhere else.
    pub fn forget(&mut self, id: TrackId) {
        self.statuses.remove(&id);
    }
}

impl Session {
    /// Whether a song's file can be found.
    pub fn file_status(&self, id: TrackId) -> &FileStatus {
        self.availability.statuses.get(&id).unwrap_or(&PRESENT)
    }

    /// What is wrong with a song's file, in words for the listener; `None` when it is there.
    pub fn file_problem(&self, id: TrackId) -> Option<String> {
        let track = self.library.track(id)?;
        match self.file_status(id) {
            FileStatus::Present => None,
            FileStatus::Missing => Some(format!(
                "Its file is missing ({}). It may have been moved, renamed or deleted.",
                track.location.display()
            )),
            FileStatus::Unavailable(place) if place.network => Some(format!(
                "It is on “{}”, which is not connected. It plays again once “{}” is back.",
                place.name(),
                place.name()
            )),
            FileStatus::Unavailable(place) => {
                Some(format!("It is on “{}”, which cannot be found. Connect the drive to play it.", place.name()))
            }
        }
    }

    /// The network shares mounted now, as the platform sees them. Shares holding library songs
    /// are remembered in the library, and every place is asked again, since a mount coming or
    /// going is exactly when a place appears or disappears.
    pub fn set_network_mounts(&mut self, mounts: Vec<PathBuf>) {
        let first = self.availability.mounts.is_none();
        for mount in &mounts {
            if self.library.tracks().any(|t| t.location.starts_with(mount))
                && self.library.remember_network_place(mount.clone())
            {
                self.library_changed();
            }
        }
        self.availability.mounts = Some(mounts);
        if first {
            // Songs on remembered shares waited for this.
            self.check_all_files();
        }
        // At once, so a share that is not connected is said so now, not at the next probe.
        self.probe_places();
    }

    /// The place a song is on.
    pub(crate) fn place_of_track(&self, track: &Track) -> Option<Place> {
        let mut network: Vec<PathBuf> = self.library.network_places().to_vec();
        network.extend(self.availability.mounts.iter().flatten().cloned());
        place_of(&track.location, &self.prefs.library_folders, &network)
    }

    /// Looks for every song's file now and waits for the answers (within the same deadlines),
    /// for the command line, which cannot wait for the background checks to come in.
    pub fn check_files_now(&mut self) {
        let mounts = self.availability.mounts.clone().unwrap_or_default();
        let found = self.view().find_unplayable(&mounts);
        self.availability.statuses.clear();
        self.apply_file_statuses(found);
    }

    /// Looks for every song's file again.
    pub(crate) fn check_all_files(&mut self) {
        let ids: Vec<TrackId> = self.library.tracks().map(|t| t.id).collect();
        self.availability.statuses.retain(|id, _| self.library.track(*id).is_some());
        self.check_tracks(&ids);
        self.availability.last_probe = Some(Instant::now());
    }

    /// Looks for these songs' files again, in the background.
    pub(crate) fn check_tracks(&mut self, ids: &[TrackId]) {
        let mut items = Vec::new();
        let mut not_connected = Vec::new();
        for track in ids.iter().filter_map(|id| self.library.track(*id)) {
            let place = self.place_of_track(track);
            match &place {
                // A remembered share that is not mounted: not connected, and nothing to touch.
                Some(p) if self.unmounted(p) => not_connected.push((track.id, FileStatus::Unavailable(p.clone()))),
                // Mounts not known yet: wait for them rather than misread an empty mount point.
                Some(p) if self.awaiting_mounts(p) => {}
                _ => items.push(CheckItem { id: track.id, location: track.location.clone(), place }),
            }
        }
        self.apply_file_statuses(not_connected);
        if !items.is_empty()
            && let Some(checker) = &self.availability.checker
        {
            checker.ask(Request::Files(items));
        }
    }

    /// Asks every place whether it is there.
    fn probe_places(&mut self) {
        self.availability.last_probe = Some(Instant::now());
        let mut places: Vec<Place> = self.library.tracks().filter_map(|t| self.place_of_track(t)).collect();
        places.sort();
        places.dedup();
        let mut ask = Vec::new();
        let mut answered = Vec::new();
        for place in places {
            if self.unmounted(&place) {
                answered.push((place, false));
            } else if !self.awaiting_mounts(&place) {
                // Mounted is not enough: the share must also answer.
                ask.push(place);
            }
        }
        self.apply_reachability(answered);
        if !ask.is_empty()
            && let Some(checker) = &self.availability.checker
        {
            checker.ask(Request::Places(ask));
        }
    }

    /// Takes in what the checker found, and asks places again when it is time.
    pub(crate) fn follow_file_checks(&mut self) {
        let answers = self.availability.checker.as_ref().map(FileChecker::answers).unwrap_or_default();
        // Answers can be overtaken by events: a share unmounted while it was being asked about
        // must not come back as connected because the question was older.
        for answer in answers {
            match answer {
                Answer::Places(places) => {
                    let places = places
                        .into_iter()
                        .map(|(place, reachable)| {
                            let reachable = reachable && !self.unmounted(&place);
                            (place, reachable)
                        })
                        .collect();
                    self.apply_reachability(places);
                }
                Answer::Files(statuses) => {
                    let statuses = statuses
                        .into_iter()
                        .map(|(id, status)| {
                            match (&status, self.library.track(id).and_then(|t| self.place_of_track(t))) {
                                (FileStatus::Present | FileStatus::Missing, Some(place)) if self.unmounted(&place) => {
                                    (id, FileStatus::Unavailable(place))
                                }
                                _ => (id, status),
                            }
                        })
                        .collect();
                    self.apply_file_statuses(statuses);
                }
            }
        }
        if self.availability.last_probe.is_none_or(|t| t.elapsed() >= PROBE_EVERY) {
            self.probe_places();
        }
    }

    fn apply_file_statuses(&mut self, statuses: Vec<(TrackId, FileStatus)>) {
        if statuses.is_empty() {
            return;
        }
        for (id, status) in statuses {
            match status {
                FileStatus::Present => self.availability.statuses.remove(&id),
                other => self.availability.statuses.insert(id, other),
            };
        }
        self.content_changed();
    }

    /// Notes which places answered. A place that went away makes its songs unavailable at once;
    /// one that came back has its songs looked for again.
    fn apply_reachability(&mut self, places: Vec<(Place, bool)>) {
        for (place, reachable) in places {
            let before = self.availability.reachable.insert(place.clone(), reachable);
            if before == Some(reachable) {
                continue;
            }
            let songs: Vec<TrackId> = self
                .library
                .tracks()
                .filter(|t| self.place_of_track(t).as_ref() == Some(&place))
                .map(|t| t.id)
                .collect();
            if reachable {
                if before == Some(false) {
                    self.notices.push(Notice::info(format!("“{}” is connected again.", place.name())));
                }
                self.check_tracks(&songs);
            } else {
                let kind = if place.network { "is not connected" } else { "cannot be found" };
                let text = match songs.len() {
                    0 => continue,
                    1 => format!("“{}” {kind}. Its song shows as unavailable until it is back.", place.name()),
                    n => format!("“{}” {kind}. Its {n} songs show as unavailable until it is back.", place.name()),
                };
                self.notices.push(Notice::warning(text));
                let unavailable = songs.into_iter().map(|id| (id, FileStatus::Unavailable(place.clone()))).collect();
                self.apply_file_statuses(unavailable);
            }
        }
    }

    fn awaiting_mounts(&self, place: &Place) -> bool {
        awaits_mounts(place, self.availability.mounts.as_deref())
    }

    fn unmounted(&self, place: &Place) -> bool {
        is_unmounted(place, self.availability.mounts.as_deref())
    }

    /// Whether a song's file is known to be gone from a place that is there, as moving it would
    /// leave it. A song on a place that is not connected is never gone.
    pub(crate) fn file_is_gone(&self, track: &Track) -> bool {
        match self.file_status(track.id) {
            FileStatus::Missing => true,
            FileStatus::Unavailable(_) => false,
            FileStatus::Present => {
                // A local disk answers at once; a network share is asked only once it is known
                // to answer, so this cannot hang.
                let safe_to_ask = self
                    .place_of_track(track)
                    .is_none_or(|p| !p.network || self.availability.reachable.get(&p) == Some(&true));
                safe_to_ask && matches!(track.location.try_exists(), Ok(false))
            }
        }
    }
}

/// Whether `place` is a network share and the mounts are not known yet.
fn awaits_mounts(place: &Place, mounts: Option<&[PathBuf]>) -> bool {
    place.network && is_mountable(&place.root) && mounts.is_none()
}

/// Whether `place` is a network share known not to be mounted now.
fn is_unmounted(place: &Place, mounts: Option<&[PathBuf]>) -> bool {
    place.network && is_mountable(&place.root) && mounts.is_some_and(|mounts| !mounts.contains(&place.root))
}

impl LibraryView<'_> {
    /// Looks for every song's file now, in the calling thread (within the same deadlines as the
    /// session's checks), given the network shares mounted now. Returns the songs that are
    /// missing or unavailable. For the command line, which has no session to keep this up.
    pub fn find_unplayable(&self, mounts: &[PathBuf]) -> Vec<(TrackId, FileStatus)> {
        let mut network = self.library.network_places().to_vec();
        network.extend(mounts.iter().cloned());
        let mut results = Vec::new();
        let mut items = Vec::new();
        for track in self.library.tracks() {
            let place = place_of(&track.location, &self.prefs.library_folders, &network);
            match place {
                Some(place) if is_unmounted(&place, Some(mounts)) => {
                    results.push((track.id, FileStatus::Unavailable(place)));
                }
                place => items.push(CheckItem { id: track.id, location: track.location.clone(), place }),
            }
        }
        results.extend(check_now(items).into_iter().filter(|(_, status)| *status != FileStatus::Present));
        results.sort_by_key(|(id, _)| *id);
        results
    }
}

/// Whether a network place is a mount point, judged by whether it is mounted. A Windows place
/// (a mapped drive letter like `Z:\`, or a `\\server\share` path) is not: a mapping can
/// outlive its server, so those are asked directly, within the deadline.
fn is_mountable(root: &std::path::Path) -> bool {
    !matches!(root.components().next(), Some(std::path::Component::Prefix(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(root: &str) -> Place {
        Place { root: PathBuf::from(root), network: true }
    }

    #[test]
    fn a_share_is_judged_by_the_mounts_once_they_are_known() {
        let share = network("/mnt/nas");
        assert!(awaits_mounts(&share, None));
        assert!(is_unmounted(&share, Some(&[])));
        assert!(!is_unmounted(&share, Some(&[PathBuf::from("/mnt/nas")])));
        let local = Place { root: PathBuf::from("/mnt/usb"), network: false };
        assert!(!is_unmounted(&local, Some(&[])) && !awaits_mounts(&local, None));
    }

    #[cfg(windows)]
    #[test]
    fn a_mapped_drive_or_server_path_is_asked_directly() {
        for place in [network(r"Z:\"), network(r"\\nas\music\")] {
            assert!(!awaits_mounts(&place, None), "{place:?}");
            assert!(!is_unmounted(&place, Some(&[])), "{place:?}");
        }
    }
}
