//! tunebox-sync — keeping a digital audio player in step with the library.
//!
//! Players are USB mass-storage volumes (Rockbox, FiiO, Sony, Shanling… anything that mounts as
//! a drive) or MTP devices (Android-based players and phones, recent Walkmans; see [`mtp`] and
//! docs/mtp.md). Their files are reached through a [`Storage`], so the rest is the same for
//! both. TuneBox recognises a player by the manifest it leaves in `.tunebox/`, which records
//! the device's name, its sync settings and every file TuneBox put there.
//!
//! Syncing is two steps, so the interesting part is pure and tested:
//! 1. [`plan`] compares what should be on the device with the manifest and decides what to
//!    copy, replace and delete, and whether it fits.
//! 2. [`execute_on`] carries the plan out: deletes first to make room, copies journalled (and
//!    through `*.part` files on folders) so an unplugged player never holds a truncated track
//!    that passes for synced, writes M3U8 playlists, and saves the manifest as it goes.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod device;
mod discover;
mod execute;
pub mod mtp;
#[cfg(test)]
mod mtp_tests;
mod plan;
mod storage;

pub use device::{Device, DeviceError, DeviceManifest, SyncMode, SyncSettings, SyncedFile};
pub use discover::{Access, Candidate, Volume, looks_like_player, scan};
pub use execute::{SyncProgress, SyncReport, execute, execute_on};
pub use mtp::{MtpAddress, MtpLink, MtpStorage};
pub use plan::{CopyOp, DeleteOp, PlaylistFile, SyncItem, SyncPlan, plan};
pub use storage::{FolderStorage, Storage};
