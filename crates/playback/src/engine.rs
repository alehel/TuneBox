//! The player thread: decodes ahead of the output and keeps the status current.

use crate::convert::Converter;
use crate::output::{OutputFactory, OutputFormat, Shared};
use crate::player::{PlayState, PlayerEvent, Status};
use rtrb::{Producer, RingBuffer};
use std::any::Any;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tunebox_audio::Decoder;

/// How much audio is decoded ahead of the output. Enough to ride out a slow disk; small enough
/// that pause and seek feel immediate.
const BUFFER_SECONDS: f64 = 0.5;
/// How long the thread waits for commands when it has nothing to decode.
const IDLE_WAIT: Duration = Duration::from_millis(20);
/// How long it waits when it is keeping the buffer topped up.
const BUSY_WAIT: Duration = Duration::from_millis(5);

pub(crate) enum Command {
    Load { path: PathBuf, play: bool },
    Play,
    Pause,
    Stop,
    Seek(Duration),
    Shutdown,
}

/// The output currently running.
struct ActiveOutput {
    format: OutputFormat,
    producer: Producer<f32>,
    capacity: usize,
    /// Keeps the stream alive; dropping it stops the output.
    _stream: Box<dyn Any>,
}

/// The file being played.
struct Current {
    path: PathBuf,
    decoder: Decoder,
    converter: Converter,
    /// Converted samples waiting for room in the ring.
    pending: Vec<f32>,
    pending_from: usize,
    decoder_done: bool,
    /// Where the output's frame count starts from (changes on seek).
    base: Duration,
    duration: Option<Duration>,
}

pub(crate) struct Engine {
    factory: Box<dyn OutputFactory>,
    shared: Arc<Shared>,
    status: Arc<Mutex<Status>>,
    events: Sender<PlayerEvent>,
    output: Option<ActiveOutput>,
    current: Option<Current>,
}

impl Engine {
    pub fn new(
        factory: Box<dyn OutputFactory>,
        shared: Arc<Shared>,
        status: Arc<Mutex<Status>>,
        events: Sender<PlayerEvent>,
    ) -> Self {
        Self { factory, shared, status, events, output: None, current: None }
    }

    pub fn run(mut self, commands: Receiver<Command>) {
        loop {
            let wait = if self.current.is_some() { BUSY_WAIT } else { IDLE_WAIT };
            match commands.recv_timeout(wait) {
                Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(command) => self.handle(command),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.pump();
            self.publish();
        }
        self.shared.playing.store(false, Ordering::Relaxed);
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Load { path, play } => self.load(path, play),
            Command::Play => self.shared.playing.store(self.current.is_some(), Ordering::Relaxed),
            Command::Pause => self.shared.playing.store(false, Ordering::Relaxed),
            Command::Stop => self.stop(),
            Command::Seek(position) => self.seek(position),
            Command::Shutdown => {}
        }
    }

    fn load(&mut self, path: PathBuf, play: bool) {
        self.stop();
        match self.open(&path) {
            Ok(current) => {
                self.current = Some(current);
                self.shared.playing.store(play, Ordering::Relaxed);
            }
            Err(message) => self.fail(Some(path), message),
        }
    }

    /// Opens `path` and makes sure an output runs in a format it can be converted to.
    fn open(&mut self, path: &std::path::Path) -> Result<Current, String> {
        let decoder = Decoder::open(path).map_err(|e| e.to_string())?;
        let wanted = OutputFormat { sample_rate: decoder.sample_rate(), channels: decoder.channels() };
        let format = self.factory.negotiate(wanted)?;
        if self.output.as_ref().is_none_or(|o| o.format != format) {
            self.output = None;
            self.output = Some(self.start_output(format)?);
        }
        let converter = Converter::new(wanted.sample_rate, wanted.channels, format.sample_rate, format.channels);
        Ok(Current {
            path: path.to_path_buf(),
            duration: decoder.duration(),
            decoder,
            converter,
            pending: Vec::new(),
            pending_from: 0,
            decoder_done: false,
            base: Duration::ZERO,
        })
    }

    fn start_output(&mut self, format: OutputFormat) -> Result<ActiveOutput, String> {
        let capacity =
            ((f64::from(format.sample_rate) * BUFFER_SECONDS) as usize).max(1024) * usize::from(format.channels.max(1));
        let (producer, consumer) = RingBuffer::new(capacity);
        let stream = self.factory.start(format, consumer, Arc::clone(&self.shared))?;
        self.shared.output_rate.store(format.sample_rate, Ordering::Relaxed);
        Ok(ActiveOutput { format, producer, capacity, _stream: stream })
    }

    fn stop(&mut self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        self.current = None;
        self.flush_output();
    }

    fn seek(&mut self, position: Duration) {
        let Some(current) = self.current.as_mut() else { return };
        match current.decoder.seek(position) {
            Ok(landed) => {
                current.base = landed;
                current.pending.clear();
                current.pending_from = 0;
                current.decoder_done = false;
                self.flush_output();
            }
            Err(e) => log::warn!("seek failed: {e}"),
        }
    }

    /// Discards everything queued for the output and restarts the played-frame count. Waits
    /// briefly for the callback to do the discarding, so no stale audio follows.
    fn flush_output(&mut self) {
        if self.output.is_some() {
            self.shared.flush.store(true, Ordering::Release);
            let deadline = Instant::now() + Duration::from_millis(250);
            while self.shared.flush.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        self.shared.played_frames.store(0, Ordering::Relaxed);
    }

    /// Decodes until the ring is full, and notices when a track has finished.
    fn pump(&mut self) {
        let (Some(current), Some(output)) = (self.current.as_mut(), self.output.as_mut()) else { return };
        if !self.shared.playing.load(Ordering::Relaxed) {
            return;
        }
        loop {
            if current.pending_from < current.pending.len() {
                let (written, _) = output.producer.push_partial_slice(&current.pending[current.pending_from..]);
                current.pending_from += written.len();
                if current.pending_from < current.pending.len() {
                    return; // The ring is full.
                }
            } else if current.decoder_done {
                break;
            } else {
                match current.decoder.next_block() {
                    Ok(Some(block)) => {
                        current.pending.clear();
                        current.pending_from = 0;
                        current.converter.process(&block.samples, &mut current.pending);
                    }
                    Ok(None) => current.decoder_done = true,
                    Err(e) => {
                        let (path, message) = (current.path.clone(), e.to_string());
                        self.stop();
                        self.fail(Some(path), message);
                        return;
                    }
                }
            }
        }
        // Decoded to the end: finished once the output has played everything queued.
        if output.producer.slots() == output.capacity {
            let path = current.path.clone();
            self.stop();
            let _ = self.events.send(PlayerEvent::Finished(path));
        }
    }

    fn fail(&mut self, path: Option<PathBuf>, message: String) {
        log::warn!("playback failed: {message}");
        let _ = self.events.send(PlayerEvent::Failed { path, message });
    }

    fn publish(&self) {
        let status = match (&self.current, &self.output) {
            (Some(current), Some(output)) => {
                let played = self.shared.played_frames.load(Ordering::Relaxed);
                let elapsed = Duration::from_secs_f64(played as f64 / f64::from(output.format.sample_rate.max(1)));
                let position = current.base.saturating_add(elapsed);
                Status {
                    state: if self.shared.playing.load(Ordering::Relaxed) {
                        PlayState::Playing
                    } else {
                        PlayState::Paused
                    },
                    path: Some(current.path.clone()),
                    position: current.duration.map_or(position, |d| position.min(d)),
                    duration: current.duration,
                }
            }
            _ => Status::default(),
        };
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }
}

#[cfg(test)]
mod tests {
    use crate::{NullOutput, PlayState, Player, PlayerEvent};
    use std::path::Path;
    use std::time::{Duration, Instant};
    use tunebox_audio::{LosslessFormat, PcmWriter};

    fn tone(path: &Path, seconds: usize, rate: u32) {
        let frames = seconds * rate as usize;
        let wave: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.03).sin() * 0.3).collect();
        let mut writer = PcmWriter::create(path, LosslessFormat::Wav, rate, 1).unwrap();
        writer.write(&[wave]).unwrap();
        writer.finish().unwrap();
    }

    fn wait_for(player: &Player, timeout: Duration, mut done: impl FnMut(&[PlayerEvent]) -> bool) -> Vec<PlayerEvent> {
        let start = Instant::now();
        let mut all = Vec::new();
        while start.elapsed() < timeout {
            all.extend(player.poll_events());
            if done(&all) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        all
    }

    #[test]
    fn plays_a_file_to_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        tone(&path, 2, 22_050);
        let player = Player::with_output(NullOutput { speed: 20.0 });
        player.load(path.clone(), true);
        let events = wait_for(&player, Duration::from_secs(5), |e| !e.is_empty());
        assert_eq!(events, [PlayerEvent::Finished(path)]);
        assert_eq!(player.status().state, PlayState::Stopped);
    }

    #[test]
    fn pause_holds_position_and_seek_moves_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        tone(&path, 10, 8_000);
        let player = Player::with_output(NullOutput { speed: 1.0 });
        player.load(path, false);
        std::thread::sleep(Duration::from_millis(100));
        let status = player.status();
        assert_eq!(status.state, PlayState::Paused);
        assert_eq!(status.position, Duration::ZERO);
        assert_eq!(status.duration.map(|d| d.as_secs()), Some(10));

        player.seek(Duration::from_secs(6));
        player.play();
        std::thread::sleep(Duration::from_millis(150));
        let position = player.status().position;
        assert!(position >= Duration::from_secs(6) && position < Duration::from_secs(7), "{position:?}");

        // What was played reaches the visualizer tap.
        let tap = player.recent_audio(512).unwrap();
        assert_eq!((tap.samples.len(), tap.sample_rate), (512, 8_000));
        assert!(tap.samples.iter().any(|s| s.abs() > 0.1));
    }

    #[test]
    fn a_bad_file_reports_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.mp3");
        std::fs::write(&path, b"nope").unwrap();
        let player = Player::with_output(NullOutput::default());
        player.load(path.clone(), true);
        let events = wait_for(&player, Duration::from_secs(2), |e| !e.is_empty());
        assert!(matches!(&events[..], [PlayerEvent::Failed { path: Some(p), .. }] if *p == path));
    }
}
