//! CD drives: what a drive is called, and reading its disc's table of contents and audio.
//!
//! Audio is read by sector range rather than by track, so a rip can apply the drive's read
//! offset (see `offset`) by reading a little of the neighbouring track.

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
    #[error("the drive could not open its tray: {0}")]
    Eject(String),
    #[error("the disc has no track {0}")]
    NoSuchTrack(u8),
    #[error("sectors {0} to {1} are not part of the disc's audio")]
    OutsideDisc(u32, u32),
    #[error("{0}")]
    Device(String),
}

/// Which drive this is, for remembering its read offset. Offsets belong to a make and model,
/// so the model is used when the system reports it; otherwise the drive's device path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DriveIdentity {
    /// Make and model, e.g. "PLEXTOR DVDR PX-716A".
    Model(String),
    /// The device, e.g. "/dev/sr0", when the model is unknown.
    Path(String),
}

impl DriveIdentity {
    /// What a setting for this drive is saved under.
    pub fn key(&self) -> &str {
        match self {
            DriveIdentity::Model(name) | DriveIdentity::Path(name) => name,
        }
    }

    /// The drive in a sentence, e.g. "this drive (PLEXTOR DVDR PX-716A)".
    pub fn describe(&self) -> String {
        match self {
            DriveIdentity::Model(model) => format!("this drive ({model})"),
            DriveIdentity::Path(path) => format!("the drive at {path} (its model is unknown)"),
        }
    }
}

/// Something that holds an audio CD.
pub trait CdDrive: Send + Sync {
    /// How the drive is shown in the source list, e.g. "Audio CD (/dev/sr0)".
    fn name(&self) -> String;

    /// Which drive this is, for its read offset.
    fn identity(&self) -> DriveIdentity;

    /// The disc's table of contents.
    fn read_toc(&self) -> Result<DiscToc, DriveError>;

    /// Streams `count` sectors from sector `first` of the disc described by `toc` to `sink`,
    /// as raw CD audio (44.1 kHz stereo, 16-bit little-endian) exactly as the drive delivers
    /// it, in sector-aligned chunks, with the fraction of the range read so far. The range must
    /// lie within the disc's audio (`0..toc.audio_leadout()`). `sink` returns `false` to stop.
    fn read_sectors(
        &self,
        toc: &DiscToc,
        first: u32,
        count: u32,
        sink: &mut dyn FnMut(&[u8], f32) -> bool,
    ) -> Result<(), DriveError>;

    /// Opens the tray, or pushes the disc out. A drive that cannot is not broken: the listener
    /// takes the disc out by hand.
    fn eject(&self) -> Result<(), DriveError>;
}

/// Refuses a sector range outside the disc's audio, which drives cannot read.
pub(crate) fn check_range(toc: &DiscToc, first: u32, count: u32) -> Result<(), DriveError> {
    let end = first.checked_add(count).ok_or(DriveError::OutsideDisc(first, u32::MAX))?;
    if end > toc.audio_leadout() {
        return Err(DriveError::OutsideDisc(first, end));
    }
    Ok(())
}

#[cfg(feature = "drives")]
pub use physical::PhysicalDrive;

#[cfg(feature = "drives")]
mod physical {
    use super::{CdDrive, DriveError, DriveIdentity, check_range};
    use crate::toc::{DiscToc, TocTrack};
    use cd_da_reader::{CdReader, ReadOptions};
    use std::process::Command;

    /// Sectors asked of the drive at a time: two seconds of audio.
    const SECTORS_PER_READ: u32 = 150;

    /// A drive on this computer, opened afresh for every operation so a handle is never held
    /// across threads or disc changes.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct PhysicalDrive {
        pub path: String,
        pub has_audio_cd: bool,
        /// Make and model, where the system reports them.
        pub model: Option<String>,
    }

    impl PhysicalDrive {
        /// Every optical drive the system reports.
        pub fn list() -> Result<Vec<PhysicalDrive>, DriveError> {
            let drives = CdReader::list_drives().map_err(device)?;
            Ok(drives
                .into_iter()
                .map(|d| PhysicalDrive { model: super::model::of(&d.path), path: d.path, has_audio_cd: d.has_audio_cd })
                .collect())
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

        fn identity(&self) -> DriveIdentity {
            match &self.model {
                Some(model) => DriveIdentity::Model(model.clone()),
                None => DriveIdentity::Path(self.path.clone()),
            }
        }

        fn read_toc(&self) -> Result<DiscToc, DriveError> {
            let toc = to_toc(&self.open()?.read_toc().map_err(|_| DriveError::NoDisc)?);
            if toc.audio_tracks().next().is_none() {
                return Err(DriveError::NotAudio);
            }
            Ok(toc)
        }

        fn read_sectors(
            &self,
            toc: &DiscToc,
            first: u32,
            count: u32,
            sink: &mut dyn FnMut(&[u8], f32) -> bool,
        ) -> Result<(), DriveError> {
            check_range(toc, first, count)?;
            let reader = self.open()?;
            let disc = reader.read_toc().map_err(|_| DriveError::NoDisc)?;
            if to_toc(&disc) != *toc {
                return Err(DriveError::DiscChanged);
            }
            // Reads a bounded run at a time, so a long range is never held in memory at once.
            let options = ReadOptions::default();
            let mut done = 0;
            while done < count {
                let sectors = SECTORS_PER_READ.min(count - done);
                let chunk = reader.read_sector_range(first + done, sectors, &options).map_err(device)?;
                done += sectors;
                if !sink(&chunk, done as f32 / count as f32) {
                    break;
                }
            }
            Ok(())
        }

        /// The system's own eject tool, which knows the drive: cd-da-reader cannot open a tray
        /// yet, and TuneBox has no `unsafe` code to send the drive the command itself.
        fn eject(&self) -> Result<(), DriveError> {
            let status = if cfg!(target_os = "macos") {
                Command::new("drutil").args(["tray", "eject"]).status()
            } else if cfg!(windows) {
                return Err(DriveError::Eject("TuneBox cannot open the tray on Windows yet".into()));
            } else {
                Command::new("eject").arg(&self.path).status()
            };
            match status {
                Ok(status) if status.success() => Ok(()),
                Ok(status) => Err(DriveError::Eject(format!("the eject tool stopped with {status}"))),
                Err(e) => Err(DriveError::Eject(format!("the eject tool could not be run: {e}"))),
            }
        }
    }
}

/// The drive's make and model, read where the system publishes them without special access.
#[cfg(feature = "drives")]
mod model {
    /// "PLEXTOR " and "DVDR   PX-716A " become "PLEXTOR DVDR PX-716A". `None` when both are blank.
    pub(super) fn join(vendor: &str, product: &str) -> Option<String> {
        let words: Vec<&str> = vendor.split_whitespace().chain(product.split_whitespace()).collect();
        (!words.is_empty()).then(|| words.join(" "))
    }

    /// Linux publishes each drive's SCSI inquiry strings in sysfs, readable by anyone.
    #[cfg(target_os = "linux")]
    pub(super) fn of(path: &str) -> Option<String> {
        let name = std::path::Path::new(path).file_name()?.to_str()?;
        let device = std::path::Path::new("/sys/class/block").join(name).join("device");
        let read = |file: &str| std::fs::read_to_string(device.join(file)).unwrap_or_default();
        join(&read("vendor"), &read("model"))
    }

    /// Elsewhere cd-da-reader does not report the model, so the drive is known by its path.
    #[cfg(not(target_os = "linux"))]
    pub(super) fn of(_path: &str) -> Option<String> {
        None
    }

    #[cfg(test)]
    mod tests {
        use super::join;

        #[test]
        fn joins_inquiry_strings() {
            assert_eq!(join("PLEXTOR \n", "DVDR   PX-716A \n"), Some("PLEXTOR DVDR PX-716A".into()));
            assert_eq!(join("", "  "), None);
            assert_eq!(join("", "BD-RE BH16NS40"), Some("BD-RE BH16NS40".into()));
        }

        #[test]
        fn an_unknown_device_has_no_model() {
            assert_eq!(super::of("/dev/no-such-drive"), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_describe_the_drive() {
        let model = DriveIdentity::Model("PLEXTOR DVDR PX-716A".into());
        assert_eq!(model.key(), "PLEXTOR DVDR PX-716A");
        assert_eq!(model.describe(), "this drive (PLEXTOR DVDR PX-716A)");
        assert_eq!(DriveIdentity::Path("/dev/sr0".into()).describe(), "the drive at /dev/sr0 (its model is unknown)");
    }
}
