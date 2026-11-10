//! CD drives.

use crate::toc::DiscToc;

/// Why a drive could not do what was asked.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DriveError {
    #[error("there is no disc in the drive")]
    NoDisc,
    #[error("the disc in the drive has no audio tracks")]
    NotAudio,
    #[error("the disc in the drive was changed")]
    DiscChanged,
    #[error("the disc has no track {0}")]
    NoSuchTrack(u8),
    #[error("{0}")]
    Device(String),
}

/// Something that holds an audio CD.
pub trait CdDrive: Send {
    /// How the drive is shown in the source list, e.g. "Audio CD (/dev/sr0)".
    fn name(&self) -> String;

    /// The disc's table of contents.
    fn read_toc(&self) -> Result<DiscToc, DriveError>;

    /// Streams track `number` of the disc described by `toc` to `sink`, as raw CD audio
    /// (44.1 kHz stereo, 16-bit little-endian) in sector-aligned chunks, together with the
    /// fraction of the track read so far. `sink` returns `false` to stop early.
    fn read_track(&self, toc: &DiscToc, number: u8, sink: &mut dyn FnMut(&[u8], f32) -> bool) -> Result<(), DriveError>;
}

#[cfg(feature = "drives")]
pub use physical::PhysicalDrive;

#[cfg(feature = "drives")]
mod physical {
    use super::{CdDrive, DriveError};
    use crate::toc::{DiscToc, TocTrack};
    use cd_da_reader::CdReader;

    /// A drive on this computer, opened afresh for every operation so a handle is never held
    /// across threads or disc changes.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct PhysicalDrive {
        pub path: String,
        pub has_audio_cd: bool,
    }

    impl PhysicalDrive {
        /// Every optical drive the system reports.
        pub fn list() -> Result<Vec<PhysicalDrive>, DriveError> {
            let drives = CdReader::list_drives().map_err(device)?;
            Ok(drives.into_iter().map(|d| PhysicalDrive { path: d.path, has_audio_cd: d.has_audio_cd }).collect())
        }

        fn open(&self) -> Result<CdReader, DriveError> {
            CdReader::open_path(&self.path).map_err(device)
        }
    }

    fn device(error: cd_da_reader::CdReaderError) -> DriveError {
        DriveError::Device(error.to_string())
    }

    fn to_toc(toc: &cd_da_reader::Toc) -> DiscToc {
        DiscToc {
            tracks: toc
                .tracks
                .iter()
                .map(|t| TocTrack { number: t.number, start_lba: t.start_lba, is_audio: t.is_audio })
                .collect(),
            leadout_lba: toc.leadout_lba,
        }
    }

    impl CdDrive for PhysicalDrive {
        fn name(&self) -> String {
            format!("Audio CD ({})", self.path)
        }

        fn read_toc(&self) -> Result<DiscToc, DriveError> {
            let toc = to_toc(&self.open()?.read_toc().map_err(|_| DriveError::NoDisc)?);
            if toc.audio_tracks().next().is_none() {
                return Err(DriveError::NotAudio);
            }
            Ok(toc)
        }

        fn read_track(&self, toc: &DiscToc, number: u8, sink: &mut dyn FnMut(&[u8], f32) -> bool) -> Result<(), DriveError> {
            let reader = self.open()?;
            let disc = reader.read_toc().map_err(|_| DriveError::NoDisc)?;
            if to_toc(&disc) != *toc {
                return Err(DriveError::DiscChanged);
            }
            let mut stream = reader.open_track_stream(&disc, number).map_err(device)?;
            let total = stream.total_sectors().max(1) as f32;
            while let Some(chunk) = stream.next_chunk().map_err(device)? {
                if !sink(&chunk, stream.current_sector() as f32 / total) {
                    break;
                }
            }
            Ok(())
        }
    }
}
