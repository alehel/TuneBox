use tunebox_library::LibraryError;
use tunebox_library::store::StoreError;

/// Something the session could not do, in words a listener can act on.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Lock(#[from] tunebox_library::LockError),
    #[error("could not save preferences: {0}")]
    Preferences(String),
    #[error("{0}")]
    Device(String),
    #[error("there is no disc in the drive")]
    NoDisc,
    #[error(
        "TuneBox does not know which MusicBrainz release this disc is. Get Track Names first: the cover is found by the release they come from"
    )]
    NoRelease,
    #[error("{0} is already in progress")]
    Busy(&'static str),
    #[error("{0}")]
    Locate(String),
    #[error("{0}")]
    Artwork(String),
    #[error(transparent)]
    Pattern(#[from] tunebox_model::naming::PatternError),
    #[error("{0}")]
    Rename(String),
    #[error(
        "{} is in a read-only library folder, which TuneBox does not change. Choose a media folder in a writable folder in Preferences.",
        .0.display()
    )]
    Protected(std::path::PathBuf),
}
