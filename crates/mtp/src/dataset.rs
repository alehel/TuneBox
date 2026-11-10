//! Datasets: the structures carried in data phases (PTP section 5, MTP 1.1 section 5). All
//! integers are little-endian. A string is a count of UTF-16 code units including the
//! terminating null (one byte, so at most 255), then the units; an empty string is a single
//! zero byte. An array is a u32 count, then the elements.
//!
//! Everything read from a device is checked: a dataset that ends early is an error, never a
//! panic, and array counts cannot make TuneBox reserve more memory than the bytes received.

use crate::error::MtpError;

/// The longest string a dataset can hold, in UTF-16 code units, without its null.
pub const MAX_STRING_UNITS: usize = 254;

/// Reads a dataset field by field.
pub struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], MtpError> {
        let end = self.position.checked_add(count).filter(|end| *end <= self.bytes.len());
        let slice = end.and_then(|end| self.bytes.get(self.position..end));
        let slice = slice.ok_or_else(|| MtpError::Protocol("a dataset ended early".into()))?;
        self.position += count;
        Ok(slice)
    }

    pub fn u8(&mut self) -> Result<u8, MtpError> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, MtpError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32, MtpError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64(&mut self) -> Result<u64, MtpError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    pub fn string(&mut self) -> Result<String, MtpError> {
        let count = usize::from(self.u8()?);
        let units: Vec<u16> = self.take(count * 2)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let end = units.iter().position(|u| *u == 0).unwrap_or(units.len());
        Ok(String::from_utf16_lossy(units.get(..end).unwrap_or_default()))
    }

    fn count(&mut self, element_size: usize) -> Result<usize, MtpError> {
        let count = usize::try_from(self.u32()?).unwrap_or(usize::MAX);
        // Refuse counts the remaining bytes cannot hold, before allocating anything.
        if count.saturating_mul(element_size) > self.bytes.len().saturating_sub(self.position) {
            return Err(MtpError::Protocol("a dataset array is longer than the data".into()));
        }
        Ok(count)
    }

    pub fn u16_array(&mut self) -> Result<Vec<u16>, MtpError> {
        let count = self.count(2)?;
        (0..count).map(|_| self.u16()).collect()
    }

    pub fn u32_array(&mut self) -> Result<Vec<u32>, MtpError> {
        let count = self.count(4)?;
        (0..count).map(|_| self.u32()).collect()
    }
}

/// Builds a dataset field by field.
#[derive(Default)]
pub struct Writer {
    pub bytes: Vec<u8>,
}

impl Writer {
    pub fn u8(&mut self, value: u8) -> &mut Self {
        self.bytes.push(value);
        self
    }

    pub fn u16(&mut self, value: u16) -> &mut Self {
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn u64(&mut self, value: u64) -> &mut Self {
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Writes `text`; fails if it is longer than a dataset string can hold.
    pub fn string(&mut self, text: &str) -> Result<&mut Self, MtpError> {
        let units: Vec<u16> = text.encode_utf16().collect();
        if units.is_empty() {
            return Ok(self.u8(0));
        }
        if units.len() > MAX_STRING_UNITS {
            return Err(MtpError::NameTooLong(text.to_owned()));
        }
        self.u8((units.len() + 1) as u8);
        units.iter().chain(&[0]).for_each(|u| self.bytes.extend_from_slice(&u.to_le_bytes()));
        Ok(self)
    }

    pub fn u16_array(&mut self, values: &[u16]) -> &mut Self {
        self.u32(values.len() as u32);
        values.iter().for_each(|v| self.bytes.extend_from_slice(&v.to_le_bytes()));
        self
    }

    pub fn u32_array(&mut self, values: &[u32]) -> &mut Self {
        self.u32(values.len() as u32);
        values.iter().for_each(|v| self.bytes.extend_from_slice(&v.to_le_bytes()));
        self
    }
}

/// What a device says about itself (`GetDeviceInfo`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    pub standard_version: u16,
    /// 6 for MTP devices.
    pub vendor_extension_id: u32,
    pub vendor_extension_version: u16,
    pub vendor_extension_desc: String,
    pub functional_mode: u16,
    pub operations: Vec<u16>,
    pub events: Vec<u16>,
    pub device_properties: Vec<u16>,
    pub capture_formats: Vec<u16>,
    pub playback_formats: Vec<u16>,
    pub manufacturer: String,
    pub model: String,
    pub device_version: String,
    pub serial_number: String,
}

impl DeviceInfo {
    pub fn parse(bytes: &[u8]) -> Result<Self, MtpError> {
        let mut r = Reader::new(bytes);
        Ok(Self {
            standard_version: r.u16()?,
            vendor_extension_id: r.u32()?,
            vendor_extension_version: r.u16()?,
            vendor_extension_desc: r.string()?,
            functional_mode: r.u16()?,
            operations: r.u16_array()?,
            events: r.u16_array()?,
            device_properties: r.u16_array()?,
            capture_formats: r.u16_array()?,
            playback_formats: r.u16_array()?,
            manufacturer: r.string()?,
            model: r.string()?,
            device_version: r.string()?,
            serial_number: r.string()?,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, MtpError> {
        let mut w = Writer::default();
        w.u16(self.standard_version).u32(self.vendor_extension_id).u16(self.vendor_extension_version);
        w.string(&self.vendor_extension_desc)?;
        w.u16(self.functional_mode)
            .u16_array(&self.operations)
            .u16_array(&self.events)
            .u16_array(&self.device_properties)
            .u16_array(&self.capture_formats)
            .u16_array(&self.playback_formats);
        w.string(&self.manufacturer)?.string(&self.model)?.string(&self.device_version)?.string(&self.serial_number)?;
        Ok(w.bytes)
    }

    /// Whether the device speaks MTP (and not only PTP, as cameras do).
    pub fn is_mtp(&self) -> bool {
        self.vendor_extension_id == crate::codes::MTP_VENDOR_EXTENSION
            || self.vendor_extension_desc.to_ascii_lowercase().contains("microsoft.com")
    }

    /// Whether the device supports `operation`.
    pub fn supports(&self, operation: u16) -> bool {
        self.operations.contains(&operation)
    }

    /// "Manufacturer Model", for showing.
    pub fn display_name(&self) -> String {
        let model = self.model.trim();
        let manufacturer = self.manufacturer.trim();
        if model.is_empty() {
            manufacturer.to_owned()
        } else if manufacturer.is_empty() || model.to_lowercase().starts_with(&manufacturer.to_lowercase()) {
            model.to_owned()
        } else {
            format!("{manufacturer} {model}")
        }
    }
}

/// Access capability values of a storage.
pub mod access {
    pub const READ_WRITE: u16 = 0;
}

/// One storage of a device (`GetStorageInfo`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StorageInfo {
    pub storage_type: u16,
    pub filesystem_type: u16,
    pub access_capability: u16,
    pub max_capacity: u64,
    pub free_space: u64,
    pub free_objects: u32,
    /// E.g. "Internal shared storage", "SD card".
    pub description: String,
    pub volume_label: String,
}

impl StorageInfo {
    pub fn parse(bytes: &[u8]) -> Result<Self, MtpError> {
        let mut r = Reader::new(bytes);
        Ok(Self {
            storage_type: r.u16()?,
            filesystem_type: r.u16()?,
            access_capability: r.u16()?,
            max_capacity: r.u64()?,
            free_space: r.u64()?,
            free_objects: r.u32()?,
            description: r.string()?,
            volume_label: r.string()?,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, MtpError> {
        let mut w = Writer::default();
        w.u16(self.storage_type).u16(self.filesystem_type).u16(self.access_capability);
        w.u64(self.max_capacity).u64(self.free_space).u32(self.free_objects);
        w.string(&self.description)?.string(&self.volume_label)?;
        Ok(w.bytes)
    }

    pub fn writable(&self) -> bool {
        self.access_capability == access::READ_WRITE
    }
}

/// A file or folder on a device (`GetObjectInfo`, `SendObjectInfo`). The thumbnail and image
/// fields of the dataset are read past and written as zeros: TuneBox sends no pictures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectInfo {
    pub storage_id: u32,
    pub format: u16,
    pub protection_status: u16,
    /// The size, or `0xFFFFFFFF` for 4 GiB or more.
    pub compressed_size: u32,
    pub parent: u32,
    pub association_type: u16,
    pub association_desc: u32,
    pub sequence_number: u32,
    pub filename: String,
    pub capture_date: String,
    pub modification_date: String,
    pub keywords: String,
}

impl ObjectInfo {
    pub fn parse(bytes: &[u8]) -> Result<Self, MtpError> {
        let mut r = Reader::new(bytes);
        let storage_id = r.u32()?;
        let format = r.u16()?;
        let protection_status = r.u16()?;
        let compressed_size = r.u32()?;
        // Thumb format (u16), thumb size, width, height, image width, height, bit depth (u32).
        r.u16()?;
        for _ in 0..6 {
            r.u32()?;
        }
        Ok(Self {
            storage_id,
            format,
            protection_status,
            compressed_size,
            parent: r.u32()?,
            association_type: r.u16()?,
            association_desc: r.u32()?,
            sequence_number: r.u32()?,
            filename: r.string()?,
            capture_date: r.string()?,
            modification_date: r.string()?,
            keywords: r.string()?,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, MtpError> {
        let mut w = Writer::default();
        w.u32(self.storage_id).u16(self.format).u16(self.protection_status).u32(self.compressed_size);
        w.u16(0);
        for _ in 0..6 {
            w.u32(0);
        }
        w.u32(self.parent).u16(self.association_type).u32(self.association_desc).u32(self.sequence_number);
        w.string(&self.filename)?
            .string(&self.capture_date)?
            .string(&self.modification_date)?
            .string(&self.keywords)?;
        Ok(w.bytes)
    }

    pub fn is_folder(&self) -> bool {
        self.format == crate::codes::format::ASSOCIATION
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::format;

    #[test]
    fn strings_byte_by_byte() {
        let mut w = Writer::default();
        w.string("Ab").unwrap().string("").unwrap();
        assert_eq!(w.bytes, [3, b'A', 0, b'b', 0, 0, 0, 0]);
        let mut r = Reader::new(&w.bytes);
        assert_eq!((r.string().unwrap(), r.string().unwrap()), ("Ab".into(), String::new()));
        assert!(Writer::default().string(&"x".repeat(255)).is_err(), "too long for a dataset");
        assert!(Writer::default().string(&"x".repeat(254)).is_ok());
    }

    #[test]
    fn an_object_info_byte_by_byte() {
        let info = ObjectInfo {
            storage_id: 0x0001_0001,
            format: format::FLAC,
            compressed_size: 1000,
            parent: 7,
            filename: "a.flac".into(),
            ..ObjectInfo::default()
        };
        let bytes = info.encode().unwrap();
        // Storage, format, protection, size; 26 bytes of thumbnail and image fields; parent.
        assert_eq!(bytes[0..12], [0x01, 0, 0x01, 0, 0x06, 0xB9, 0, 0, 0xE8, 0x03, 0, 0]);
        assert_eq!(bytes[12..38], [0; 26]);
        assert_eq!(bytes[38..42], [7, 0, 0, 0]);
        assert_eq!(bytes[52], 7, "six characters and a null");
        assert_eq!(ObjectInfo::parse(&bytes).unwrap(), info);
    }

    #[test]
    fn device_and_storage_info_round_trip() {
        let device = DeviceInfo {
            standard_version: 100,
            vendor_extension_id: 6,
            vendor_extension_desc: "microsoft.com: 1.0;".into(),
            operations: vec![0x1001, 0x1002],
            manufacturer: "Sony".into(),
            model: "WALKMAN NW-A306".into(),
            serial_number: "123".into(),
            ..DeviceInfo::default()
        };
        assert_eq!(DeviceInfo::parse(&device.encode().unwrap()).unwrap(), device);
        assert!(device.is_mtp() && device.supports(0x1002));
        assert_eq!(device.display_name(), "Sony WALKMAN NW-A306");
        let storage = StorageInfo {
            max_capacity: 64_000_000_000,
            free_space: 1_000,
            description: "SD card".into(),
            ..StorageInfo::default()
        };
        assert_eq!(StorageInfo::parse(&storage.encode().unwrap()).unwrap(), storage);
    }

    #[test]
    fn hostile_datasets_are_errors() {
        assert!(DeviceInfo::parse(&[]).is_err());
        // An array claiming four billion elements in eight bytes.
        assert!(Reader::new(&[0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0]).u32_array().is_err());
        // A string longer than the data.
        assert!(Reader::new(&[200, 0x41, 0]).string().is_err());
    }
}
