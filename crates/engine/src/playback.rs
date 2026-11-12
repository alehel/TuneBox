//! Playing music: the queue, the transport buttons, and play and skip counts.
//!
//! Gapless playback: the player is always told which song follows the current one
//! (`Session::sync_next`), so it can go straight on into it. When it does, it says so, and the
//! session only catches up (counts the play, moves the queue); it does not load anything.

use crate::file_check::FileStatus;
use crate::notices::Notice;
use crate::prefs::Preferences;
use crate::session::Session;
use chrono::Utc;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tunebox_model::TrackId;
use tunebox_playback::{PlayQueue, PlayState, Player, PlayerEvent, RepeatMode};

/// Previous restarts the current track rather than going back once this much has played.
const RESTART_THRESHOLD: Duration = Duration::from_secs(3);
/// Moving on before this much has played counts as a skip.
const SKIP_THRESHOLD: Duration = Duration::from_secs(20);

/// What the LCD shows.
#[derive(Clone, Debug, PartialEq)]
pub struct NowPlaying {
    pub track: TrackId,
    pub state: PlayState,
    pub position: Duration,
    pub duration: Duration,
}

pub(crate) struct PlaybackState {
    player: Player,
    queue: Option<PlayQueue>,
    shuffle: bool,
    repeat: RepeatMode,
    /// Tracks that failed to play in a row, to stop a queue of unplayable files spinning.
    failures_in_a_row: usize,
    /// The file the player was last told to play, or went on to. Events about another file
    /// are stale: they were sent before the latest load.
    playing_path: Option<PathBuf>,
    /// The song the player was last told to go on to, until it does (or drops it).
    next_sent: Option<(TrackId, PathBuf)>,
    /// Songs in this queue whose file could not be opened, so they are not queued again.
    unplayable: HashSet<TrackId>,
}

impl PlaybackState {
    pub fn new(player: Player, prefs: &Preferences) -> Self {
        player.set_volume(prefs.volume);
        Self {
            player,
            queue: None,
            shuffle: prefs.shuffle,
            repeat: prefs.repeat,
            failures_in_a_row: 0,
            playing_path: None,
            next_sent: None,
            unplayable: HashSet::new(),
        }
    }

    pub fn apply_preferences(&mut self, prefs: &Preferences) {
        self.player.set_volume(prefs.volume);
    }

    pub fn current(&self) -> Option<TrackId> {
        self.queue.as_ref().and_then(PlayQueue::current)
    }

    pub fn stop(&mut self) {
        self.player.stop();
        self.queue = None;
        self.playing_path = None;
        self.next_sent = None;
    }

    /// Plays `path` from `from`, outside the queue: an audiobook's staged track, heard while
    /// editing. Its events are ignored, as for any file the queue did not load.
    pub(crate) fn play_file_from(&mut self, path: PathBuf, from: Duration, then: Option<PathBuf>) {
        self.stop();
        self.player.load(path, true);
        self.player.seek(from);
        // Straight on into the next file, from its start, as gapless playback does.
        self.player.set_next(then);
    }

    /// Where the player is, and in which file.
    pub(crate) fn position(&self) -> (Option<PathBuf>, Duration, PlayState) {
        let status = self.player.status();
        (status.path, status.position, status.state)
    }
}

fn shuffle_seed() -> u64 {
    Utc::now().timestamp_nanos_opt().unwrap_or_default().unsigned_abs()
}

impl Session {
    /// Plays `list` from `list[start]` (double-clicking a track). Unchecked tracks are skipped,
    /// except the one started explicitly, and so are songs whose file is missing or on a drive
    /// or share that is not connected: each would only fail. Choosing one of those says why it
    /// cannot play.
    pub fn play_tracks(&mut self, list: &[TrackId], start: usize) {
        let Some(&chosen) = list.get(start) else { return };
        if let Some(problem) = self.file_problem(chosen) {
            let title = self.library.track(chosen).map(|t| t.title().to_owned()).unwrap_or_default();
            self.notices.push(Notice::warning(format!("“{title}” cannot be played. {problem}")));
            return;
        }
        let playable: Vec<TrackId> = list
            .iter()
            .copied()
            .filter(|id| {
                *id == chosen
                    || (self.library.track(*id).is_some_and(|t| t.checked)
                        && *self.file_status(*id) == FileStatus::Present)
            })
            .collect();
        let start = playable.iter().position(|id| *id == chosen).unwrap_or(0);
        let state = &mut self.playback;
        state.queue = PlayQueue::new(playable, start, state.shuffle, state.repeat, shuffle_seed());
        state.failures_in_a_row = 0;
        state.unplayable.clear();
        self.load_current(true);
    }

    /// Loads the current song, cutting off whatever played, and queues the one after it.
    fn load_current(&mut self, play: bool) {
        match self.playback.current().and_then(|id| self.library.track(id)) {
            Some(track) => {
                let path = track.location.clone();
                self.playback.player.load(path.clone(), play);
                self.playback.playing_path = Some(path);
                // A load clears what the player had queued.
                self.playback.next_sent = None;
                self.sync_next();
            }
            None => self.stop(),
        }
    }

    /// Tells the player which song follows the current one, when that changed. Called every
    /// tick, so whatever changes it (shuffle, repeat, a song unchecked, a file gone missing or
    /// its drive disconnected) reaches the player without each of those having to say so.
    pub(crate) fn sync_next(&mut self) {
        let wanted = self.up_next().and_then(|id| self.library.track(id).map(|t| (id, t.location.clone())));
        if wanted != self.playback.next_sent {
            self.playback.player.set_next(wanted.as_ref().map(|(_, path)| path.clone()));
            self.playback.next_sent = wanted;
        }
    }

    /// The song that plays when the current one ends by itself.
    fn up_next(&self) -> Option<TrackId> {
        let queue = self.playback.queue.as_ref()?;
        let current = queue.current();
        queue.peek_after_finish(|id| self.can_follow(id, current))
    }

    /// Whether a song may play by itself after another: still in the library, checked, its file
    /// there, and not already failed in this queue. The song playing is exempt from its check
    /// box: the listener may have started it unchecked, and Repeat One then repeats it.
    fn can_follow(&self, id: TrackId, current: Option<TrackId>) -> bool {
        self.library.track(id).is_some_and(|t| t.checked || Some(id) == current)
            && *self.file_status(id) == FileStatus::Present
            && !self.playback.unplayable.contains(&id)
    }

    /// The Play/Pause button. Returns `false` when nothing is loaded, so the frontend can start
    /// the selection instead.
    pub fn play_pause(&mut self) -> bool {
        if self.playback.queue.is_none() {
            return false;
        }
        match self.playback.player.status().state {
            PlayState::Playing => self.playback.player.pause(),
            PlayState::Paused => self.playback.player.play(),
            PlayState::Stopped => self.load_current(true),
        }
        true
    }

    pub fn stop(&mut self) {
        self.playback.stop();
    }

    /// The Next button.
    pub fn next_track(&mut self) {
        self.note_skip();
        let next = self.playback.queue.as_mut().and_then(PlayQueue::skip_forward);
        if next.is_some() { self.load_current(true) } else { self.stop() }
    }

    /// The Previous button: back to the start of the track, or to the track before if it has
    /// only just started.
    pub fn previous_track(&mut self) {
        if self.playback.player.status().position > RESTART_THRESHOLD {
            self.playback.player.seek(Duration::ZERO);
            return;
        }
        if self.playback.queue.as_mut().and_then(PlayQueue::skip_back).is_some() {
            self.load_current(true);
        }
    }

    pub fn seek(&mut self, position: Duration) {
        self.playback.player.seek(position);
    }

    pub fn volume(&self) -> f32 {
        self.prefs.volume
    }

    /// Sets the volume (0–1). Remembered in preferences.
    pub fn set_volume(&mut self, volume: f32) {
        self.playback.player.set_volume(volume);
        self.prefs.volume = self.playback.player.volume();
    }

    pub fn shuffle(&self) -> bool {
        self.playback.shuffle
    }

    pub fn set_shuffle(&mut self, on: bool) {
        self.playback.shuffle = on;
        if let Some(queue) = self.playback.queue.as_mut() {
            queue.set_shuffle(on, shuffle_seed());
        }
        self.sync_next();
        self.prefs.shuffle = on;
        self.save_preferences();
    }

    pub fn repeat(&self) -> RepeatMode {
        self.playback.repeat
    }

    /// The repeat button: off → all → one → off.
    pub fn cycle_repeat(&mut self) {
        let repeat = self.playback.repeat.cycled();
        self.playback.repeat = repeat;
        if let Some(queue) = self.playback.queue.as_mut() {
            queue.set_repeat(repeat);
        }
        self.sync_next();
        self.prefs.repeat = repeat;
        self.save_preferences();
    }

    /// The last `frames` samples played (mono), for visualizers. `None` unless playing.
    pub fn recent_audio(&self, frames: usize) -> Option<tunebox_playback::AudioTap> {
        self.playback.player.recent_audio(frames)
    }

    pub fn now_playing(&self) -> Option<NowPlaying> {
        let track = self.playback.current()?;
        let status = self.playback.player.status();
        let duration =
            status.duration.or_else(|| self.library.track(track).map(|t| t.file.duration)).unwrap_or_default();
        Some(NowPlaying { track, state: status.state, position: status.position, duration })
    }

    /// Counts a skip when the listener leaves a track early.
    fn note_skip(&mut self) {
        let Some(current) = self.playback.current() else { return };
        let status = self.playback.player.status();
        if status.state != PlayState::Stopped && status.position < SKIP_THRESHOLD {
            let _ = self.library.record_skip(current, Utc::now());
            self.library_changed();
        }
    }

    /// Reacts to the player: a finished song counts as played and the queue moves on (to the
    /// song the player already went on to, or by loading the next); a song that cannot be
    /// played is reported and skipped.
    pub(crate) fn follow_player(&mut self) {
        for event in self.playback.player.poll_events() {
            match event {
                PlayerEvent::Finished { path, started } => {
                    if self.playback.playing_path.as_ref() != Some(&path) {
                        continue; // From before the latest load.
                    }
                    self.playback.failures_in_a_row = 0;
                    if let Some(current) = self.playback.current() {
                        let _ = self.library.record_play(current, Utc::now());
                        self.library_changed();
                    }
                    match started {
                        Some(started) => self.went_on_to(started),
                        None => self.advance_after_finish(),
                    }
                }
                PlayerEvent::Failed { path, message, upcoming: true } => {
                    // Only the song still queued: an earlier one was replaced and is no concern.
                    let failed = self.playback.next_sent.take_if(|(_, sent)| Some(&*sent) == path.as_ref());
                    if let Some((id, _)) = failed {
                        self.playback.unplayable.insert(id);
                        self.check_tracks(&[id]);
                        self.report_failure(path, message);
                    }
                }
                PlayerEvent::Failed { path, message, upcoming: false } => {
                    // Perhaps the file went, perhaps its share dropped: find out which.
                    if let Some(current) = self.playback.current() {
                        self.check_tracks(&[current]);
                        self.playback.unplayable.insert(current);
                    }
                    self.report_failure(path.clone(), message);
                    // The player stopped, and dropped what it had queued.
                    self.playback.next_sent = None;
                    self.playback.failures_in_a_row += 1;
                    let queue_len = self.playback.queue.as_ref().map_or(0, |q| q.tracks().len());
                    if path.is_some() && self.playback.failures_in_a_row < queue_len {
                        self.advance_after_finish();
                    } else {
                        self.stop();
                    }
                }
            }
        }
        // After the events, so a song that just failed is passed over.
        self.sync_next();
    }

    fn report_failure(&mut self, path: Option<PathBuf>, message: String) {
        let name = path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        self.notices.push(Notice::error(match name {
            Some(name) => format!("Could not play “{name}”: {message}"),
            None => message,
        }));
    }

    /// The player went on by itself to `started`, the song it was told would follow: the queue
    /// catches up, and nothing is loaded.
    fn went_on_to(&mut self, started: PathBuf) {
        let sent = self.playback.next_sent.take();
        // Normally the song last sent. If the queue changed after the player had committed to
        // the song it sent before, find that one by its file.
        let id = match sent {
            Some((id, path)) if path == started => Some(id),
            _ => self.playback.queue.as_ref().and_then(|q| {
                q.tracks().iter().copied().find(|id| self.library.track(*id).is_some_and(|t| t.location == started))
            }),
        };
        let moved = match (self.playback.queue.as_mut(), id) {
            (Some(queue), Some(id)) => queue.move_to(id),
            _ => false,
        };
        if moved {
            self.playback.playing_path = Some(started);
        } else {
            // Playing a song the queue does not hold: nothing sensible to show, so stop.
            self.stop();
        }
    }

    /// Moves to the song after the current one and loads it, or stops at the end of the queue.
    fn advance_after_finish(&mut self) {
        let current = self.playback.current();
        let mut queue = self.playback.queue.take();
        let next = queue.as_mut().and_then(|q| q.advance_after_finish(|id| self.can_follow(id, current)));
        self.playback.queue = queue;
        if next.is_some() { self.load_current(true) } else { self.stop() }
    }
}
