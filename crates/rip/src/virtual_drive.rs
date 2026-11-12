//! A drive with a made-up disc in it: each track a quiet tone. It makes the whole CD import
//! path testable without hardware, and gives demos and screenshots a disc to show.
//!
//! A spoken disc ([`VirtualDrive::spoken`]) sounds more like an audiobook: sentences with pauses
//! between them, and on the disc's last track an announcement after a longer pause, so pause
//! finding and the audiobook's suggestions have something to find.
//!
//! The disc's *true* audio is fixed ([`VirtualDrive::true_audio`]). Like a real drive, the
//! virtual one can have a read offset: with `read_offset` +667, asking for sample *n* returns
//! true sample *n* − 667, and the lead-in and lead-out read as silence. Ripping with the
//! matching correction must give back the true audio.

use crate::drive::{CdDrive, DriveError, DriveIdentity, check_range};
use crate::offset::SAMPLES_PER_SECTOR;
#[cfg(test)]
use crate::toc::SECTOR_BYTES;
use crate::toc::{DiscToc, FRAMES_PER_SECOND, TocTrack};

/// Sectors returned per chunk, as a real drive reads in bursts.
const SECTORS_PER_CHUNK: u32 = 75;
/// The usual pregap before track 1: 2 seconds, silent.
const PREGAP: u32 = 150;

/// A synthetic disc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualDrive {
    pub name: String,
    /// Each track's length in whole seconds.
    pub track_seconds: Vec<u32>,
    /// The simulated drive's read offset, in samples (0 reads the disc exactly).
    pub read_offset: i32,
    /// Silent sectors before track 1.
    pub pregap: u32,
    /// Speech-like sound instead of a steady tone.
    pub spoken: bool,
}

impl VirtualDrive {
    pub fn new(name: impl Into<String>, track_seconds: Vec<u32>) -> Self {
        Self { name: name.into(), track_seconds, read_offset: 0, pregap: PREGAP, spoken: false }
    }

    /// The same drive, reading `samples` early (positive) or late (negative) like a real one.
    pub fn with_read_offset(mut self, samples: i32) -> Self {
        self.read_offset = samples;
        self
    }

    /// The same disc, spoken: sentences and pauses, ending with an announcement.
    pub fn spoken(mut self) -> Self {
        self.spoken = true;
        self
    }

    /// The same disc with `sectors` of silence before track 1 (0: track 1 starts the disc).
    pub fn with_pregap(mut self, sectors: u32) -> Self {
        self.pregap = sectors;
        self
    }

    fn toc(&self) -> DiscToc {
        let mut start = self.pregap;
        let mut tracks = Vec::new();
        for (i, seconds) in self.track_seconds.iter().enumerate() {
            tracks.push(TocTrack { number: i as u8 + 1, start_lba: start, is_audio: true });
            start = start.saturating_add(seconds.saturating_mul(FRAMES_PER_SECOND));
        }
        DiscToc { tracks, leadout_lba: start }
    }

    /// The disc's real audio from sample `first` (counted from sector 0; negative is the
    /// lead-in) for `count` samples, as CD audio bytes. Outside the tracks it is silence.
    pub fn true_audio(&self, first: i64, count: usize) -> Vec<u8> {
        let toc = self.toc();
        let per_sector = i64::from(SAMPLES_PER_SECTOR);
        let mut bytes = Vec::with_capacity(count.saturating_mul(4));
        for sample in (first..).take(count) {
            let track = toc
                .tracks
                .iter()
                .rev()
                .find(|t| i64::from(t.start_lba) * per_sector <= sample)
                .filter(|_| sample < i64::from(toc.leadout_lba) * per_sector);
            let value = match track {
                Some(track) if self.spoken => {
                    let start = i64::from(track.start_lba) * per_sector;
                    let length = toc.sectors(track.number).map_or(0, |s| i64::from(s) * per_sector);
                    let last = toc.tracks.last().map(|t| t.number) == Some(track.number);
                    voice(sample - start, length, last, track.number)
                }
                Some(track) => {
                    // A different pitch for every track: an A-minor arpeggio.
                    let hz = [220.0, 261.63, 329.63, 440.0][usize::from(track.number) % 4];
                    let t = (sample - i64::from(track.start_lba) * per_sector) as f32 / 44_100.0;
                    ((t * hz * std::f32::consts::TAU).sin() * 3000.0) as i16
                }
                None => 0,
            };
            bytes.extend_from_slice(&value.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}

/// Speech-like sound at `sample` of a track `length` samples long: a sentence in each 5-second
/// stretch (2.8–4.4 s, so the pauses between run 0.6–2.2 s), syllables within it, a voice with
/// a few harmonics. On a disc's `last` track, the final seconds are an announcement after a
/// long pause.
fn voice(sample: i64, length: i64, last: bool, track: u8) -> i16 {
    let rate = 44_100.0;
    let t = sample as f64 / rate;
    let end = length as f64 / rate;
    let speaking = if last && end > 12.0 && t > end - 7.4 {
        // "End of disc one. Please insert disc two.": a 1.6 s pause, 4.6 s of speech, then quiet.
        t > end - 5.8 && t < end - 1.2
    } else {
        let cell = (t / 5.0).floor();
        let hash = ((cell * 12.9898 + f64::from(track) * 78.233).sin() * 43_758.545).fract().abs();
        t - cell * 5.0 < 2.8 + 1.6 * hash
    };
    if !speaking {
        return 0;
    }
    let tau = std::f64::consts::TAU;
    let syllables = (t * 3.3 * tau).sin().abs().powf(0.6);
    let pitch = 140.0 + 25.0 * (t * 0.7 * tau).sin();
    let tone = (t * pitch * tau).sin() + 0.5 * (t * pitch * 2.0 * tau).sin() + 0.25 * (t * pitch * 3.0 * tau).sin();
    (tone * syllables * 4500.0) as i16
}

impl CdDrive for VirtualDrive {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn identity(&self) -> DriveIdentity {
        DriveIdentity::Model(self.name.clone())
    }

    fn read_toc(&self) -> Result<DiscToc, DriveError> {
        if self.track_seconds.is_empty() { Err(DriveError::NoDisc) } else { Ok(self.toc()) }
    }

    fn read_sectors(
        &self,
        toc: &DiscToc,
        first: u32,
        count: u32,
        sink: &mut dyn FnMut(&[u8], f32) -> bool,
    ) -> Result<(), DriveError> {
        if *toc != self.toc() {
            return Err(DriveError::DiscChanged);
        }
        check_range(toc, first, count)?;
        let mut done = 0;
        while done < count {
            let sectors = SECTORS_PER_CHUNK.min(count - done);
            let start = i64::from(first + done) * i64::from(SAMPLES_PER_SECTOR) - i64::from(self.read_offset);
            let chunk = self.true_audio(start, (sectors * SAMPLES_PER_SECTOR) as usize);
            done += sectors;
            if !sink(&chunk, done as f32 / count as f32) {
                break;
            }
        }
        Ok(())
    }

    /// A fixed disc stays in: see [`crate::VirtualChanger`] for one that comes out.
    fn eject(&self) -> Result<(), DriveError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_whole_sectors_for_each_track() {
        let drive = VirtualDrive::new("Test", vec![2, 3]);
        let toc = drive.read_toc().unwrap();
        assert_eq!(toc.duration(2).map(|d| d.as_secs()), Some(3));
        let track = toc.track(2).unwrap();
        let mut bytes = 0;
        let mut last_fraction = 0.0;
        drive
            .read_sectors(&toc, track.start_lba, toc.sectors(2).unwrap(), &mut |chunk, fraction| {
                bytes += chunk.len();
                last_fraction = fraction;
                true
            })
            .unwrap();
        assert_eq!(bytes, 3 * 75 * SECTOR_BYTES);
        assert_eq!(last_fraction, 1.0);
    }

    #[test]
    fn reads_shifted_by_its_offset_with_silent_lead_in_and_lead_out() {
        let exact = VirtualDrive::new("Test", vec![1]);
        let early = exact.clone().with_read_offset(10);
        let toc = exact.read_toc().unwrap();
        let read = |drive: &VirtualDrive, first: u32, count: u32| {
            let mut bytes = Vec::new();
            drive
                .read_sectors(&toc, first, count, &mut |chunk, _| {
                    bytes.extend_from_slice(chunk);
                    true
                })
                .unwrap();
            bytes
        };
        let start = toc.track(1).unwrap().start_lba;
        let exact_bytes = read(&exact, start, 2);
        let early_bytes = read(&early, start, 2);
        assert_eq!(early_bytes[40..], exact_bytes[..exact_bytes.len() - 40]);
        assert_ne!(exact_bytes, early_bytes);
        // The silent pregap before track 1, and the lead-out after the last.
        assert!(read(&exact, 0, 1).iter().all(|b| *b == 0));
        assert!(exact.true_audio(i64::from(toc.leadout_lba) * 588, 10).iter().all(|b| *b == 0));
        assert!(exact.true_audio(-100, 100).iter().all(|b| *b == 0));
    }

    #[test]
    fn refuses_reads_past_the_lead_out() {
        let drive = VirtualDrive::new("Test", vec![1]);
        let toc = drive.read_toc().unwrap();
        let result = drive.read_sectors(&toc, toc.leadout_lba - 1, 2, &mut |_, _| true);
        assert_eq!(result, Err(DriveError::OutsideDisc(toc.leadout_lba - 1, toc.leadout_lba + 1)));
    }

    #[test]
    fn empty_drive_has_no_disc() {
        assert_eq!(VirtualDrive::new("Empty", vec![]).read_toc(), Err(DriveError::NoDisc));
    }
}
