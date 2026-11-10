use tunebox_model::{PlaylistId, TrackId};

/// Something asked of the library that it cannot do.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LibraryError {
    #[error("there is no track {0} in the library")]
    NoSuchTrack(TrackId),
    #[error("there is no playlist {0} in the library")]
    NoSuchPlaylist(PlaylistId),
    #[error("\"{0}\" is a smart playlist; its tracks are chosen by its rules")]
    SmartPlaylist(String),
    #[error("\"{0}\" is not a smart playlist")]
    NotSmart(String),
    #[error("a playlist needs a name")]
    EmptyName,
}
