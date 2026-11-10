//! Where a track lives inside an organised media folder:
//! `Artist/Album/1-03 Title.ext`, with names made safe for every file system TuneBox syncs to.
//!
//! Names are always made safe for the strictest file system (Windows and FAT players), whatever
//! TuneBox runs on, since music moves between them. The one rule that differs by platform is
//! whether names that differ only in case are the same name ([`CaseRule`]); callers pass it in,
//! so the Windows and macOS behaviour is tested on Linux too.

use crate::TrackMeta;
use std::path::{Path, PathBuf};

/// Longest single path component we write. FAT32 and exFAT allow 255 UTF-16 units; leaving room
/// keeps paths valid on players that add their own prefixes.
const MAX_COMPONENT_CHARS: usize = 120;

/// The path of a track relative to the media folder.
pub fn relative_path(meta: &TrackMeta, fallback_title: &str, extension: &str) -> PathBuf {
    let artist = component(meta.filing_artist(), "Unknown Artist");
    let album = component(&meta.album, "Unknown Album");
    let title = component(meta.display_title(fallback_title), "Untitled");
    let file_name = match (meta.disc.number, meta.disc.total, meta.track.number) {
        (Some(disc), Some(total), Some(track)) if total > 1 => format!("{disc}-{track:02} {title}"),
        (_, _, Some(track)) => format!("{track:02} {title}"),
        _ => title,
    };
    let mut path = PathBuf::from(artist);
    path.push(album);
    path.push(format!("{file_name}.{extension}"));
    path
}

/// A name made safe as one path component on Windows, macOS, Linux and FAT-formatted players.
pub fn component(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .take(MAX_COMPONENT_CHARS)
        .collect();
    // Windows drops trailing dots and spaces, and a leading dot hides the file elsewhere.
    let trimmed = cleaned.trim().trim_end_matches('.').trim_start_matches('.').trim();
    if trimmed.is_empty() || is_reserved_windows_name(trimmed) { fallback.to_owned() } else { trimmed.to_owned() }
}

/// Whether a file system tells names apart by case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseRule {
    /// `Song.flac` and `song.flac` are two files (Linux).
    Sensitive,
    /// `Song.flac` and `song.flac` are one file (Windows, macOS, FAT and exFAT players).
    Insensitive,
}

impl CaseRule {
    /// The rule of the platform TuneBox runs on, for its usual disks.
    pub fn native() -> CaseRule {
        if cfg!(any(windows, target_os = "macos")) { CaseRule::Insensitive } else { CaseRule::Sensitive }
    }

    /// A key that is equal for two paths exactly when this rule makes them the same name.
    pub fn key(self, path: &Path) -> String {
        let text = path.to_string_lossy();
        match self {
            CaseRule::Sensitive => text.into_owned(),
            CaseRule::Insensitive => text.to_lowercase(),
        }
    }

    pub fn same_name(self, a: &Path, b: &Path) -> bool {
        self.key(a) == self.key(b)
    }
}

/// Whether two different paths differ only in case, so that on a case-insensitive disk they
/// are the same file and a rename between them needs a step in between.
pub fn differ_only_in_case(a: &Path, b: &Path) -> bool {
    a != b && CaseRule::Insensitive.same_name(a, b)
}

fn is_reserved_windows_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.chars().last().is_some_and(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NumberOf;

    fn meta() -> TrackMeta {
        TrackMeta {
            title: "Song: Part 1/2".into(),
            artist: "AC/DC".into(),
            album: "Live?".into(),
            track: NumberOf { number: Some(3), total: Some(10) },
            ..TrackMeta::default()
        }
    }

    #[test]
    fn builds_artist_album_track_path() {
        assert_eq!(relative_path(&meta(), "x", "flac"), PathBuf::from("AC_DC/Live_/03 Song_ Part 1_2.flac"));
    }

    #[test]
    fn multi_disc_albums_prefix_the_disc() {
        let mut m = meta();
        m.disc = NumberOf::new(2, 2);
        assert!(relative_path(&m, "x", "flac").ends_with("2-03 Song_ Part 1_2.flac"));
    }

    #[test]
    fn unsafe_names_fall_back() {
        assert_eq!(component("...", "Unknown"), "Unknown");
        assert_eq!(component("CON", "Unknown"), "Unknown");
        assert_eq!(component("Concert", "Unknown"), "Concert");
        assert_eq!(component(" Trailing. ", "Unknown"), "Trailing");
    }

    #[test]
    fn windows_rules_apply_everywhere() {
        for reserved in ["con", "Aux.flac", "NUL .txt", "com1", "LPT9", "CONIN$"] {
            assert_eq!(component(reserved, "Safe"), "Safe", "{reserved}");
        }
        assert_eq!(component("COM10", "Safe"), "COM10");
        assert_eq!(component("a<b>c:d\"e|f?g*h", "Safe"), "a_b_c_d_e_f_g_h");
        assert_eq!(component("tab\there", "Safe"), "tab_here");
        assert_eq!(component(".hidden", "Safe"), "hidden", "a leading dot would hide the file");
    }

    #[test]
    fn case_rules_decide_what_is_the_same_name() {
        let (a, b) = (Path::new("/m/Song.flac"), Path::new("/m/song.FLAC"));
        assert!(CaseRule::Insensitive.same_name(a, b));
        assert!(!CaseRule::Sensitive.same_name(a, b));
        assert!(differ_only_in_case(a, b));
        assert!(!differ_only_in_case(a, a));
        assert!(!differ_only_in_case(a, Path::new("/m/Other.flac")));
    }

    #[test]
    fn long_names_are_capped() {
        let long = "x".repeat(500);
        assert_eq!(component(&long, "u").chars().count(), MAX_COMPONENT_CHARS);
    }
}
