//! MTP over USB, through `nusb` (pure Rust: usbfs on Linux, IOKit on macOS). Windows reaches
//! MTP devices only through Windows Portable Devices (see docs/mtp.md), so there [`ports`] finds
//! nothing and [`open`] says so.
//!
//! Finding devices reads only what the system already knows of them (descriptors), so it is
//! cheap and never disturbs a device. Opening one claims its MTP interface, which fails as
//! *busy* while another program (gvfs, KDE, Image Capture) holds it.

/// A USB device with an MTP interface.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UsbPort {
    /// Where it is plugged in (bus and port path): stable while it stays connected.
    pub key: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// The product name the device gives over USB, e.g. "NW-A306".
    pub name: String,
    pub serial: Option<String>,
    interface: u8,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod imp {
    use super::UsbPort;
    use crate::transport::Transport;
    use nusb::MaybeFuture;
    use nusb::descriptors::TransferType;
    use nusb::io::{EndpointRead, EndpointWrite};
    use nusb::transfer::{Bulk, Direction, In, Out};
    use std::io::{Read, Write};
    use std::time::Duration;

    /// How long one USB transfer may take before the device is taken to have stopped answering.
    const TIMEOUT: Duration = Duration::from_secs(30);
    /// The size of each USB transfer, and how many are in flight, when streaming a file.
    const TRANSFER_SIZE: usize = 256 * 1024;
    const TRANSFERS_IN_FLIGHT: usize = 4;

    /// The interface class of PTP ("still image") devices, which many MTP devices use.
    const STILL_IMAGE: (u8, u8, u8) = (0x06, 0x01, 0x01);
    /// Android and others use a vendor-specific interface called "MTP".
    const VENDOR_SPECIFIC: u8 = 0xFF;

    fn io_error(error: nusb::Error) -> std::io::Error {
        let kind = match error.kind() {
            nusb::ErrorKind::Busy => std::io::ErrorKind::ResourceBusy,
            nusb::ErrorKind::PermissionDenied => std::io::ErrorKind::PermissionDenied,
            nusb::ErrorKind::Disconnected | nusb::ErrorKind::NotFound => std::io::ErrorKind::NotConnected,
            _ => std::io::ErrorKind::Other,
        };
        std::io::Error::new(kind, error)
    }

    pub fn ports() -> Vec<UsbPort> {
        let devices = match nusb::list_devices().wait() {
            Ok(devices) => devices,
            Err(e) => {
                log::warn!("could not list USB devices: {e}");
                return Vec::new();
            }
        };
        devices
            .filter_map(|device| {
                let interface = device.interfaces().find(|i| {
                    (i.class(), i.subclass(), i.protocol()) == STILL_IMAGE
                        || (i.class() == VENDOR_SPECIFIC && i.interface_string() == Some("MTP"))
                })?;
                let ports: Vec<String> = device.port_chain().iter().map(u8::to_string).collect();
                Some(UsbPort {
                    key: format!("usb-{}-{}", device.bus_id(), ports.join(".")),
                    vendor_id: device.vendor_id(),
                    product_id: device.product_id(),
                    name: device.product_string().unwrap_or("MTP device").trim().to_owned(),
                    serial: device.serial_number().map(str::to_owned),
                    interface: interface.interface_number(),
                })
            })
            .collect()
    }

    pub struct UsbTransport {
        writer: EndpointWrite<Bulk>,
        reader: EndpointRead<Bulk>,
    }

    pub fn open(port: &UsbPort) -> std::io::Result<UsbTransport> {
        let info = nusb::list_devices()
            .wait()
            .map_err(io_error)?
            .find(|d| {
                let ports: Vec<String> = d.port_chain().iter().map(u8::to_string).collect();
                format!("usb-{}-{}", d.bus_id(), ports.join(".")) == port.key && d.vendor_id() == port.vendor_id
            })
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotConnected, "the player is no longer connected")
            })?;
        let device = info.open().wait().map_err(io_error)?;
        let interface = device.claim_interface(port.interface).wait().map_err(io_error)?;
        let endpoints: Vec<(u8, Direction)> = interface
            .descriptor()
            .map(|d| {
                d.endpoints()
                    .filter(|e| e.transfer_type() == TransferType::Bulk)
                    .map(|e| (e.address(), e.direction()))
                    .collect()
            })
            .unwrap_or_default();
        let find = |direction| endpoints.iter().find(|(_, d)| *d == direction).map(|(a, _)| *a);
        let missing = || std::io::Error::other("the player's MTP interface has no bulk endpoints");
        let (in_address, out_address) =
            (find(Direction::In).ok_or_else(missing)?, find(Direction::Out).ok_or_else(missing)?);
        let reader = interface
            .endpoint::<Bulk, In>(in_address)
            .map_err(io_error)?
            .reader(TRANSFER_SIZE)
            .with_num_transfers(TRANSFERS_IN_FLIGHT)
            .with_read_timeout(TIMEOUT);
        let writer = interface
            .endpoint::<Bulk, Out>(out_address)
            .map_err(io_error)?
            .writer(TRANSFER_SIZE)
            .with_num_transfers(TRANSFERS_IN_FLIGHT)
            .with_write_timeout(TIMEOUT);
        Ok(UsbTransport { writer, reader })
    }

    impl Transport for UsbTransport {
        fn send(&mut self, head: &[u8], body: &mut dyn Read) -> std::io::Result<()> {
            self.writer.write_all(head)?;
            std::io::copy(body, &mut self.writer)?;
            self.writer.flush_end()
        }

        fn receive(&mut self, limit: usize) -> std::io::Result<Vec<u8>> {
            let mut message = self.reader.until_short_packet();
            let mut bytes = Vec::new();
            (&mut message).take(limit as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > limit {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the player sent more than expected"));
            }
            message.consume_end().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            Ok(bytes)
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    use super::UsbPort;
    use crate::transport::Transport;
    use std::io::Read;

    pub fn ports() -> Vec<UsbPort> {
        Vec::new()
    }

    /// Never constructed: there is no USB route to MTP devices on this system.
    pub struct UsbTransport(());

    pub fn open(_port: &UsbPort) -> std::io::Result<UsbTransport> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "MTP players are not supported on this system yet"))
    }

    impl Transport for UsbTransport {
        fn send(&mut self, _head: &[u8], _body: &mut dyn Read) -> std::io::Result<()> {
            Err(std::io::ErrorKind::Unsupported.into())
        }

        fn receive(&mut self, _limit: usize) -> std::io::Result<Vec<u8>> {
            Err(std::io::ErrorKind::Unsupported.into())
        }
    }
}

pub use imp::UsbTransport;

/// The connected USB devices that have an MTP interface (cameras with a PTP interface too:
/// opening one tells them apart, see [`crate::DeviceInfo::is_mtp`]).
pub fn ports() -> Vec<UsbPort> {
    imp::ports()
}

/// Opens and claims the MTP interface of the device at `port`. Fails with
/// `ErrorKind::ResourceBusy` when another program holds it, `PermissionDenied` when the system
/// does not let the listener open it, `NotConnected` when it is gone.
pub fn open(port: &UsbPort) -> std::io::Result<UsbTransport> {
    imp::open(port)
}

#[cfg(test)]
mod tests {
    #[test]
    fn listing_usb_devices_does_not_fail() {
        let _ = super::ports();
    }
}
