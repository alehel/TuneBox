//! tunebox-library — the music library: every track TuneBox knows about and every playlist.
//!
//! [`Library`] is a plain in-memory value with one owner (the engine's session). It keeps its
//! invariants itself: ids are unique and never reused, a file is in the library at most once,
//! and playlists only refer to tracks that exist. [`store`] reads and writes it as a versioned
//! JSON file, atomically.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod browse;
mod error;
mod library;
pub mod store;
mod summary;

pub use browse::{BrowseFilter, Browser};
pub use error::LibraryError;
pub use library::{Library, NewTrack};
pub use summary::Summary;
