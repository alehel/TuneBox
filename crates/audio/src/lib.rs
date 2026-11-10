//! tunebox-audio — the only place TuneBox touches codecs.
//!
//! [`Decoder`] streams any playable file as interleaved `f32` blocks and seeks by time;
//! [`PcmWriter`] writes lossless files (FLAC, WAV, AIFF) block by block, for CD rips. Both sit on
//! craft-audio, which decodes WAV, AIFF and AAC natively and MP3, FLAC, ALAC and Vorbis through
//! symphonia. Swapping codec back-ends later touches this crate alone.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod decode;
mod encode;

pub use craft_audio::AudioError;
pub use decode::{Block, Decoder};
pub use encode::{LosslessFormat, PcmWriter};
