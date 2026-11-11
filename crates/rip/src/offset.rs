//! Drive read offsets: which sectors to read so a track comes out sample-exact.
//!
//! Every CD drive delivers audio a constant number of samples early or late: its *read offset*,
//! which belongs to the drive's make and model. A drive with an offset of +667 hands back, when
//! asked for sample *n*, the sample that really sits at *n* − 667, so a rip reads 667 samples
//! later than the table of contents says, taking that much of the next track and leaving the
//! same amount of its own start to the previous one. A negative offset shifts the other way.
//! (One sample here is one stereo frame, 4 bytes; a sector holds 588.)
//!
//! Drives read whole sectors, so the shifted range is widened to sector boundaries and the
//! surplus is dropped at both ends. Where the shift reaches before the start of the disc (into
//! the lead-in) or past the end of its audio (into the lead-out), most drives cannot read at
//! all; those samples are written as silence, as is usual.
//!
//! [`shifted_read`] is the whole calculation, pure, so every edge case is tested here. Reading
//! any sector range through `CdDrive::read_sectors` (rather than carrying the tail of one track
//! into the next) is what lets the listener rip any selection of tracks in any order.

use std::ops::Range;

/// Stereo samples (frames) in one audio sector.
pub const SAMPLES_PER_SECTOR: u32 = 588;

/// The largest read offset accepted, either way, in samples. Published drive offsets fall
/// within a few thousand samples; anything far beyond is a typing mistake.
pub const MAX_READ_OFFSET: i32 = 10_000;

/// Checks a read offset typed by the listener.
pub fn check_read_offset(samples: i64) -> Result<i32, String> {
    i32::try_from(samples).ok().filter(|s| s.abs() <= MAX_READ_OFFSET).ok_or_else(|| {
        format!(
            "{samples} samples is not a drive read offset. Read offsets are between \
             -{MAX_READ_OFFSET} and +{MAX_READ_OFFSET} samples; look your drive up in the \
             AccurateRip drive list, or use 0 if it is not listed."
        )
    })
}

/// How to read one track with a read offset applied. Counts are in samples (stereo frames).
///
/// The track comes out as `silence_before` samples of silence, then the read of `sectors`
/// sectors from `first_sector` with `skip` samples dropped at its start and only `keep`
/// samples kept, then `silence_after` samples of silence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShiftedRead {
    pub silence_before: u64,
    pub first_sector: u32,
    pub sectors: u32,
    pub skip: u64,
    pub keep: u64,
    pub silence_after: u64,
}

impl ShiftedRead {
    /// Samples read but not used at the end of the read.
    pub fn drop_end(&self) -> u64 {
        (u64::from(self.sectors) * u64::from(SAMPLES_PER_SECTOR)).saturating_sub(self.skip + self.keep)
    }

    /// The length of the track that comes out: always the track's own length.
    pub fn total(&self) -> u64 {
        self.silence_before + self.keep + self.silence_after
    }
}

/// Plans the read of the track in sectors `track` of a disc whose audio occupies sectors
/// `disc`, for a drive with a read offset of `offset` samples.
pub fn shifted_read(track: Range<u32>, disc: Range<u32>, offset: i32) -> ShiftedRead {
    let per_sector = i64::from(SAMPLES_PER_SECTOR);
    let track_start = i64::from(track.start) * per_sector;
    let track_end = i64::from(track.end.max(track.start)) * per_sector;
    let readable = i64::from(disc.start) * per_sector..i64::from(disc.end.max(disc.start)) * per_sector;
    // The samples the drive must be asked for.
    let wanted = track_start + i64::from(offset)..track_end + i64::from(offset);
    let length = wanted.end - wanted.start;
    // The part of that the drive can read; the rest is silence.
    let read_start = wanted.start.clamp(readable.start, readable.end);
    let read_end = wanted.end.clamp(read_start, readable.end);
    let keep = read_end - read_start;
    let silence_before = if keep == 0 {
        // Nothing readable: all silence, before or after depending on the direction.
        if wanted.end <= readable.start { length } else { 0 }
    } else {
        read_start - wanted.start
    };
    let silence_after = length - keep - silence_before;
    let (first_sector, sectors, skip) = if keep == 0 {
        (0, 0, 0)
    } else {
        let first = read_start.div_euclid(per_sector);
        let last = (read_end + per_sector - 1).div_euclid(per_sector);
        (first, last - first, read_start - first * per_sector)
    };
    // Every value is a non-negative count within the disc, so these conversions cannot fail;
    // `unwrap_or_default` keeps that from ever becoming a crash.
    ShiftedRead {
        silence_before: u64::try_from(silence_before).unwrap_or_default(),
        first_sector: u32::try_from(first_sector).unwrap_or_default(),
        sectors: u32::try_from(sectors).unwrap_or_default(),
        skip: u64::try_from(skip).unwrap_or_default(),
        keep: u64::try_from(keep).unwrap_or_default(),
        silence_after: u64::try_from(silence_after).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = SAMPLES_PER_SECTOR as u64;

    /// A three-track disc: sectors 0–100, 100–300, 300–310 (a 10-sector track at the end).
    const DISC: Range<u32> = 0..310;

    fn check(read: ShiftedRead, track: Range<u32>) {
        assert_eq!(read.total(), u64::from(track.end - track.start) * S, "{read:?}");
        assert!(read.skip < S || read.keep == 0, "{read:?}");
        assert!(read.drop_end() < S, "{read:?}");
        assert!(read.first_sector + read.sectors <= DISC.end, "{read:?}");
    }

    #[test]
    fn zero_offset_reads_the_track_exactly() {
        let read = shifted_read(100..300, DISC, 0);
        assert_eq!(
            read,
            ShiftedRead {
                silence_before: 0,
                first_sector: 100,
                sectors: 200,
                skip: 0,
                keep: 200 * S,
                silence_after: 0
            }
        );
        assert_eq!(read.drop_end(), 0);
    }

    #[test]
    fn positive_offset_reads_into_the_next_track() {
        let read = shifted_read(100..300, DISC, 6);
        assert_eq!(
            read,
            ShiftedRead {
                silence_before: 0,
                first_sector: 100,
                sectors: 201,
                skip: 6,
                keep: 200 * S,
                silence_after: 0
            }
        );
        assert_eq!(read.drop_end(), S - 6);
        check(read, 100..300);
    }

    #[test]
    fn negative_offset_reads_from_the_previous_track() {
        let read = shifted_read(100..300, DISC, -1164);
        // 1164 samples = 1 sector and 576 samples back.
        assert_eq!(
            read,
            ShiftedRead {
                silence_before: 0,
                first_sector: 98,
                sectors: 201,
                skip: S - 576,
                keep: 200 * S,
                silence_after: 0
            }
        );
        check(read, 100..300);
    }

    #[test]
    fn offsets_of_whole_sectors_need_no_trimming() {
        let read = shifted_read(100..300, DISC, 588);
        assert_eq!((read.first_sector, read.sectors, read.skip, read.drop_end()), (101, 200, 0, 0));
        let read = shifted_read(100..300, DISC, -1176);
        assert_eq!((read.first_sector, read.sectors, read.skip, read.drop_end()), (98, 200, 0, 0));
    }

    #[test]
    fn first_track_pads_the_lead_in_with_silence() {
        let read = shifted_read(0..100, DISC, -667);
        assert_eq!(read.silence_before, 667);
        assert_eq!((read.first_sector, read.skip, read.keep), (0, 0, 100 * S - 667));
        assert_eq!(read.silence_after, 0);
        check(read, 0..100);
        // A positive offset on the first track needs no padding.
        let read = shifted_read(0..100, DISC, 667);
        assert_eq!((read.silence_before, read.silence_after, read.first_sector, read.skip), (0, 0, 1, 667 - S));
        check(read, 0..100);
    }

    #[test]
    fn last_track_pads_the_lead_out_with_silence() {
        let read = shifted_read(300..310, DISC, 667);
        assert_eq!(read.silence_after, 667);
        assert_eq!(read.silence_before, 0);
        assert_eq!((read.first_sector, read.skip, read.keep), (301, 667 - S, 10 * S - 667));
        check(read, 300..310);
        let read = shifted_read(300..310, DISC, -667);
        assert_eq!((read.silence_before, read.silence_after), (0, 0));
        check(read, 300..310);
    }

    #[test]
    fn offset_longer_than_a_short_last_track_is_all_silence() {
        // 10 sectors = 5880 samples; an offset of 6000 shifts the whole track past the lead-out.
        let read = shifted_read(300..310, DISC, 6000);
        assert_eq!(
            read,
            ShiftedRead { silence_before: 0, first_sector: 0, sectors: 0, skip: 0, keep: 0, silence_after: 10 * S }
        );
        // Shifting a short first track wholly into the lead-in.
        let read = shifted_read(0..5, DISC, -6000);
        assert_eq!((read.silence_before, read.sectors, read.silence_after), (5 * S, 0, 0));
    }

    #[test]
    fn offset_longer_than_a_short_middle_track_reads_the_neighbour() {
        // A 2-sector track (1176 samples) shifted by 6000 is read wholly from the next track.
        let read = shifted_read(150..152, 0..400, 6000);
        assert_eq!((read.silence_before, read.silence_after, read.keep), (0, 0, 2 * S));
        assert_eq!(read.first_sector, 150 + 6000 / 588);
        assert_eq!(read.skip, 6000 % 588);
    }

    #[test]
    fn a_single_track_disc_pads_one_end() {
        for offset in [-6000, -1164, -1, 0, 1, 6, 667, 6000] {
            let read = shifted_read(0..310, DISC, offset);
            check(read, 0..310);
            let padding = u64::from(offset.unsigned_abs());
            if offset < 0 {
                assert_eq!((read.silence_before, read.silence_after), (padding, 0));
            } else {
                assert_eq!((read.silence_before, read.silence_after), (0, padding));
            }
        }
    }

    #[test]
    fn every_offset_keeps_the_track_length() {
        for track in [0..100, 100..300, 300..310] {
            for offset in (-6000..=6000).step_by(37) {
                check(shifted_read(track.clone(), DISC, offset), track.clone());
            }
        }
    }

    #[test]
    fn checks_offsets() {
        assert_eq!(check_read_offset(667), Ok(667));
        assert_eq!(check_read_offset(-1164), Ok(-1164));
        assert_eq!(check_read_offset(0), Ok(0));
        assert_eq!(check_read_offset(10_000), Ok(10_000));
        assert_eq!(check_read_offset(-10_000), Ok(-10_000));
        assert!(check_read_offset(10_001).is_err());
        assert!(check_read_offset(-60_000).is_err());
        assert!(check_read_offset(i64::MAX).is_err());
    }
}
