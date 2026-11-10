//! tunebox-sync — keeping a digital audio player in step with the library.
//!
//! Players are USB mass-storage volumes (Rockbox, FiiO, Sony, Shanling… anything that mounts as
//! a drive). TuneBox recognises one by the manifest it leaves at `.tunebox/device.json`, which
//! records the device's name, its sync settings and every file TuneBox put there.
//!
//! Syncing is two steps, so the interesting part is pure and tested:
//! 1. [`plan`] compares what should be on the device with the manifest and decides what to
//!    copy, replace and delete, and whether it fits.
//! 2. [`execute`] carries the plan out: deletes first to make room, copies through `*.part`
//!    files so an unplugged player never holds a truncated track, writes M3U8 playlists, and
//!    saves the manifest as it goes.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod device;
mod discover;
mod execute;
mod plan;

pub use device::{Device, DeviceError, DeviceManifest, SyncMode, SyncSettings, SyncedFile};
pub use discover::{Candidate, Volume, looks_like_player, scan};
pub use execute::{SyncProgress, SyncReport, execute};
pub use plan::{CopyOp, DeleteOp, PlaylistFile, SyncItem, SyncPlan, plan};
