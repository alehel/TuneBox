//! A disc's table of contents: where each track starts, in sectors.

use std::time::Duration;

/// Sectors ("frames") per second of CD audio.
pub const FRAMES_PER_SECOND: u32 = 75;
/// Bytes in one audio sector: 588 stereo frames of 16-bit samples.
pub const SECTOR_BYTES: usize = 2352;
/// The gap between the audio session and a data session on an enhanced CD.
const DATA_SESSION_GAP: u32 = 11_400;

/// One entry of the table of contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TocTrack {
    pub number: u8,
    /// First sector, counted from the end of the 2-second lead-in.
    pub start_lba: u32,
    pub is_audio: bool,
}

/// The table of contents of the disc in a drive.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DiscToc {
    pub tracks: Vec<TocTrack>,
    /// First sector after the last track.
    pub leadout_lba: u32,
}

impl DiscToc {
    pub fn audio_tracks(&self) -> impl Iterator<Item = &TocTrack> {
        self.tracks.iter().filter(|t| t.is_audio)
    }

    pub fn track(&self, number: u8) -> Option<&TocTrack> {
        self.tracks.iter().find(|t| t.number == number)
    }

    /// Where a track's audio ends: the next track's start, less the session gap when a data
    /// track follows, or the lead-out.
    pub fn end_lba(&self, number: u8) -> Option<u32> {
        let index = self.tracks.iter().position(|t| t.number == number)?;
        let start = self.tracks.get(index)?.start_lba;
        let end = match self.tracks.get(index + 1) {
            Some(next) if !next.is_audio => next.start_lba.saturating_sub(DATA_SESSION_GAP),
            Some(next) => next.start_lba,
            None => self.leadout_lba,
        };
        Some(end.max(start))
    }

    /// A track's length in sectors.
    pub fn sectors(&self, number: u8) -> Option<u32> {
        let start = self.track(number)?.start_lba;
        Some(self.end_lba(number)?.saturating_sub(start))
    }

    /// A track's playing time.
    pub fn duration(&self, number: u8) -> Option<Duration> {
        self.sectors(number).map(sectors_to_duration)
    }

    /// The playing time of every audio track together.
    pub fn total_duration(&self) -> Duration {
        self.audio_tracks().filter_map(|t| self.duration(t.number)).sum()
    }

    /// Where the audio session ends: the lead-out, or the end of the last audio track on an
    /// enhanced CD. Disc ids are computed from this.
    pub fn audio_leadout(&self) -> u32 {
        self.audio_tracks().last().and_then(|t| self.end_lba(t.number)).unwrap_or(self.leadout_lba)
    }
}

pub fn sectors_to_duration(sectors: u32) -> Duration {
    Duration::from_millis(u64::from(sectors) * 1000 / u64::from(FRAMES_PER_SECOND))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The six-track disc of the MusicBrainz disc id documentation.
    pub fn sample_toc() -> DiscToc {
        let starts = [0, 15_213, 32_164, 46_442, 63_264, 80_339];
        DiscToc {
            tracks: starts
                .iter()
                .enumerate()
                .map(|(i, &start_lba)| TocTrack { number: i as u8 + 1, start_lba, is_audio: true })
                .collect(),
            leadout_lba: 95_312,
        }
    }

    #[test]
    fn durations() {
        let toc = sample_toc();
        assert_eq!(toc.sectors(1), Some(15_213));
        assert_eq!(toc.duration(6).map(|d| d.as_secs()), Some((95_312 - 80_339) / 75));
        assert_eq!(toc.total_duration().as_secs(), 95_312 / 75);
        assert_eq!(toc.duration(9), None);
    }

    #[test]
    fn enhanced_cd_excludes_the_data_session() {
        let mut toc = sample_toc();
        toc.tracks.push(TocTrack { number: 7, start_lba: 120_000, is_audio: false });
        toc.leadout_lba = 150_000;
        assert_eq!(toc.end_lba(6), Some(120_000 - DATA_SESSION_GAP));
        assert_eq!(toc.audio_leadout(), 120_000 - DATA_SESSION_GAP);
        assert_eq!(toc.audio_tracks().count(), 6);
    }
}
