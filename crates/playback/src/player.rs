//! The handle the rest of TuneBox holds: send commands, read status, collect events.

use crate::engine::{Command, Engine};
use crate::output::{OutputFactory, Shared};
use rtrb::Consumer;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

/// What the player is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// A snapshot of the player, refreshed continuously by the player thread.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub state: PlayState,
    /// The file loaded, if any.
    pub path: Option<PathBuf>,
    pub position: Duration,
    pub duration: Option<Duration>,
}

/// Something that happened, for the owner to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerEvent {
    /// The loaded file played to its end.
    Finished(PathBuf),
    /// A file could not be played, or the output failed.
    Failed { path: Option<PathBuf>, message: String },
}

/// The most recent audio that was played, for visualizers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioTap {
    /// Mono samples, oldest first.
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// The reading end of the visualizer tap and the last few thousand samples read from it.
struct TapReader {
    reader: Consumer<f32>,
    recent: VecDeque<f32>,
}

/// How many recent samples are kept for [`Player::recent_audio`].
const RECENT_SAMPLES: usize = 8192;

/// Plays audio files on a dedicated thread.
pub struct Player {
    commands: Sender<Command>,
    events: Receiver<PlayerEvent>,
    status: Arc<Mutex<Status>>,
    shared: Arc<Shared>,
    tap: Mutex<TapReader>,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// A player using the system's sound output.
    #[cfg(feature = "cpal")]
    pub fn new() -> Self {
        Self::with_output(crate::CpalOutput::default())
    }

    /// A player using `output`.
    pub fn with_output(output: impl OutputFactory) -> Self {
        let (commands, command_rx) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let (shared, tap_reader) = Shared::new();
        let shared = Arc::new(shared);
        let tap = Mutex::new(TapReader { reader: tap_reader, recent: VecDeque::with_capacity(RECENT_SAMPLES) });
        let (engine_shared, engine_status) = (Arc::clone(&shared), Arc::clone(&status));
        let factory: Box<dyn OutputFactory> = Box::new(output);
        // The engine is built on its own thread: the output stream it will own must not move.
        let thread = std::thread::Builder::new()
            .name("tunebox-player".into())
            .spawn(move || Engine::new(factory, engine_shared, engine_status, event_tx).run(command_rx))
            .map_err(|e| log::error!("could not start the player thread: {e}"))
            .ok();
        Self { commands, events, status, shared, tap, thread }
    }

    fn send(&self, command: Command) {
        // Only fails once the player thread has gone, which `poll_events` reports.
        let _ = self.commands.send(command);
    }

    /// Loads `path`, replacing whatever was loaded, and starts playing it if `play` is set.
    pub fn load(&self, path: PathBuf, play: bool) {
        self.send(Command::Load { path, play });
    }

    pub fn play(&self) {
        self.send(Command::Play);
    }

    pub fn pause(&self) {
        self.send(Command::Pause);
    }

    pub fn stop(&self) {
        self.send(Command::Stop);
    }

    pub fn seek(&self, position: Duration) {
        self.send(Command::Seek(position));
    }

    /// Sets the volume, 0–1. Takes effect immediately.
    pub fn set_volume(&self, volume: f32) {
        self.shared.set_volume(volume);
    }

    pub fn volume(&self) -> f32 {
        self.shared.volume()
    }

    /// Up to `frames` of the audio played most recently (mono, before the volume), or `None`
    /// when nothing is playing.
    pub fn recent_audio(&self, frames: usize) -> Option<AudioTap> {
        let mut tap = self.tap.lock().unwrap_or_else(PoisonError::into_inner);
        let TapReader { reader, recent } = &mut *tap;
        while let Ok(sample) = reader.pop() {
            if recent.len() == RECENT_SAMPLES {
                recent.pop_front();
            }
            recent.push_back(sample);
        }
        if !self.shared.playing.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        let sample_rate = self.shared.output_rate.load(std::sync::atomic::Ordering::Relaxed);
        let skip = recent.len().saturating_sub(frames);
        Some(AudioTap { samples: recent.iter().skip(skip).copied().collect(), sample_rate })
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Events since the last call, oldest first.
    pub fn poll_events(&self) -> Vec<PlayerEvent> {
        let mut events: Vec<PlayerEvent> = self.events.try_iter().collect();
        if self.thread.as_ref().is_none_or(JoinHandle::is_finished) && events.is_empty() {
            events.push(PlayerEvent::Failed { path: None, message: "The audio player stopped unexpectedly.".into() });
        }
        events
    }
}

#[cfg(feature = "cpal")]
impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
