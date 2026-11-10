//! The audio file formats TuneBox knows by name.

use serde::{Deserialize, Serialize};

/// An audio file's format, as shown in the Kind column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioFormat {
    Mp3,
    Aac,
    Alac,
    Flac,
    Wav,
    Aiff,
    Vorbis,
    Opus,
}

impl AudioFormat {
    /// Every format, in the order the import dialog lists them.
    pub const ALL: [AudioFormat; 8] = [
        AudioFormat::Mp3,
        AudioFormat::Aac,
        AudioFormat::Alac,
        AudioFormat::Flac,
        AudioFormat::Wav,
        AudioFormat::Aiff,
        AudioFormat::Vorbis,
        AudioFormat::Opus,
    ];

    /// Guesses the format from a file extension (case-insensitive). `m4a` is reported as AAC;
    /// the tag reader refines it to ALAC when the stream says so.
    /// Every file extension TuneBox reads, for file pickers.
    pub const EXTENSIONS: [&'static str; 14] =
        ["mp3", "m4a", "m4b", "mp4", "aac", "flac", "wav", "wave", "aif", "aiff", "aifc", "ogg", "oga", "opus"];

    pub fn from_extension(ext: &str) -> Option<AudioFormat> {
        let format = match ext.to_ascii_lowercase().as_str() {
            "mp3" => AudioFormat::Mp3,
            "m4a" | "m4b" | "mp4" | "aac" => AudioFormat::Aac,
            "flac" => AudioFormat::Flac,
            "wav" | "wave" => AudioFormat::Wav,
            "aif" | "aiff" | "aifc" => AudioFormat::Aiff,
            "ogg" | "oga" => AudioFormat::Vorbis,
            "opus" => AudioFormat::Opus,
            _ => return None,
        };
        Some(format)
    }

    /// The extension TuneBox writes for this format.
    pub fn extension(self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Aac | AudioFormat::Alac => "m4a",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
            AudioFormat::Aiff => "aiff",
            AudioFormat::Vorbis => "ogg",
            AudioFormat::Opus => "opus",
        }
    }

    /// The description shown in the Kind column, e.g. "FLAC audio file".
    pub fn kind_label(self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "MPEG audio file",
            AudioFormat::Aac => "AAC audio file",
            AudioFormat::Alac => "Apple Lossless audio file",
            AudioFormat::Flac => "FLAC audio file",
            AudioFormat::Wav => "WAV audio file",
            AudioFormat::Aiff => "AIFF audio file",
            AudioFormat::Vorbis => "Ogg Vorbis audio file",
            AudioFormat::Opus => "Opus audio file",
        }
    }

    /// Whether the format keeps every bit of the original audio.
    pub fn is_lossless(self) -> bool {
        matches!(self, AudioFormat::Alac | AudioFormat::Flac | AudioFormat::Wav | AudioFormat::Aiff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_extensions_are_exactly_the_readable_ones() {
        for ext in AudioFormat::EXTENSIONS {
            assert!(AudioFormat::from_extension(ext).is_some(), "{ext}");
        }
        for format in AudioFormat::ALL {
            assert!(AudioFormat::EXTENSIONS.contains(&format.extension()), "{format:?}");
        }
    }

    #[test]
    fn extensions_round_trip_for_formats_with_their_own_extension() {
        for format in AudioFormat::ALL {
            let back = AudioFormat::from_extension(format.extension());
            let expected = if format == AudioFormat::Alac { AudioFormat::Aac } else { format };
            assert_eq!(back, Some(expected), "{format:?}");
        }
    }

    #[test]
    fn extension_matching_ignores_case() {
        assert_eq!(AudioFormat::from_extension("FLAC"), Some(AudioFormat::Flac));
        assert_eq!(AudioFormat::from_extension("txt"), None);
    }
}
