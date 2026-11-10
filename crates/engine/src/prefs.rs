//! Preferences: the settings the Preferences window edits, saved next to the library.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tunebox_audio::LosslessFormat;
use tunebox_model::{Column, TagField};
use tunebox_playback::RepeatMode;

/// A folder of music the listener keeps, and whether TuneBox may write in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryFolder {
    pub path: PathBuf,
    /// Never create, change or delete anything in this folder.
    pub read_only: bool,
}

/// Everything the listener can set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Where imported and ripped music is kept.
    pub media_folder: PathBuf,
    /// "Copy files to the media folder when adding to library". Off by default: songs are
    /// played from where they are and nothing is added to the listener's folders.
    pub copy_to_media_folder: bool,
    /// The listener's own music folders, each read-only or writable (see `files`).
    pub library_folders: Vec<LibraryFolder>,
    /// What CDs are ripped to.
    pub rip_format: LosslessFormat,
    /// Look up CD track names on MusicBrainz as soon as a disc is inserted. Off by default:
    /// TuneBox makes no network request the listener did not ask for.
    pub lookup_cd_names_automatically: bool,
    /// Fetch the cover from the Cover Art Archive as soon as a CD's names have been looked up
    /// on MusicBrainz. Off by default, for the same reason.
    pub fetch_cd_covers_automatically: bool,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub visible_columns: Vec<Column>,
    /// Whether the column browser is shown above the track list.
    pub show_browser: bool,
    /// The visualizer look, by the frontend's name for it.
    pub visualizer_style: String,
    /// Show the playing track's details over the visualizer.
    pub visualizer_track_info: bool,
    /// Additional tag fields Get Info always shows, beside the everyday ones. The rest are a
    /// click away under "Show all fields".
    pub info_fields: Vec<TagField>,
    /// Watch the library folders: add songs that appear in them and read songs other programs
    /// change, as it happens. Watching only reads.
    pub watch_library_folders: bool,
}

impl Preferences {
    pub fn with_media_folder(media_folder: PathBuf) -> Self {
        Self { media_folder, ..Self::default() }
    }

    /// Lists `path` as a library folder, or changes its rule if it is listed already.
    pub fn add_library_folder(&mut self, path: PathBuf, read_only: bool) {
        match self.library_folders.iter_mut().find(|f| f.path == path) {
            Some(folder) => folder.read_only = read_only,
            None => self.library_folders.push(LibraryFolder { path, read_only }),
        }
    }

    /// Pins `field` to Get Info, or unpins it. Pinned fields keep the order of `TagField::ALL`,
    /// so they appear in the same sequence however they were chosen.
    pub fn pin_info_field(&mut self, field: TagField, pinned: bool) {
        self.info_fields.retain(|f| *f != field);
        if pinned {
            self.info_fields.push(field);
            // TagField orders as `ALL` lists the fields (a model test holds it to that).
            self.info_fields.sort();
        }
    }

    /// Stops listing `path` (its songs stay in the library).
    pub fn remove_library_folder(&mut self, path: &Path) {
        self.library_folders.retain(|f| f.path != path);
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
            copy_to_media_folder: false,
            library_folders: Vec::new(),
            rip_format: LosslessFormat::Flac,
            lookup_cd_names_automatically: false,
            fetch_cd_covers_automatically: false,
            volume: 0.8,
            shuffle: false,
            repeat: RepeatMode::Off,
            visible_columns: Column::DEFAULT_VISIBLE.to_vec(),
            show_browser: true,
            visualizer_style: String::new(),
            visualizer_track_info: true,
            info_fields: Vec::new(),
            watch_library_folders: true,
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
        prefs.pin_info_field(TagField::Work, true);
        prefs.save(&path).unwrap();
        assert_eq!(Preferences::load(&path, &media), prefs);
        std::fs::write(&path, b"{{{").unwrap();
        assert_eq!(Preferences::load(&path, &media), Preferences::with_media_folder(media));
    }

    #[test]
    fn pinned_fields_keep_the_display_order_and_appear_once() {
        let mut prefs = Preferences::default();
        prefs.pin_info_field(TagField::Mood, true);
        prefs.pin_info_field(TagField::Conductor, true);
        prefs.pin_info_field(TagField::Mood, true);
        assert_eq!(prefs.info_fields, [TagField::Conductor, TagField::Mood]);
        prefs.pin_info_field(TagField::Conductor, false);
        assert_eq!(prefs.info_fields, [TagField::Mood]);
    }
}
