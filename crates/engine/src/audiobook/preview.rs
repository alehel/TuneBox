//! Hearing a stretch of a staged track while editing (Play Across Start, Play End, the trim
//! editor), and its waveform for the trim editor.
//!
//! Play Across Start plays the end of the track before and the start of this one, so the
//! listener hears the join where a chapter would start: the player goes from the one file
//! straight into the next, as gapless playback does.
//!
//! A preview plays outside the play queue and stops by itself at its end; anything else that
//! plays stops it.

use super::project;
use crate::EngineError;
use crate::session::Session;
use std::path::PathBuf;
use std::time::Duration;
use tunebox_audio::Decoder;
use tunebox_model::audiobook::{SAMPLE_RATE, TrackRef, samples, seconds};
use tunebox_playback::PlayState;

/// A stretch of a staged track being played.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub track: TrackRef,
    pub from: Duration,
    pub to: Duration,
    /// Where it has got to.
    pub position: Duration,
}

/// The preview in progress, as the session keeps it.
#[derive(Clone, Debug)]
pub(crate) struct PreviewRun {
    track: TrackRef,
    path: PathBuf,
    from: Duration,
    to: Duration,
    /// A second file played on from its start, to this time: the track after the join.
    then: Option<(PathBuf, Duration)>,
}

/// How much of each side of a join Play Across Start plays, and how much of a track's end
/// Play End plays.
const ACROSS_SECONDS: f64 = 4.0;
const END_SECONDS: f64 = 8.0;

impl Session {
    /// Plays samples `from..to` of the staged `track` (clamped to the track).
    pub fn preview_audiobook(&mut self, track: TrackRef, from: u64, to: u64) -> Result<(), EngineError> {
        let state = self.audiobook.as_ref().ok_or(EngineError::NoAudiobook)?;
        let data = state.book.track(track).ok_or(EngineError::Audiobook(format!("The book has no track {track}.")))?;
        if !data.staged {
            return Err(EngineError::Audiobook(format!("Track {track} is not ripped yet, so it cannot be played.")));
        }
        let to = to.min(data.samples);
        let from = from.min(to);
        let path = project::staged_path(&state.folder, track);
        let (from, to) = (duration(from), duration(to));
        self.playback.play_file_from(path.clone(), from, None);
        self.preview = Some(PreviewRun { track, path, from, to, then: None });
        self.content_changed();
        Ok(())
    }

    /// Plays the last seconds of the ripped track before `track` and the first of `track`: the
    /// join a chapter starting at `track` would have. The book's first track plays from its start.
    pub fn preview_audiobook_across(&mut self, track: TrackRef) -> Result<(), EngineError> {
        let state = self.audiobook.as_ref().ok_or(EngineError::NoAudiobook)?;
        let tracks: Vec<TrackRef> = state.book.track_refs();
        let before = tracks.iter().position(|t| *t == track).and_then(|i| i.checked_sub(1)).and_then(|i| tracks.get(i));
        let staged = |t: TrackRef| state.book.track(t).filter(|d| d.staged).map(|d| d.samples);
        let span = samples(ACROSS_SECONDS);
        let Some(length) = staged(track) else {
            return Err(EngineError::Audiobook(format!("Track {track} is not ripped yet, so it cannot be played.")));
        };
        match before.and_then(|b| staged(*b).map(|len| (*b, len))) {
            Some((before, before_length)) => {
                let first = project::staged_path(&state.folder, before);
                let second = project::staged_path(&state.folder, track);
                let from = duration(before_length.saturating_sub(span));
                let then_to = duration(span.min(length));
                self.playback.play_file_from(first.clone(), from, Some(second.clone()));
                self.preview = Some(PreviewRun {
                    track,
                    path: first,
                    from,
                    to: duration(before_length),
                    then: Some((second, then_to)),
                });
                self.content_changed();
                Ok(())
            }
            None => self.preview_audiobook(track, 0, span),
        }
    }

    /// Plays the last seconds of `track`, where announcements are.
    pub fn preview_audiobook_end(&mut self, track: TrackRef) -> Result<(), EngineError> {
        let length = self.audiobook().and_then(|b| b.track(track)).map_or(0, |t| t.samples);
        let span = samples(END_SECONDS);
        self.preview_audiobook(track, length.saturating_sub(span), length)
    }

    /// Stops the preview, if one plays.
    pub fn stop_audiobook_preview(&mut self) {
        if self.preview.take().is_some() {
            self.playback.stop();
            self.content_changed();
        }
    }

    /// The preview playing, if any.
    pub fn audiobook_preview(&self) -> Option<Preview> {
        let run = self.preview.as_ref()?;
        let (_, position, _) = self.playback.position();
        Some(Preview { track: run.track, from: run.from, to: run.to, position })
    }

    /// Stops the preview at its end, and forgets it once the player plays something else.
    pub(crate) fn follow_audiobook_preview(&mut self) {
        let Some(run) = &self.preview else { return };
        let (path, position, state) = self.playback.position();
        let playing = |end: Duration| state == PlayState::Stopped || (state == PlayState::Playing && position >= end);
        let ended = match (&run.then, path.as_ref()) {
            // Still in the first file: it plays to its end and on into the second.
            (Some(_), Some(p)) if *p == run.path => state == PlayState::Stopped,
            (Some((second, end)), Some(p)) if p == second => playing(*end),
            (None, Some(p)) if *p == run.path => playing(run.to),
            _ => {
                // Something else plays now: the preview is over without stopping it.
                self.preview = None;
                return;
            }
        };
        if ended {
            self.stop_audiobook_preview();
        }
    }

    /// The loudness of samples `from..to` of the staged `track`, in `columns` columns: each the
    /// peak of its stretch, 0–1. For the trim editor's waveform.
    pub fn audiobook_waveform(
        &self,
        track: TrackRef,
        from: u64,
        to: u64,
        columns: usize,
    ) -> Result<Vec<f32>, EngineError> {
        let state = self.audiobook.as_ref().ok_or(EngineError::NoAudiobook)?;
        let staged = state.book.track(track).is_some_and(|t| t.staged);
        if !staged || columns == 0 || to <= from {
            return Ok(vec![0.0; columns]);
        }
        let unreadable = |e: tunebox_audio::AudioError| {
            EngineError::Audiobook(format!("The ripped audio of track {track} could not be read ({e})."))
        };
        let mut decoder = Decoder::open(&project::staged_path(&state.folder, track)).map_err(unreadable)?;
        // Seeking lands near the target; the samples are counted from where it landed.
        let mut position = samples_at(decoder.seek(duration(from)).map_err(unreadable)?);
        let per_column = ((to - from) as f64 / columns as f64).max(1.0);
        let mut peaks = vec![0.0f32; columns];
        let channels = usize::from(decoder.channels().max(1));
        'blocks: while let Some(block) = decoder.next_block().map_err(unreadable)? {
            for frame in block.samples.chunks(channels) {
                if position >= to {
                    break 'blocks;
                }
                if position >= from {
                    let column = (((position - from) as f64 / per_column) as usize).min(columns - 1);
                    let level = frame.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
                    if let Some(peak) = peaks.get_mut(column) {
                        *peak = peak.max(level);
                    }
                }
                position += 1;
            }
        }
        Ok(peaks)
    }
}

fn duration(samples: u64) -> Duration {
    Duration::from_secs_f64(seconds(samples))
}

fn samples_at(time: Duration) -> u64 {
    (time.as_secs_f64() * f64::from(SAMPLE_RATE)).round() as u64
}
