//! tunebox-playback — what plays, and playing it.
//!
//! - [`PlayQueue`] decides what comes next: list order or shuffled, repeat off, all or one. It is
//!   pure and knows only track ids.
//! - [`Player`] plays one file at a time on its own thread: it decodes ahead into a lock-free ring
//!   buffer that the sound device's callback drains, converting sample rate and channel count on
//!   the way. It reports progress through [`Player::status`] and endings through
//!   [`Player::poll_events`].
//!
//! ```text
//!   Player (handle) ──commands──▶ player thread: Decoder → Converter ──ring──▶ output callback
//!          ◀──status / events───                                         (cpal or NullOutput)
//! ```

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod analysis;
mod convert;
mod engine;
mod output;
mod player;
mod queue;

#[cfg(feature = "cpal")]
pub use output::CpalOutput;
pub use output::{NullOutput, OutputFactory, OutputFormat, Shared};
pub use player::{AudioTap, PlayState, Player, PlayerEvent, Status};
pub use queue::{PlayQueue, RepeatMode};
