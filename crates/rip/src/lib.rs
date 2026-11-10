//! tunebox-rip — importing audio CDs.
//!
//! 1. A [`CdDrive`] reports the disc's [`DiscToc`] (track layout) and streams a track's raw audio.
//!    [`PhysicalDrive`] reads real drives through cd-da-reader; [`VirtualDrive`] serves a
//!    synthetic disc for tests and demos.
//! 2. The table of contents gives the disc's MusicBrainz [`disc_id`], with which
//!    [`musicbrainz::lookup`] finds album and track names (only when asked: it is the one
//!    network call TuneBox makes).
//! 3. [`rip`] reads the chosen tracks, encodes them losslessly into the media folder, tags them
//!    and returns them as `NewTrack`s for the library.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod discid;
mod drive;
mod info;
#[cfg(feature = "musicbrainz")]
pub mod musicbrainz;
mod rip;
mod toc;
mod virtual_drive;

pub use discid::{disc_id, freedb_id};
#[cfg(feature = "drives")]
pub use drive::PhysicalDrive;
pub use drive::{CdDrive, DriveError};
pub use info::DiscInfo;
pub use rip::{RipProgress, RipReport, RipRequest, rip};
pub use toc::{DiscToc, FRAMES_PER_SECOND, SECTOR_BYTES, TocTrack};
pub use virtual_drive::VirtualDrive;
