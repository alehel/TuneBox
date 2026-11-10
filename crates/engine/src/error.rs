use tunebox_library::LibraryError;
use tunebox_library::store::StoreError;

/// Something the session could not do, in words a listener can act on.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("could not save preferences: {0}")]
    Preferences(String),
    #[error("{0}")]
    Device(String),
    #[error("there is no disc in the drive")]
    NoDisc,
    #[error("{0} is already in progress")]
    Busy(&'static str),
    #[error(
        "{} is in a read-only library folder, which TuneBox does not change. Choose a media folder in a writable folder in Preferences.",
        .0.display()
    )]
    Protected(std::path::PathBuf),
}
