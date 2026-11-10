//! The player thread: decodes ahead of the output, goes straight on from one song into the
//! next, and keeps the status current.
//!
//! Gapless playback: the song queued with [`Command::Next`] is opened early (see `upcoming`).
//! When the song being decoded ends, the next one's audio follows it into the same ring with no
//! stop and no flush, and the output frame where it begins is noted. Decoding runs about half a
//! second ahead of the output, so the status and the [`PlayerEvent::Finished`] hand-over switch
//! songs when the output reaches that frame, not when decoding does. Encoder delay and padding
//! (MP3, AAC, Opus) are trimmed by the decoder layer, so joining the decoded audio is enough.

use crate::convert::Converter;
use crate::output::{OutputFactory, OutputFormat, Shared};
use crate::player::{PlayState, PlayerEvent, Status};
use crate::upcoming::{Prepared, Progress, Upcoming};
use rtrb::{Producer, RingBuffer};
use std::any::Any;
use std::collections::VecDeque;
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
    Load {
        path: PathBuf,
        play: bool,
    },
    /// The song to go on to when the one being decoded ends, replacing any queued before; `None`
    /// clears it. Once the player has started decoding a queued song it is committed, and the
    /// next `Next` names the song after it.
    Next(Option<PathBuf>),
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
    /// Samples pushed into the ring since the played-frame count was last reset, so a song's
    /// first frame can be placed on the output's timeline.
    written_samples: u64,
    /// Keeps the stream alive; dropping it stops the output.
    _stream: Box<dyn Any>,
}

impl ActiveOutput {
    /// The output frame the next sample pushed will be played at.
    fn written_frames(&self) -> u64 {
        self.written_samples / u64::from(self.format.channels.max(1))
    }
}

/// A song in the ring: the one being heard, or one decoded into the ring behind it.
struct Song {
    path: PathBuf,
    decoder: Decoder,
    /// The file's own format.
    wanted: OutputFormat,
    converter: Converter,
    /// Converted samples waiting for room in the ring.
    pending: Vec<f32>,
    pending_from: usize,
    decoder_done: bool,
    /// The output frame (as `Shared::played_frames` counts) at which this song is heard.
    start_frame: u64,
    /// The song's position at `start_frame` (changes on seek).
    base: Duration,
    duration: Option<Duration>,
}

impl Song {
    fn new(prepared: Prepared, start_frame: u64) -> Self {
        let Prepared { path, decoder, wanted, format } = prepared;
        Song {
            path,
            duration: decoder.duration(),
            decoder,
            wanted,
            converter: Converter::new(wanted.sample_rate, wanted.channels, format.sample_rate, format.channels),
            pending: Vec::new(),
            pending_from: 0,
            decoder_done: false,
            start_frame,
            base: Duration::ZERO,
        }
    }

    fn has_pending(&self) -> bool {
        self.pending_from < self.pending.len()
    }
}

pub(crate) struct Engine {
    factory: Box<dyn OutputFactory>,
    shared: Arc<Shared>,
    status: Arc<Mutex<Status>>,
    events: Sender<PlayerEvent>,
    output: Option<ActiveOutput>,
    /// The songs in the ring, the one being heard first, the one being decoded last. Usually
    /// one; two while the next song's start waits behind the end of the current one.
    songs: VecDeque<Song>,
    /// The song to go on to after the last in `songs`.
    next: Option<Upcoming>,
}

/// Why filling the ring stopped.
#[derive(PartialEq, Eq)]
enum Fill {
    /// The ring is full, or the next song is still opening.
    Later,
    /// The last song is decoded to its end and nothing can follow it in this output.
    Ended,
}

impl Engine {
    pub fn new(
        factory: Box<dyn OutputFactory>,
        shared: Arc<Shared>,
        status: Arc<Mutex<Status>>,
        events: Sender<PlayerEvent>,
    ) -> Self {
        Self { factory, shared, status, events, output: None, songs: VecDeque::new(), next: None }
    }

    pub fn run(mut self, commands: Receiver<Command>) {
        loop {
            let wait = if self.songs.is_empty() { IDLE_WAIT } else { BUSY_WAIT };
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
            Command::Next(path) => self.next = path.map(Upcoming::open),
            Command::Play => self.shared.playing.store(!self.songs.is_empty(), Ordering::Relaxed),
            Command::Pause => self.shared.playing.store(false, Ordering::Relaxed),
            Command::Stop => self.stop(),
            Command::Seek(position) => self.seek(position),
            Command::Shutdown => {}
        }
    }

    fn load(&mut self, path: PathBuf, play: bool) {
        self.stop();
        match self.open(path.clone()) {
            Ok(song) => {
                self.songs.push_back(song);
                self.shared.playing.store(play, Ordering::Relaxed);
            }
            Err(message) => self.fail(Some(path), message, false),
        }
    }

    /// Opens `path` and makes sure an output runs in a format it can be converted to.
    fn open(&mut self, path: PathBuf) -> Result<Song, String> {
        let decoder = Decoder::open(&path).map_err(|e| e.to_string())?;
        let wanted = OutputFormat { sample_rate: decoder.sample_rate(), channels: decoder.channels() };
        let format = self.ensure_output(wanted)?;
        Ok(Song::new(Prepared { path, decoder, wanted, format }, 0))
    }

    /// Makes sure an output runs in the format negotiated for `wanted`, (re)starting it when it
    /// runs in another, and returns that format.
    fn ensure_output(&mut self, wanted: OutputFormat) -> Result<OutputFormat, String> {
        let format = self.factory.negotiate(wanted)?;
        if self.output.as_ref().is_none_or(|o| o.format != format) {
            self.output = None;
            self.output = Some(self.start_output(format)?);
            self.shared.played_frames.store(0, Ordering::Relaxed);
        }
        Ok(format)
    }

    fn start_output(&mut self, format: OutputFormat) -> Result<ActiveOutput, String> {
        let capacity =
            ((f64::from(format.sample_rate) * BUFFER_SECONDS) as usize).max(1024) * usize::from(format.channels.max(1));
        let (producer, consumer) = RingBuffer::new(capacity);
        let stream = self.factory.start(format, consumer, Arc::clone(&self.shared))?;
        self.shared.output_rate.store(format.sample_rate, Ordering::Relaxed);
        Ok(ActiveOutput { format, producer, capacity, written_samples: 0, _stream: stream })
    }

    /// Stops playing and forgets the songs, including the one queued to follow.
    fn stop(&mut self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        self.songs.clear();
        self.next = None;
        self.flush_output();
    }

    fn seek(&mut self, position: Duration) {
        let Some(song) = self.songs.front_mut() else { return };
        match song.decoder.seek(position) {
            Ok(landed) => {
                song.base = landed;
                song.start_frame = 0;
                song.pending.clear();
                song.pending_from = 0;
                song.decoder_done = false;
                // The flush below also throws away the start of a song already decoded behind
                // this one, so that song goes back to being next, rewound. (Songs shorter than
                // the buffer could have queued more than one; those after it are dropped, and
                // the owner's next `Next` names what follows.)
                let output_format = self.output.as_ref().map(|o| o.format);
                if let (Some(follower), Some(format)) = (self.songs.drain(1..).next(), output_format) {
                    let Song { path, decoder, wanted, .. } = follower;
                    self.next = Some(Prepared { path, decoder, wanted, format }.rewound());
                }
                self.flush_output();
            }
            Err(e) => log::warn!("seek failed: {e}"),
        }
    }

    /// Discards everything queued for the output and restarts the played-frame count. Waits
    /// briefly for the callback to do the discarding, so no stale audio follows.
    fn flush_output(&mut self) {
        if let Some(output) = self.output.as_mut() {
            self.shared.flush.store(true, Ordering::Release);
            let deadline = Instant::now() + Duration::from_millis(250);
            while self.shared.flush.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            output.written_samples = 0;
        }
        self.shared.played_frames.store(0, Ordering::Relaxed);
    }

    /// Keeps the ring full, follows the output from song to song, and notices the end.
    fn pump(&mut self) {
        self.progress_next();
        self.follow_output();
        if !self.shared.playing.load(Ordering::Relaxed) {
            return;
        }
        if self.fill_ring() == Fill::Ended {
            self.finish_when_drained();
        }
    }

    /// Moves the song queued to follow on once its file is open; reports it once if it cannot
    /// be, and drops it, while what is playing carries on.
    fn progress_next(&mut self) {
        let Some(next) = self.next.take() else { return };
        match next.progress(self.factory.as_mut()) {
            Progress::Waiting(next) => self.next = Some(next),
            Progress::Failed { path, message } => self.fail(Some(path), message, true),
        }
    }

    /// Hands over to each song whose first frame the output has reached.
    fn follow_output(&mut self) {
        let played = self.shared.played_frames.load(Ordering::Relaxed);
        while self.songs.get(1).is_some_and(|song| played >= song.start_frame) {
            if let (Some(finished), Some(started)) = (self.songs.pop_front(), self.songs.front()) {
                let started = Some(started.path.clone());
                let _ = self.events.send(PlayerEvent::Finished { path: finished.path, started });
            }
        }
    }

    /// Decodes until the ring is full, going on into the next song when one is ready.
    fn fill_ring(&mut self) -> Fill {
        loop {
            let behind_another = self.songs.len() > 1;
            let (Some(song), Some(output)) = (self.songs.back_mut(), self.output.as_mut()) else { return Fill::Later };
            if song.has_pending() {
                let (written, _) = output.producer.push_partial_slice(&song.pending[song.pending_from..]);
                song.pending_from += written.len();
                output.written_samples = output.written_samples.saturating_add(written.len() as u64);
                if song.has_pending() {
                    return Fill::Later; // The ring is full.
                }
            } else if !song.decoder_done {
                match song.decoder.next_block() {
                    Ok(Some(block)) => {
                        song.pending.clear();
                        song.pending_from = 0;
                        song.converter.process(&block.samples, &mut song.pending);
                    }
                    Ok(None) => song.decoder_done = true,
                    Err(e) if behind_another => {
                        // A damaged song already queued behind the one being heard: it ends
                        // where the damage starts, rather than cutting the other short.
                        log::warn!("decoding {} stopped early: {e}", song.path.display());
                        song.decoder_done = true;
                    }
                    Err(e) => {
                        let (path, message) = (song.path.clone(), e.to_string());
                        self.stop();
                        self.fail(Some(path), message, false);
                        return Fill::Later;
                    }
                }
            } else {
                match self.next.take() {
                    // Same format as the output: straight on, into the same ring.
                    Some(Upcoming::Ready(next)) if next.format == output.format => {
                        let start_frame = output.written_frames();
                        self.songs.push_back(Song::new(next, start_frame));
                    }
                    // Another format: the output has to restart, once this song has played.
                    Some(next @ Upcoming::Ready(_)) => {
                        self.next = Some(next);
                        return Fill::Ended;
                    }
                    // Still opening (a slow disk): keep the output fed meanwhile and wait.
                    Some(next @ Upcoming::Opening { .. }) => {
                        self.next = Some(next);
                        return Fill::Later;
                    }
                    None => return Fill::Ended,
                }
            }
        }
    }

    /// Once the output has played everything queued: goes on to a next song in another format,
    /// or stops.
    fn finish_when_drained(&mut self) {
        let drained = self.output.as_ref().is_some_and(|o| o.producer.slots() == o.capacity);
        if !drained || self.songs.len() > 1 {
            return;
        }
        let Some(finished) = self.songs.pop_front() else { return };
        let started = match self.next.take() {
            // Another sample rate or channel count than the running output: restart the output
            // in the new format, which leaves a short gap. Converting it to the running format
            // instead would need a better resampler than the converter's linear one.
            Some(Upcoming::Ready(next)) => match self.restart_for(next) {
                Ok(song) => {
                    let started = song.path.clone();
                    self.songs.push_back(song);
                    Some(started)
                }
                Err((path, message)) => {
                    self.stop();
                    self.fail(Some(path), message, true);
                    None
                }
            },
            _ => {
                self.stop();
                None
            }
        };
        let _ = self.events.send(PlayerEvent::Finished { path: finished.path, started });
    }

    /// Restarts the output in the format `next` needs and makes it the song playing.
    fn restart_for(&mut self, next: Prepared) -> Result<Song, (PathBuf, String)> {
        // Negotiated again: an output factory may remember details of the last negotiation.
        match self.ensure_output(next.wanted) {
            Ok(format) => {
                // Normally a fresh output (frame 0); the drained old one if it was kept after all.
                let start_frame = self.output.as_ref().map_or(0, ActiveOutput::written_frames);
                Ok(Song::new(Prepared { format, ..next }, start_frame))
            }
            Err(message) => Err((next.path, message)),
        }
    }

    fn fail(&mut self, path: Option<PathBuf>, message: String, upcoming: bool) {
        log::warn!("playback failed: {message}");
        let _ = self.events.send(PlayerEvent::Failed { path, message, upcoming });
    }

    fn publish(&self) {
        let status = match (self.songs.front(), &self.output) {
            (Some(song), Some(output)) => {
                let played = self.shared.played_frames.load(Ordering::Relaxed).saturating_sub(song.start_frame);
                let elapsed = Duration::from_secs_f64(played as f64 / f64::from(output.format.sample_rate.max(1)));
                let position = song.base.saturating_add(elapsed);
                Status {
                    state: if self.shared.playing.load(Ordering::Relaxed) {
                        PlayState::Playing
                    } else {
                        PlayState::Paused
                    },
                    path: Some(song.path.clone()),
                    position: song.duration.map_or(position, |d| position.min(d)),
                    duration: song.duration,
                }
            }
            _ => Status::default(),
        };
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }
}

#[cfg(test)]
mod tests {
    use crate::{NullOutput, OutputFactory, OutputFormat, PlayState, Player, PlayerEvent, Shared};
    use rtrb::Consumer;
    use std::any::Any;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tunebox_audio::{Decoder, LosslessFormat, PcmWriter};

    /// Like `NullOutput`, but keeps everything it outputs, silence included.
    #[derive(Clone)]
    struct RecordingOutput {
        speed: f64,
        heard: Arc<Mutex<Vec<f32>>>,
    }

    impl RecordingOutput {
        fn new(speed: f64) -> Self {
            Self { speed, heard: Arc::default() }
        }

        /// What was output, without the silence before the first sound and after the last (the
        /// output runs before the first song is decoded and after the last has ended).
        fn heard(&self) -> Vec<f32> {
            let heard = self.heard.lock().unwrap();
            let first = heard.iter().position(|s| *s != 0.0).unwrap_or(heard.len());
            let last = heard.iter().rposition(|s| *s != 0.0).map_or(first, |i| i + 1);
            heard[first..last].to_vec()
        }
    }

    struct Stopper(Arc<AtomicBool>);

    impl Drop for Stopper {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    impl OutputFactory for RecordingOutput {
        fn negotiate(&mut self, wanted: OutputFormat) -> Result<OutputFormat, String> {
            Ok(wanted)
        }

        fn start(
            &mut self,
            format: OutputFormat,
            mut queue: Consumer<f32>,
            shared: Arc<Shared>,
        ) -> Result<Box<dyn Any>, String> {
            const FRAMES: usize = 256;
            let stop = Arc::new(AtomicBool::new(false));
            let (stopped, heard) = (Arc::clone(&stop), Arc::clone(&self.heard));
            let period = Duration::from_secs_f64(FRAMES as f64 / f64::from(format.sample_rate) / self.speed);
            std::thread::spawn(move || {
                let mut buffer = vec![0.0; FRAMES * usize::from(format.channels)];
                while !stopped.load(Ordering::Relaxed) {
                    shared.fill(&mut buffer, &mut queue, format.channels);
                    heard.lock().unwrap().extend_from_slice(&buffer);
                    std::thread::sleep(period);
                }
            });
            Ok(Box::new(Stopper(stop)))
        }
    }

    /// A continuous tone that never touches zero, so any silence in what is heard is a gap.
    /// `phase` continues it from where an earlier file left off.
    fn continuous_tone(path: &Path, frames: usize, phase: usize, rate: u32) {
        let wave: Vec<f32> = (phase..phase + frames).map(|i| 0.4 + (i as f32 * 0.03).sin() * 0.3).collect();
        let mut writer = PcmWriter::create(path, LosslessFormat::Wav, rate, 1).unwrap();
        writer.write(&[wave]).unwrap();
        writer.finish().unwrap();
    }

    /// Every sample of a file, as the decoder gives them.
    fn decoded(path: &Path) -> Vec<f32> {
        let mut decoder = Decoder::open(path).unwrap();
        let mut samples = Vec::new();
        while let Some(block) = decoder.next_block().unwrap() {
            samples.extend(block.samples);
        }
        samples
    }

    fn finished(path: &Path, started: Option<&Path>) -> PlayerEvent {
        PlayerEvent::Finished { path: path.to_path_buf(), started: started.map(Path::to_path_buf) }
    }

    /// Two files of one continuous tone, `seconds` long each at 8 kHz mono.
    fn two_halves(dir: &Path, seconds: f64) -> (PathBuf, PathBuf) {
        let frames = (seconds * 8_000.0) as usize;
        let (first, second) = (dir.join("first.wav"), dir.join("second.wav"));
        continuous_tone(&first, frames, 0, 8_000);
        continuous_tone(&second, frames, frames, 8_000);
        (first, second)
    }

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
        assert_eq!(events, [finished(&path, None)]);
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
        assert!(matches!(&events[..], [PlayerEvent::Failed { path: Some(p), upcoming: false, .. }] if *p == path));
    }

    #[test]
    fn the_next_song_follows_without_a_gap() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = two_halves(dir.path(), 1.5);
        let output = RecordingOutput::new(10.0);
        let player = Player::with_output(output.clone());
        player.load(first.clone(), true);
        player.set_next(Some(second.clone()));
        let events = wait_for(&player, Duration::from_secs(10), |e| e.len() >= 2);
        assert_eq!(events, [finished(&first, Some(&second)), finished(&second, None)]);
        assert_eq!(player.status().state, PlayState::Stopped);

        let mut expected = decoded(&first);
        expected.extend(decoded(&second));
        let heard = output.heard();
        assert_eq!(heard.len(), expected.len(), "every frame of both, and nothing between them");
        assert!(heard == expected, "the samples come out exactly as decoded");
    }

    #[test]
    fn the_hand_over_comes_when_the_output_reaches_the_next_song() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = two_halves(dir.path(), 2.0);
        let first_frames = decoded(&first).len();
        let output = RecordingOutput::new(4.0);
        let player = Player::with_output(output.clone());
        player.load(first.clone(), true);
        player.set_next(Some(second.clone()));
        let start = Instant::now();
        let mut heard_at_hand_over = None;
        while start.elapsed() < Duration::from_secs(10) && heard_at_hand_over.is_none() {
            let status = player.status();
            if player.poll_events().contains(&finished(&first, Some(&second))) {
                heard_at_hand_over = Some(output.heard().len());
            } else if status.path.as_deref() == Some(second.as_path()) {
                panic!("the status moved on before the hand-over");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let heard = heard_at_hand_over.expect("handed over");
        // Decoding reaches the boundary half a second (4 000 frames) earlier; the hand-over
        // waits for the output, and comes within a few output periods of it.
        assert!(heard >= first_frames, "{heard} heard of {first_frames}");
        assert!(heard < first_frames + 1_500, "{heard} heard of {first_frames}");
        let status = player.status();
        assert_eq!(status.path.as_deref(), Some(second.as_path()));
        assert!(status.position < Duration::from_millis(500), "{:?}", status.position);
    }

    #[test]
    fn a_next_song_that_cannot_be_opened_is_reported_once_and_the_current_one_plays_on() {
        let dir = tempfile::tempdir().unwrap();
        let (first, _) = two_halves(dir.path(), 1.0);
        let bad = dir.path().join("bad.mp3");
        std::fs::write(&bad, b"nope").unwrap();
        let output = RecordingOutput::new(10.0);
        let player = Player::with_output(output.clone());
        player.load(first.clone(), true);
        player.set_next(Some(bad.clone()));
        let events =
            wait_for(&player, Duration::from_secs(10), |e| e.iter().any(|e| matches!(e, PlayerEvent::Finished { .. })));
        assert!(
            matches!(&events[..], [PlayerEvent::Failed { path: Some(p), upcoming: true, .. }, PlayerEvent::Finished { started: None, .. }] if *p == bad),
            "{events:?}"
        );
        assert!(output.heard() == decoded(&first), "the current song played to its end");
    }

    #[test]
    fn replacing_the_next_song_before_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = two_halves(dir.path(), 1.5);
        let other = dir.path().join("other.wav");
        continuous_tone(&other, 8_000, 12_000, 8_000);
        let output = RecordingOutput::new(10.0);
        let player = Player::with_output(output.clone());
        player.load(first.clone(), true);
        player.set_next(Some(other.clone()));
        player.set_next(Some(second.clone()));
        let events = wait_for(&player, Duration::from_secs(10), |e| e.len() >= 2);
        assert_eq!(events, [finished(&first, Some(&second)), finished(&second, None)]);
        let mut expected = decoded(&first);
        expected.extend(decoded(&second));
        assert!(output.heard() == expected);
    }

    #[test]
    fn seeking_near_the_end_rewinds_the_next_song_already_decoded() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = two_halves(dir.path(), 3.0);
        let output = RecordingOutput::new(2.0);
        let player = Player::with_output(output.clone());
        player.load(first.clone(), true);
        player.set_next(Some(second.clone()));
        // With under half a second left, the start of the next song is already in the ring.
        let start = Instant::now();
        while player.status().position < Duration::from_millis(2_600) {
            assert!(start.elapsed() < Duration::from_secs(10), "timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
        player.seek(Duration::from_secs(2));
        let events = wait_for(&player, Duration::from_secs(10), |e| e.len() >= 2);
        assert_eq!(events, [finished(&first, Some(&second)), finished(&second, None)]);
        // The end of what was heard is the first song from 2 s on, then all of the second.
        let first_tail = decoded(&first).split_off(16_000);
        let mut expected = first_tail;
        expected.extend(decoded(&second));
        let heard = output.heard();
        assert!(heard.len() > expected.len());
        assert!(heard[heard.len() - expected.len()..] == expected[..], "the next song is heard whole, right after");
    }

    #[test]
    fn a_next_song_in_another_format_still_follows() {
        let dir = tempfile::tempdir().unwrap();
        let (first, _) = two_halves(dir.path(), 0.5);
        let other = dir.path().join("other.wav");
        continuous_tone(&other, 11_025, 0, 22_050);
        let player = Player::with_output(RecordingOutput::new(10.0));
        player.load(first.clone(), true);
        player.set_next(Some(other.clone()));
        let events = wait_for(&player, Duration::from_secs(10), |e| e.len() >= 2);
        assert_eq!(events, [finished(&first, Some(&other)), finished(&other, None)]);
    }

    #[test]
    fn stopping_drops_the_next_song() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = two_halves(dir.path(), 0.5);
        let player = Player::with_output(RecordingOutput::new(10.0));
        player.set_next(Some(second.clone()));
        player.stop();
        player.load(first.clone(), true);
        let events = wait_for(&player, Duration::from_secs(10), |e| !e.is_empty());
        assert_eq!(events, [finished(&first, None)]);
        // And a load drops it too.
        player.set_next(Some(second));
        player.load(first.clone(), true);
        let events = wait_for(&player, Duration::from_secs(10), |e| !e.is_empty());
        assert_eq!(events, [finished(&first, None)]);
    }
}
