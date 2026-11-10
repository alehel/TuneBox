//! Preferences: the settings the Preferences window edits, saved next to the library.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tunebox_audio::LosslessFormat;
use tunebox_model::Column;
use tunebox_playback::RepeatMode;

/// Everything the listener can set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Where imported and ripped music is kept.
    pub media_folder: PathBuf,
    /// "Copy files to the media folder when adding to library".
    pub copy_to_media_folder: bool,
    /// What CDs are ripped to.
    pub rip_format: LosslessFormat,
    /// Look up CD track names on MusicBrainz as soon as a disc is inserted. Off by default:
    /// TuneBox makes no network request the listener did not ask for.
    pub lookup_cd_names_automatically: bool,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub visible_columns: Vec<Column>,
    /// Whether the column browser is shown above the track list.
    pub show_browser: bool,
}

impl Preferences {
    pub fn with_media_folder(media_folder: PathBuf) -> Self {
        Self { media_folder, ..Self::default() }
    }

    /// Reads preferences; a missing or unreadable file gives the defaults (and the problem is
    /// logged), since losing preferences must never stop TuneBox from starting.
    pub fn load(path: &Path, default_media_folder: &Path) -> Self {
        let defaults = Self::with_media_folder(default_media_folder.to_path_buf());
        let Ok(bytes) = std::fs::read(path) else { return defaults };
        match serde_json::from_slice::<Preferences>(&bytes) {
            Ok(mut prefs) => {
                if prefs.media_folder.as_os_str().is_empty() {
                    prefs.media_folder = defaults.media_folder;
                }
                prefs
            }
            Err(e) => {
                log::warn!("ignoring unreadable preferences at {}: {e}", path.display());
                defaults
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        craft_atomic::atomic_write(path, &bytes).map_err(|e| e.to_string())
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            media_folder: PathBuf::new(),
            copy_to_media_folder: true,
            rip_format: LosslessFormat::Flac,
            lookup_cd_names_automatically: false,
            volume: 0.8,
            shuffle: false,
            repeat: RepeatMode::Off,
            visible_columns: Column::DEFAULT_VISIBLE.to_vec(),
            show_browser: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_survives_damage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let media = dir.path().join("media");
        let mut prefs = Preferences::with_media_folder(media.clone());
        prefs.rip_format = LosslessFormat::Wav;
        prefs.save(&path).unwrap();
        assert_eq!(Preferences::load(&path, &media), prefs);
        std::fs::write(&path, b"{{{").unwrap();
        assert_eq!(Preferences::load(&path, &media), Preferences::with_media_folder(media));
    }
}
