//! What can go wrong talking to a device.

use crate::codes::{describe_response, response};

#[derive(Debug, thiserror::Error)]
pub enum MtpError {
    /// The connection failed: unplugged, timed out, or refused by the system.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The device answered an operation with an error code.
    #[error("the player refused: {} (code 0x{code:04X})", describe_response(*code))]
    Refused { operation: u16, code: u16 },
    /// The device sent something that does not follow the protocol.
    #[error("the player sent something TuneBox does not understand: {0}")]
    Protocol(String),
    /// A file or folder name longer than MTP can carry (254 UTF-16 units).
    #[error("the name “{0}” is too long for the player")]
    NameTooLong(String),
}

impl MtpError {
    /// The device's response code, if it answered with one.
    pub fn response_code(&self) -> Option<u16> {
        match self {
            MtpError::Refused { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// Whether the device is out of space.
    pub fn is_full(&self) -> bool {
        self.response_code() == Some(response::STORE_FULL)
    }
}

impl From<MtpError> for std::io::Error {
    fn from(error: MtpError) -> Self {
        match error {
            MtpError::Io(e) => e,
            MtpError::Refused { code: response::INVALID_OBJECT_HANDLE, .. } => {
                std::io::Error::new(std::io::ErrorKind::NotFound, error.to_string())
            }
            MtpError::Refused { code: response::STORE_FULL, .. } => {
                std::io::Error::new(std::io::ErrorKind::StorageFull, error.to_string())
            }
            other => std::io::Error::other(other.to_string()),
        }
    }
}
