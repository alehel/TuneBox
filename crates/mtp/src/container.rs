//! Containers: the envelopes every command, data phase and response travels in on USB
//! (MTP 1.1 Appendix D.7, "USB container structure"). Little-endian: length (u32, including
//! this 12-byte header), type (u16), code (u16), transaction id (u32), then the payload: up to
//! five u32 parameters for commands and responses, the dataset or file bytes for data.

use crate::codes::container_type;
use crate::error::MtpError;

pub const HEADER_LEN: usize = 12;

/// The most parameters a command or response carries.
const MAX_PARAMS: usize = 5;

/// A container as received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    pub kind: u16,
    pub code: u16,
    pub transaction: u32,
    pub payload: Vec<u8>,
}

impl Container {
    /// The payload read as u32 parameters (a response's results). Extra bytes are ignored.
    pub fn params(&self) -> Vec<u32> {
        self.payload.chunks_exact(4).take(MAX_PARAMS).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
    }
}

/// The header of a container with `payload_len` bytes after it. A container of 4 GiB or more
/// gives its length as `0xFFFFFFFF`, as the specification asks; the receiver then reads until
/// the transfer ends.
pub fn header(kind: u16, code: u16, transaction: u32, payload_len: u64) -> [u8; HEADER_LEN] {
    let length = u32::try_from(payload_len.saturating_add(HEADER_LEN as u64)).unwrap_or(u32::MAX);
    let mut bytes = [0; HEADER_LEN];
    bytes[0..4].copy_from_slice(&length.to_le_bytes());
    bytes[4..6].copy_from_slice(&kind.to_le_bytes());
    bytes[6..8].copy_from_slice(&code.to_le_bytes());
    bytes[8..12].copy_from_slice(&transaction.to_le_bytes());
    bytes
}

/// A whole command or response container with its parameters.
pub fn with_params(kind: u16, code: u16, transaction: u32, params: &[u32]) -> Vec<u8> {
    let params = params.get(..params.len().min(MAX_PARAMS)).unwrap_or_default();
    let mut bytes = header(kind, code, transaction, (params.len() * 4) as u64).to_vec();
    params.iter().for_each(|p| bytes.extend_from_slice(&p.to_le_bytes()));
    bytes
}

/// A command container.
pub fn command(code: u16, transaction: u32, params: &[u32]) -> Vec<u8> {
    with_params(container_type::COMMAND, code, transaction, params)
}

/// Reads a received container. Its stated length must match what arrived (or be the
/// `0xFFFFFFFF` of a container too long to state it).
pub fn parse(bytes: &[u8]) -> Result<Container, MtpError> {
    let head = bytes
        .get(..HEADER_LEN)
        .ok_or_else(|| MtpError::Protocol(format!("a container of {} bytes is too short", bytes.len())))?;
    let length = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
    let kind = u16::from_le_bytes([head[4], head[5]]);
    let code = u16::from_le_bytes([head[6], head[7]]);
    let transaction = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
    if length != u32::MAX && usize::try_from(length).ok() != Some(bytes.len()) {
        return Err(MtpError::Protocol(format!("a container says {length} bytes but {} arrived", bytes.len())));
    }
    Ok(Container { kind, code, transaction, payload: bytes.get(HEADER_LEN..).unwrap_or_default().to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::{operation, response};

    #[test]
    fn an_open_session_command_byte_by_byte() {
        // OpenSession, transaction 0, session id 1: 16 bytes.
        assert_eq!(
            command(operation::OPEN_SESSION, 0, &[1]),
            [0x10, 0, 0, 0, 0x01, 0x00, 0x02, 0x10, 0, 0, 0, 0, 0x01, 0, 0, 0]
        );
    }

    #[test]
    fn a_response_with_parameters() {
        // SendObjectInfo's response: OK, transaction 7, storage 0x10001, parent 5, handle 42.
        let bytes = [0x18, 0, 0, 0, 0x03, 0x00, 0x01, 0x20, 7, 0, 0, 0, 0x01, 0, 0x01, 0, 5, 0, 0, 0, 42, 0, 0, 0];
        let container = parse(&bytes).unwrap();
        assert_eq!((container.kind, container.code, container.transaction), (3, response::OK, 7));
        assert_eq!(container.params(), [0x0001_0001, 5, 42]);
    }

    #[test]
    fn lengths_that_disagree_are_refused() {
        assert!(parse(&[0x10, 0, 0, 0, 3, 0, 1, 0x20, 0, 0, 0, 0]).is_err());
        assert!(parse(&[1, 2, 3]).is_err());
        assert!(parse(&[0xFF, 0xFF, 0xFF, 0xFF, 2, 0, 9, 0x10, 1, 0, 0, 0, 0xAB]).is_ok(), "unstated length");
    }

    #[test]
    fn huge_payloads_state_no_length() {
        assert_eq!(header(2, 0x100D, 1, 5_000_000_000)[0..4], [0xFF; 4]);
        assert_eq!(header(2, 0x100D, 1, 4)[0..4], [16, 0, 0, 0]);
    }
}
