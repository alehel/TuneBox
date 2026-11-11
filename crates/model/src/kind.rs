//! What a track is for: music, or an audiobook. Audiobooks have their own place in the library
//! and stay out of Music, shuffle and Up Next.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A track's media kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaKind {
    #[default]
    Music,
    Audiobook,
}

impl MediaKind {
    pub const ALL: [MediaKind; 2] = [MediaKind::Music, MediaKind::Audiobook];

    pub fn is_music(&self) -> bool {
        *self == MediaKind::Music
    }

    /// The name shown, and accepted by `tunebox-cli`: "Music", "Audiobook".
    pub fn name(self) -> &'static str {
        match self {
            MediaKind::Music => "Music",
            MediaKind::Audiobook => "Audiobook",
        }
    }

    /// Reads a name, ignoring case; "book" and "audiobooks" are accepted too.
    pub fn from_name(name: &str) -> Option<MediaKind> {
        match name.trim().to_lowercase().as_str() {
            "music" => Some(MediaKind::Music),
            "audiobook" | "audiobooks" | "book" => Some(MediaKind::Audiobook),
            _ => None,
        }
    }

    /// The kind a file's name suggests: `.m4b` files are audiobooks.
    pub fn from_path(path: &Path) -> MediaKind {
        let is_m4b = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("m4b"));
        if is_m4b { MediaKind::Audiobook } else { MediaKind::Music }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_paths() {
        assert_eq!(MediaKind::from_name(" Audiobooks "), Some(MediaKind::Audiobook));
        assert_eq!(MediaKind::from_name("MUSIC"), Some(MediaKind::Music));
        assert_eq!(MediaKind::from_name("film"), None);
        assert_eq!(MediaKind::from_path(Path::new("/a/Book.M4B")), MediaKind::Audiobook);
        assert_eq!(MediaKind::from_path(Path::new("/a/Song.m4a")), MediaKind::Music);
    }
}
