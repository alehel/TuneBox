//! Which player a device's files are for, and what that changes about syncing.
//!
//! Many players simply read files off a card or their own memory, with no protocol to ask
//! what they can play: Rockbox players, and small file players such as Snowsky's Echo Mini and
//! Echo Nano. The listener says which player it is when setting it up; TuneBox records it in
//! the manifest on the device itself (`.tunebox/device.json`), so a microSD card is recognised
//! the same in the player and in a card reader. See docs/file-players.md.

use serde::{Deserialize, Serialize};

/// The player a device is synced for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayerModel {
    /// Anything that plays files: every song is copied.
    #[default]
    Generic,
    Rockbox,
    SnowskyEchoMini,
    SnowskyEchoNano,
}

/// The formats Snowsky's Echo players play, from the published specifications. OGG and Opus
/// are left out: OGG is listed but owners report it does not play, and a song left off with a
/// reason is better than one copied that will not play. AIFF is not listed.
const SNOWSKY_FORMATS: &[&str] = &["mp3", "flac", "wav", "ape", "m4a", "wma", "dsf", "dff"];

impl PlayerModel {
    pub const ALL: [PlayerModel; 4] =
        [PlayerModel::Generic, PlayerModel::Rockbox, PlayerModel::SnowskyEchoMini, PlayerModel::SnowskyEchoNano];

    /// The name shown to the listener.
    pub fn label(self) -> &'static str {
        match self {
            PlayerModel::Generic => "Other player",
            PlayerModel::Rockbox => "Rockbox",
            PlayerModel::SnowskyEchoMini => "Snowsky Echo Mini",
            PlayerModel::SnowskyEchoNano => "Snowsky Echo Nano",
        }
    }

    /// The name used on the command line (`--player echo-mini`).
    pub fn id(self) -> &'static str {
        match self {
            PlayerModel::Generic => "generic",
            PlayerModel::Rockbox => "rockbox",
            PlayerModel::SnowskyEchoMini => "echo-mini",
            PlayerModel::SnowskyEchoNano => "echo-nano",
        }
    }

    pub fn from_id(id: &str) -> Option<PlayerModel> {
        PlayerModel::ALL.into_iter().find(|model| model.id().eq_ignore_ascii_case(id.trim()))
    }

    /// Whether the player plays files with this extension. `None`: TuneBox does not know the
    /// player's formats, so it copies everything.
    fn formats(self) -> Option<&'static [&'static str]> {
        match self {
            PlayerModel::Generic | PlayerModel::Rockbox => None,
            PlayerModel::SnowskyEchoMini | PlayerModel::SnowskyEchoNano => Some(SNOWSKY_FORMATS),
        }
    }

    /// Whether a file named `path` is one the player can play.
    pub fn plays(self, path: &str) -> bool {
        let Some(formats) = self.formats() else { return true };
        let extension = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        formats.contains(&extension.as_str())
    }

    /// Whether the player reads M3U8 playlists, so writing them is on by default. Owners
    /// disagree about the Echo players, so there it starts off (the listener can turn it on).
    pub fn reads_playlists(self) -> bool {
        !matches!(self, PlayerModel::SnowskyEchoMini | PlayerModel::SnowskyEchoNano)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_ids() {
        assert!(PlayerModel::Generic.plays("Music/a.opus"));
        assert!(PlayerModel::SnowskyEchoMini.plays("Music/A/01 Song.FLAC"));
        assert!(!PlayerModel::SnowskyEchoNano.plays("Music/A/01 Song.ogg"));
        assert!(!PlayerModel::SnowskyEchoNano.plays("Music/A/no extension"));
        for model in PlayerModel::ALL {
            assert_eq!(PlayerModel::from_id(model.id()), Some(model));
        }
        assert_eq!(PlayerModel::from_id("Echo-Mini"), Some(PlayerModel::SnowskyEchoMini));
        assert_eq!(PlayerModel::from_id("zune"), None);
    }
}
