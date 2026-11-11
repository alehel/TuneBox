//! tunebox-audiobook — building an audiobook from its staged rip.
//!
//! The book's discs were ripped track by track into lossless files (the engine stages them with
//! `tunebox-rip`). [`build`] reads them back in book order, leaves out removed tracks, cuts the
//! trims, and writes either one chaptered `.m4b` or one file per chapter, tagged as an
//! audiobook, in the media folder. [`check`] says up front whether a book can be built.
//!
//! - `source`: reading a part of a staged track, checked against its expected length.
//! - `output`: one output file in the chosen format.
//! - `build`: the build itself, all or nothing.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod build;
mod output;
mod source;

pub use build::{BuildError, BuildProgress, BuildRequest, build, check};
