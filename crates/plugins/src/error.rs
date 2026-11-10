//! What can go wrong with a plug-in, in words the listener can act on.

use crate::drawing::InvalidDrawing;

/// Why a plug-in file could not be loaded. Shown once, when the folder is scanned.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum LoadError {
    #[error("it could not be read ({0})")]
    Unreadable(String),
    #[error("it is {0} bytes; plug-ins may be at most {max} bytes", max = crate::sandbox::MAX_FILE_BYTES)]
    TooBig(u64),
    #[error("it is not a WebAssembly module TuneBox can run ({0})")]
    NotWebAssembly(String),
    #[error("it asks for things TuneBox does not give plug-ins ({0})")]
    HasImports(String),
    #[error("it has no `{0}` export of the right kind; is it a TuneBox visualizer plug-in?")]
    MissingExport(&'static str),
    #[error("it was built for plug-in version {found}, and this TuneBox runs version {expected}")]
    WrongVersion { found: u32, expected: u32 },
    #[error("{0}")]
    BadInfo(String),
    #[error("it failed while starting: {0}")]
    Failed(Fault),
    #[error("another plug-in, “{0}”, already uses its id")]
    DuplicateId(String),
}

/// Why a running plug-in was stopped. Any fault turns the plug-in off for the session.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Fault {
    #[error("it took too long to draw a frame")]
    OutOfTime,
    #[error("it kept drawing too slowly")]
    TooSlow,
    #[error("it hit an error ({0})")]
    Trapped(String),
    #[error("it drew something TuneBox cannot show ({0})")]
    InvalidDrawing(#[from] InvalidDrawing),
    #[error("it broke the plug-in runtime ({0})")]
    Crashed(String),
}

impl Fault {
    /// The notice for the listener when the look called `name` is turned off.
    pub fn notice(&self, name: &str) -> String {
        format!("The “{name}” visualizer stopped working and was turned off: {self}.")
    }
}
