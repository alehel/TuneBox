//! tunebox-audio — the only place TuneBox touches codecs.
//!
//! [`Decoder`] streams any playable file as interleaved `f32` blocks and seeks by time;
//! [`PcmWriter`] writes lossless files (FLAC, WAV, AIFF) block by block, for CD rips;
//! [`Mp4AudioWriter`] writes AAC or Apple Lossless MP4 files with chapters, for audiobooks.
//! [`LoudnessMeter`] finds the pauses in speech as it is ripped. All of this sits on
//! craft-audio, which decodes WAV, AIFF, AAC, ALAC and Opus natively and MP3, FLAC and Vorbis
//! through symphonia. Swapping codec back-ends later touches this crate alone.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod decode;
mod encode;
mod loudness;
mod mix;
mod mp4;

pub use craft_audio::AudioError;
pub use decode::{Block, Decoder};
pub use encode::{LosslessFormat, PcmWriter};
pub use loudness::{LoudnessMeter, find_pauses};
pub use mix::mono_mix;
pub use mp4::{Mp4AudioWriter, Mp4Chapter, Mp4Codec, Mp4Settings, read_mp4_chapters};
