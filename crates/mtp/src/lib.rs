//! tunebox-mtp — talking to MTP players (Android-based players and phones, recent Walkmans).
//!
//! Written from the public specifications (MTP 1.1 from the USB-IF, PTP / ISO 15740); see
//! docs/mtp.md for the investigation behind it. The layers, bottom up:
//! - [`codes`], [`container`], [`dataset`]: the bytes of the protocol. Pure, and tested on
//!   hand-built packets.
//! - [`Session`]: the operations TuneBox needs (storages, folders, sending, reading and deleting
//!   files) over any [`Transport`].
//! - [`usb`]: the USB transport and finding devices, through `nusb` on Linux and macOS.
//! - `simulator` (feature `simulator`): an in-memory device for tests.
//!
//! The sync logic that uses this lives in tunebox-sync (`MtpStorage`).

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod codes;
pub mod container;
pub mod dataset;
mod error;
mod session;
#[cfg(any(test, feature = "simulator"))]
pub mod simulator;
mod transport;
pub mod usb;

pub use dataset::{DeviceInfo, ObjectInfo, StorageInfo};
pub use error::MtpError;
pub use session::Session;
pub use transport::Transport;
