//! A drive with a made-up disc in it: each track a quiet tone. It makes the whole CD import
//! path testable without hardware, and gives demos and screenshots a disc to show.

use crate::drive::{CdDrive, DriveError};
use crate::toc::{DiscToc, FRAMES_PER_SECOND, SECTOR_BYTES, TocTrack};

/// Stereo frames in one sector.
const FRAMES_PER_SECTOR: usize = SECTOR_BYTES / 4;
/// Sectors returned per chunk, as a real drive reads in bursts.
const SECTORS_PER_CHUNK: u32 = 75;
/// The 2-second pregap before track 1.
const PREGAP: u32 = 150;

/// A synthetic disc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualDrive {
    pub name: String,
    /// Each track's length in whole seconds.
    pub track_seconds: Vec<u32>,
}

impl VirtualDrive {
    pub fn new(name: impl Into<String>, track_seconds: Vec<u32>) -> Self {
        Self { name: name.into(), track_seconds }
    }

    fn toc(&self) -> DiscToc {
        let mut start = PREGAP;
        let mut tracks = Vec::new();
        for (i, seconds) in self.track_seconds.iter().enumerate() {
            tracks.push(TocTrack { number: i as u8 + 1, start_lba: start, is_audio: true });
            start = start.saturating_add(seconds.saturating_mul(FRAMES_PER_SECOND));
        }
        DiscToc { tracks, leadout_lba: start }
    }
}

impl CdDrive for VirtualDrive {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn read_toc(&self) -> Result<DiscToc, DriveError> {
        if self.track_seconds.is_empty() { Err(DriveError::NoDisc) } else { Ok(self.toc()) }
    }

    fn read_track(
        &self,
        toc: &DiscToc,
        number: u8,
        sink: &mut dyn FnMut(&[u8], f32) -> bool,
    ) -> Result<(), DriveError> {
        if *toc != self.toc() {
            return Err(DriveError::DiscChanged);
        }
        let total = toc.sectors(number).ok_or(DriveError::NoSuchTrack(number))?;
        // A different pitch for every track: an A-minor arpeggio.
        let hz = [220.0, 261.63, 329.63, 440.0][usize::from(number) % 4];
        let mut sector = 0;
        while sector < total {
            let count = SECTORS_PER_CHUNK.min(total - sector);
            let first_frame = sector as usize * FRAMES_PER_SECTOR;
            let frames = count as usize * FRAMES_PER_SECTOR;
            let mut chunk = Vec::with_capacity(frames * 4);
            for frame in first_frame..first_frame + frames {
                let t = frame as f32 / 44_100.0;
                let sample = ((t * hz * std::f32::consts::TAU).sin() * 3000.0) as i16;
                chunk.extend_from_slice(&sample.to_le_bytes());
                chunk.extend_from_slice(&sample.to_le_bytes());
            }
            sector += count;
            if !sink(&chunk, sector as f32 / total as f32) {
                break;
            }
        }
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
        let mut bytes = 0;
        let mut last_fraction = 0.0;
        drive
            .read_track(&toc, 2, &mut |chunk, fraction| {
                bytes += chunk.len();
                last_fraction = fraction;
                true
            })
            .unwrap();
        assert_eq!(bytes, 3 * 75 * SECTOR_BYTES);
        assert_eq!(last_fraction, 1.0);
    }

    #[test]
    fn empty_drive_has_no_disc() {
        assert_eq!(VirtualDrive::new("Empty", vec![]).read_toc(), Err(DriveError::NoDisc));
    }
}
