//! Reading a part of a staged track: samples `from..to`, as planar stereo blocks.
//!
//! The staged file must hold exactly the track's length as the disc's table of contents gives
//! it; anything else means the file was damaged after ripping, and the book would come out
//! with a gap or a jump. That is refused with the disc to rip again.

use std::path::Path;
use tunebox_audio::Decoder;
use tunebox_model::audiobook::TrackRef;

/// Why a staged track could not be read.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("The ripped audio of track {track} could not be read ({reason}). Rip disc {} again.", .track.disc)]
    Unreadable { track: TrackRef, reason: String },
    #[error(
        "The ripped audio of track {track} is {found} samples long, not {expected}. Rip disc {} again.",
        .track.disc
    )]
    WrongLength { track: TrackRef, expected: u64, found: u64 },
    #[error("{0}")]
    Sink(String),
}

/// Where read audio goes: planar stereo blocks; `Ok(false)` stops reading.
pub type PartSink<'a> = dyn FnMut(&[Vec<f32>]) -> Result<bool, String> + 'a;

/// Reads samples `from..to` of the staged `track` at `path`, whose full length is `length`,
/// into `sink` in planar stereo blocks. `sink` returns `false` to stop (when cancelled).
pub fn read_part(
    path: &Path,
    track: TrackRef,
    length: u64,
    from: u64,
    to: u64,
    sink: &mut PartSink<'_>,
) -> Result<(), SourceError> {
    let unreadable = |reason: String| SourceError::Unreadable { track, reason };
    let mut decoder = Decoder::open(path).map_err(|e| unreadable(e.to_string()))?;
    if decoder.channels() != 2 || decoder.sample_rate() != 44_100 {
        return Err(unreadable("it is not CD audio".into()));
    }
    let mut position: u64 = 0;
    let mut stopped = false;
    while let Some(block) = decoder.next_block().map_err(|e| unreadable(e.to_string()))? {
        let frames = block.frames() as u64;
        let (start, end) = (position, position + frames);
        position = end;
        if stopped || end <= from || start >= to {
            continue;
        }
        let skip = from.saturating_sub(start) as usize;
        let take = (to.min(end) - start.max(from)) as usize;
        let mut planar = vec![Vec::with_capacity(take), Vec::with_capacity(take)];
        for frame in block.samples.chunks_exact(2).skip(skip).take(take) {
            if let [left, right] = frame {
                planar[0].push(*left);
                planar[1].push(*right);
            }
        }
        if !sink(&planar).map_err(SourceError::Sink)? {
            stopped = true;
        }
    }
    if stopped {
        return Ok(());
    }
    if position != length {
        return Err(SourceError::WrongLength { track, expected: length, found: position });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunebox_audio::{LosslessFormat, PcmWriter};

    fn staged(path: &Path, frames: usize) {
        let ramp: Vec<f32> = (0..frames).map(|i| (i % 1000) as f32 / 2000.0).collect();
        let mut writer = PcmWriter::create(path, LosslessFormat::Flac, 44_100, 2).unwrap();
        writer.write(&[ramp.clone(), ramp]).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn reads_exactly_the_part_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.flac");
        staged(&path, 50_000);
        let mut got = Vec::new();
        read_part(&path, TrackRef::new(1, 1), 50_000, 1_234, 40_000, &mut |b| {
            got.extend_from_slice(&b[0]);
            Ok(true)
        })
        .unwrap();
        assert_eq!(got.len(), 40_000 - 1_234);
        let expected = (1_234 % 1000) as f32 / 2000.0;
        assert!((got[0] - expected).abs() < 1e-4, "{} vs {expected}", got[0]);
    }

    #[test]
    fn a_file_of_the_wrong_length_is_refused_with_the_disc_to_rip_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.flac");
        staged(&path, 1_000);
        let error = read_part(&path, TrackRef::new(3, 2), 2_000, 0, 2_000, &mut |_| Ok(true)).unwrap_err();
        assert!(error.to_string().contains("Rip disc 3 again"), "{error}");
        let missing = read_part(&dir.path().join("none.flac"), TrackRef::new(1, 1), 1, 0, 1, &mut |_| Ok(true));
        assert!(matches!(missing, Err(SourceError::Unreadable { .. })));
    }
}
