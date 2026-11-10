//! Playlists: hand-picked lists of tracks, or smart playlists that pick their own.

use crate::{PlaylistId, SmartRules, TrackId};
use serde::{Deserialize, Serialize};

/// A playlist in the source list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: PlaylistId,
    pub name: String,
    pub kind: PlaylistKind,
}

/// What decides a playlist's contents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaylistKind {
    /// Tracks the listener put there, in their order. A track may appear more than once.
    Manual { tracks: Vec<TrackId> },
    /// Tracks chosen by rules, re-evaluated as the library changes.
    Smart(SmartRules),
}

impl Playlist {
    pub fn is_smart(&self) -> bool {
        matches!(self.kind, PlaylistKind::Smart(_))
    }

    /// The hand-picked tracks, or `None` for a smart playlist.
    pub fn manual_tracks(&self) -> Option<&[TrackId]> {
        match &self.kind {
            PlaylistKind::Manual { tracks } => Some(tracks),
            PlaylistKind::Smart(_) => None,
        }
    }
}
