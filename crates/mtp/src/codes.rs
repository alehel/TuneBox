//! The numbers of the protocol: operation, response, format and container codes, as the PTP
//! (ISO 15740) and MTP 1.1 specifications define them. Only those TuneBox uses are listed.

/// Container types (MTP 1.1 Appendix D, "USB container").
pub mod container_type {
    pub const COMMAND: u16 = 1;
    pub const DATA: u16 = 2;
    pub const RESPONSE: u16 = 3;
}

/// Operation codes.
pub mod operation {
    pub const GET_DEVICE_INFO: u16 = 0x1001;
    pub const OPEN_SESSION: u16 = 0x1002;
    pub const CLOSE_SESSION: u16 = 0x1003;
    pub const GET_STORAGE_IDS: u16 = 0x1004;
    pub const GET_STORAGE_INFO: u16 = 0x1005;
    pub const GET_OBJECT_HANDLES: u16 = 0x1007;
    pub const GET_OBJECT_INFO: u16 = 0x1008;
    pub const GET_OBJECT: u16 = 0x1009;
    pub const DELETE_OBJECT: u16 = 0x100B;
    pub const SEND_OBJECT_INFO: u16 = 0x100C;
    pub const SEND_OBJECT: u16 = 0x100D;
}

/// Response codes.
pub mod response {
    pub const OK: u16 = 0x2001;
    pub const GENERAL_ERROR: u16 = 0x2002;
    pub const SESSION_NOT_OPEN: u16 = 0x2003;
    pub const OPERATION_NOT_SUPPORTED: u16 = 0x2005;
    pub const INCOMPLETE_TRANSFER: u16 = 0x2007;
    pub const INVALID_STORAGE_ID: u16 = 0x2008;
    pub const INVALID_OBJECT_HANDLE: u16 = 0x2009;
    pub const STORE_FULL: u16 = 0x200C;
    pub const OBJECT_WRITE_PROTECTED: u16 = 0x200D;
    pub const STORE_READ_ONLY: u16 = 0x200E;
    pub const ACCESS_DENIED: u16 = 0x200F;
    pub const STORE_NOT_AVAILABLE: u16 = 0x2013;
    pub const DEVICE_BUSY: u16 = 0x2019;
    pub const INVALID_PARENT_OBJECT: u16 = 0x201A;
    pub const INVALID_PARAMETER: u16 = 0x201D;
    pub const SESSION_ALREADY_OPEN: u16 = 0x201E;
    pub const TRANSACTION_CANCELLED: u16 = 0x201F;
}

/// Object format codes.
pub mod format {
    pub const UNDEFINED: u16 = 0x3000;
    /// A folder.
    pub const ASSOCIATION: u16 = 0x3001;
    pub const TEXT: u16 = 0x3004;
    pub const AIFF: u16 = 0x3007;
    pub const WAV: u16 = 0x3008;
    pub const MP3: u16 = 0x3009;
    pub const WMA: u16 = 0xB901;
    pub const OGG: u16 = 0xB902;
    pub const AAC: u16 = 0xB903;
    pub const FLAC: u16 = 0xB906;
    pub const MP4_CONTAINER: u16 = 0xB982;
    pub const M3U_PLAYLIST: u16 = 0xBA11;
}

/// The association type of an ordinary folder.
pub const GENERIC_FOLDER: u16 = 0x0001;

/// `GetObjectHandles`' parent for "the top level of the storage".
pub const ROOT_PARENT: u32 = 0xFFFF_FFFF;

/// The vendor extension id of MTP (Microsoft).
pub const MTP_VENDOR_EXTENSION: u32 = 6;

/// The format code for a file named `name`, from its extension.
pub fn format_for_name(name: &str) -> u16 {
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match extension.as_str() {
        "mp3" => format::MP3,
        "flac" => format::FLAC,
        "ogg" | "oga" | "opus" => format::OGG,
        "m4a" | "mp4" | "m4b" => format::MP4_CONTAINER,
        "aac" => format::AAC,
        "wma" => format::WMA,
        "wav" => format::WAV,
        "aif" | "aiff" => format::AIFF,
        "m3u" | "m3u8" => format::M3U_PLAYLIST,
        "txt" | "json" => format::TEXT,
        _ => format::UNDEFINED,
    }
}

/// A short description of a response code, for messages.
pub fn describe_response(code: u16) -> &'static str {
    match code {
        response::OK => "OK",
        response::GENERAL_ERROR => "general error",
        response::SESSION_NOT_OPEN => "session not open",
        response::OPERATION_NOT_SUPPORTED => "operation not supported",
        response::INCOMPLETE_TRANSFER => "incomplete transfer",
        response::INVALID_STORAGE_ID => "no such storage",
        response::INVALID_OBJECT_HANDLE => "no such file",
        response::STORE_FULL => "the storage is full",
        response::OBJECT_WRITE_PROTECTED => "the file is write-protected",
        response::STORE_READ_ONLY => "the storage is read-only",
        response::ACCESS_DENIED => "access denied",
        response::STORE_NOT_AVAILABLE => "the storage is not available",
        response::DEVICE_BUSY => "the device is busy",
        response::INVALID_PARENT_OBJECT => "no such folder",
        response::INVALID_PARAMETER => "invalid parameter",
        response::SESSION_ALREADY_OPEN => "session already open",
        response::TRANSACTION_CANCELLED => "cancelled",
        _ => "unexpected response",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_follow_the_extension() {
        assert_eq!(format_for_name("01 Song.FLAC"), format::FLAC);
        assert_eq!(format_for_name("Mix.m3u8"), format::M3U_PLAYLIST);
        assert_eq!(format_for_name("no extension"), format::UNDEFINED);
    }
}
