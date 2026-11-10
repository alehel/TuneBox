//! Playing music: the queue, the transport buttons, and play and skip counts.

use crate::notices::Notice;
use crate::prefs::Preferences;
use crate::session::Session;
use chrono::Utc;
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
}

impl PlaybackState {
    pub fn new(player: Player, prefs: &Preferences) -> Self {
        player.set_volume(prefs.volume);
        Self { player, queue: None, shuffle: prefs.shuffle, repeat: prefs.repeat, failures_in_a_row: 0 }
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
    }
}

fn shuffle_seed() -> u64 {
    Utc::now().timestamp_nanos_opt().unwrap_or_default().unsigned_abs()
}

impl Session {
    /// Plays `list` from `list[start]` (double-clicking a track). Unchecked tracks are skipped,
    /// except the one started explicitly.
    pub fn play_tracks(&mut self, list: &[TrackId], start: usize) {
        let Some(&chosen) = list.get(start) else { return };
        let playable: Vec<TrackId> = list
            .iter()
            .copied()
            .filter(|id| *id == chosen || self.library.track(*id).is_some_and(|t| t.checked))
            .collect();
        let start = playable.iter().position(|id| *id == chosen).unwrap_or(0);
        let state = &mut self.playback;
        state.queue = PlayQueue::new(playable, start, state.shuffle, state.repeat, shuffle_seed());
        state.failures_in_a_row = 0;
        self.load_current(true);
    }

    fn load_current(&mut self, play: bool) {
        match self.playback.current().and_then(|id| self.library.track(id)) {
            Some(track) => self.playback.player.load(track.location.clone(), play),
            None => self.stop(),
        }
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
        self.prefs.repeat = repeat;
        self.save_preferences();
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

    /// Reacts to the player: a finished track counts as played and the queue moves on; a track
    /// that cannot be played is reported and skipped.
    pub(crate) fn follow_player(&mut self) {
        for event in self.playback.player.poll_events() {
            match event {
                PlayerEvent::Finished(_) => {
                    self.playback.failures_in_a_row = 0;
                    if let Some(current) = self.playback.current() {
                        let _ = self.library.record_play(current, Utc::now());
                        self.library_changed();
                    }
                    self.advance_after_finish();
                }
                PlayerEvent::Failed { path, message } => {
                    let name = path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
                    self.notices.push(Notice::error(match name {
                        Some(name) => format!("Could not play “{name}”: {message}"),
                        None => message,
                    }));
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
    }

    fn advance_after_finish(&mut self) {
        let next = self.playback.queue.as_mut().and_then(PlayQueue::advance_after_finish);
        if next.is_some() { self.load_current(true) } else { self.stop() }
    }
}
