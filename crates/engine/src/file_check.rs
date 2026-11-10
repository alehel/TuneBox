//! Looking for songs' files without ever hanging. A network share that stops answering can
//! block a file system call for minutes (or for good, on a stale NFS mount), so every probe runs
//! on a helper thread with a deadline: a place that does not answer in time counts as not
//! connected. A place whose helper is still stuck is not asked again until it comes back, so a
//! dead share costs one stuck thread, not one per check.
//!
//! [`FileChecker`] does this on a thread of its own for the session; [`check_now`] does it in
//! the calling thread for the command line.

use crate::places::Place;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tunebox_model::TrackId;

/// How long a place may take to answer before it counts as not connected.
const DEADLINE: Duration = Duration::from_secs(4);

/// Whether a song's file can be found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileStatus {
    /// Where the library says it is (or not checked yet).
    Present,
    /// The place it is on is there, but the file is not: moved, renamed or deleted.
    Missing,
    /// The place it is on cannot be reached: a share not connected, a drive unplugged. Nothing
    /// is wrong with the song; it is back when the place is.
    Unavailable(Place),
}

/// A song to look for.
#[derive(Clone, Debug)]
pub(crate) struct CheckItem {
    pub id: TrackId,
    pub location: PathBuf,
    pub place: Option<Place>,
}

pub(crate) enum Request {
    /// Whether each place can be reached.
    Places(Vec<Place>),
    Files(Vec<CheckItem>),
}

pub(crate) enum Answer {
    Places(Vec<(Place, bool)>),
    Files(Vec<(TrackId, FileStatus)>),
}

/// Places whose helper thread has not come back.
#[derive(Clone, Default)]
struct Stuck(Arc<Mutex<HashSet<PathBuf>>>);

impl Stuck {
    fn contains(&self, key: &Path) -> bool {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).contains(key)
    }
    fn insert(&self, key: PathBuf) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).insert(key);
    }
    fn remove(&self, key: &Path) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).remove(key);
    }
}

/// The session's checker: requests in, answers out, on a thread of its own.
pub(crate) struct FileChecker {
    requests: Sender<Request>,
    answers: Receiver<Answer>,
}

impl FileChecker {
    /// `None` if the thread cannot be started; songs then simply show no indicator.
    pub fn start() -> Option<Self> {
        let (requests, inbox) = mpsc::channel::<Request>();
        let (outbox, answers) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("tunebox-file-check".into()).spawn(move || {
            let stuck = Stuck::default();
            for request in inbox {
                let answer = match request {
                    Request::Places(places) => {
                        Answer::Places(places.into_iter().map(|p| (p.clone(), reachable(&p.root, &stuck))).collect())
                    }
                    Request::Files(items) => Answer::Files(check(items, &stuck)),
                };
                if outbox.send(answer).is_err() {
                    break;
                }
            }
        });
        match spawned {
            Ok(_) => Some(FileChecker { requests, answers }),
            Err(e) => {
                log::warn!("cannot check for missing files: {e}");
                None
            }
        }
    }

    pub fn ask(&self, request: Request) {
        // The thread outlives the session's use of it; a send fails only if it died.
        let _ = self.requests.send(request);
    }

    pub fn answers(&self) -> Vec<Answer> {
        self.answers.try_iter().collect()
    }
}

/// Looks for `items` in the calling thread, within the same deadlines.
pub(crate) fn check_now(items: Vec<CheckItem>) -> Vec<(TrackId, FileStatus)> {
    check(items, &Stuck::default())
}

/// Whether `root` answers in time.
fn reachable(root: &Path, stuck: &Stuck) -> bool {
    if stuck.contains(root) {
        return false;
    }
    let (sender, answer) = mpsc::channel();
    let path = root.to_path_buf();
    let helper_stuck = stuck.clone();
    stuck.insert(path.clone());
    let spawned = std::thread::Builder::new().name("tunebox-probe".into()).spawn(move || {
        let is_there = path.is_dir();
        helper_stuck.remove(&path);
        let _ = sender.send(is_there);
    });
    if spawned.is_err() {
        stuck.remove(root);
        return false;
    }
    answer.recv_timeout(DEADLINE).unwrap_or(false)
}

/// Looks for each file, place by place: an unreachable place makes all its songs unavailable
/// without touching them.
fn check(items: Vec<CheckItem>, stuck: &Stuck) -> Vec<(TrackId, FileStatus)> {
    let mut by_place: BTreeMap<Option<Place>, Vec<(TrackId, PathBuf)>> = BTreeMap::new();
    for item in items {
        by_place.entry(item.place).or_default().push((item.id, item.location));
    }
    let mut results = Vec::new();
    for (place, songs) in by_place {
        match &place {
            Some(place) if !reachable(&place.root, stuck) => {
                results.extend(songs.into_iter().map(|(id, _)| (id, FileStatus::Unavailable(place.clone()))));
            }
            _ => results.extend(look_for(place.as_ref(), songs, stuck)),
        }
    }
    results
}

/// What a helper looking for files says.
enum Found {
    File(TrackId, std::io::Result<bool>),
    Done,
}

/// Looks for files on a helper thread, giving each one the deadline. If the place stops
/// answering part way, the rest are unavailable (or left unknown, for the system disk).
fn look_for(place: Option<&Place>, songs: Vec<(TrackId, PathBuf)>, stuck: &Stuck) -> Vec<(TrackId, FileStatus)> {
    let key = place.map(|p| p.root.clone()).unwrap_or_default();
    let (sender, found) = mpsc::channel();
    let helper_stuck = stuck.clone();
    let helper_key = key.clone();
    let paths: Vec<(TrackId, PathBuf)> = songs.clone();
    stuck.insert(key.clone());
    let spawned = std::thread::Builder::new().name("tunebox-file-look".into()).spawn(move || {
        for (id, path) in paths {
            if sender.send(Found::File(id, path.try_exists())).is_err() {
                break;
            }
        }
        // Released before saying so: the next question about this place must not find it
        // still marked stuck and take it for not connected.
        helper_stuck.remove(&helper_key);
        let _ = sender.send(Found::Done);
    });
    if spawned.is_err() {
        stuck.remove(&key);
        return Vec::new();
    }
    let mut results = Vec::with_capacity(songs.len());
    // Until the helper is done, or a file takes longer than the deadline.
    while let Ok(Found::File(id, exists)) = found.recv_timeout(DEADLINE) {
        let status = match (exists, place) {
            (Ok(true), _) => FileStatus::Present,
            (Ok(false), _) => FileStatus::Missing,
            // An I/O error (a share gone stale) is not proof the file is gone.
            (Err(_), Some(place)) => FileStatus::Unavailable(place.clone()),
            (Err(_), None) => FileStatus::Present,
        };
        results.push((id, status));
    }
    if results.len() < songs.len()
        && let Some(place) = place
    {
        let answered: HashSet<TrackId> = results.iter().map(|(id, _)| *id).collect();
        let rest = songs.into_iter().filter(|(id, _)| !answered.contains(id));
        results.extend(rest.map(|(id, _)| (id, FileStatus::Unavailable(place.clone()))));
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_needs_a_reachable_place_and_an_unreachable_one_touches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let here = dir.path().join("here.flac");
        std::fs::write(&here, b"x").unwrap();
        let local = Place { root: dir.path().to_path_buf(), network: false };
        let share = Place { root: dir.path().join("not mounted"), network: true };
        let item =
            |id, location: PathBuf, place: &Place| CheckItem { id: TrackId(id), location, place: Some(place.clone()) };
        let mut results = check_now(vec![
            item(1, here, &local),
            item(2, dir.path().join("deleted.flac"), &local),
            item(3, share.root.join("a.flac"), &share),
            CheckItem { id: TrackId(4), location: dir.path().join("nowhere.flac"), place: None },
        ]);
        results.sort_by_key(|(id, _)| *id);
        assert_eq!(
            results,
            [
                (TrackId(1), FileStatus::Present),
                (TrackId(2), FileStatus::Missing),
                (TrackId(3), FileStatus::Unavailable(share)),
                (TrackId(4), FileStatus::Missing),
            ]
        );
    }

    #[test]
    fn a_place_just_checked_is_not_taken_for_stuck() {
        // Once, the helper let go of the place after its last answer, so the next question
        // sometimes found it still marked and called a healthy place not connected.
        let dir = tempfile::tempdir().unwrap();
        let place = Place { root: dir.path().to_path_buf(), network: true };
        let stuck = Stuck::default();
        for round in 0..200 {
            let item = CheckItem { id: TrackId(1), location: dir.path().join("a.flac"), place: Some(place.clone()) };
            check(vec![item], &stuck);
            assert!(reachable(&place.root, &stuck), "round {round}");
        }
    }

    #[test]
    fn a_place_that_is_still_stuck_is_not_asked_again() {
        let dir = tempfile::tempdir().unwrap();
        let stuck = Stuck::default();
        stuck.insert(dir.path().to_path_buf());
        assert!(!reachable(dir.path(), &stuck), "treated as not connected without a new probe");
        stuck.remove(dir.path());
        assert!(reachable(dir.path(), &stuck));
    }
}
