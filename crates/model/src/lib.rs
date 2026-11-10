//! tunebox-model — the vocabulary every other TuneBox crate speaks.
//!
//! Tracks, their metadata and play statistics, playlists (manual and smart), the columns a track
//! list can show and sort by, search, and the folder layout of an organised media folder. The
//! crate is pure: no file system, no threads, no clocks (callers pass "now" in), so everything
//! here is deterministic and unit-tested.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod column;
mod display;
mod format;
mod id;
mod meta;
pub mod organize;
mod playlist;
mod rating;
pub mod search;
pub mod smart;
mod track;

pub use column::{Column, SortOrder, compare_tracks};
pub use display::{format_bytes, format_clock, format_total_time};
pub use format::AudioFormat;
pub use id::{PlaylistId, TrackId};
pub use meta::{NumberOf, TrackMeta};
pub use playlist::{Playlist, PlaylistKind};
pub use rating::Rating;
pub use smart::SmartRules;
pub use track::{FileInfo, PlayStats, Track};
