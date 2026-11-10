//! A session with a device: the operations TuneBox needs, one at a time, blocking.
//!
//! Each operation is a command container, an optional data phase and a response (MTP 1.1
//! section D.7). A failure of the connection itself (unplugged, timed out, a malformed reply)
//! leaves the two ends out of step, so the session marks itself broken and every later
//! operation fails at once with the same plain message, instead of talking past the device.

use crate::codes::{GENERIC_FOLDER, container_type, format, format_for_name, operation, response};
use crate::container::{self, Container};
use crate::dataset::{DeviceInfo, ObjectInfo, Reader, StorageInfo};
use crate::error::MtpError;
use crate::transport::Transport;
use std::io::Read;

/// The largest dataset TuneBox accepts (an object-handle list of four million files fits).
const DATASET_LIMIT: usize = 16 * 1024 * 1024;

/// The session id TuneBox opens. Any non-zero value will do; one host talks to the device.
const SESSION_ID: u32 = 1;

pub struct Session<T: Transport> {
    transport: T,
    next_transaction: u32,
    broken: bool,
}

/// Data going to the device in an operation's data phase.
struct Outgoing<'a> {
    len: u64,
    body: &'a mut dyn Read,
}

impl<T: Transport> Session<T> {
    /// Opens a session on the device at the other end of `transport`.
    pub fn open(transport: T) -> Result<Self, MtpError> {
        let mut session = Self { transport, next_transaction: 0, broken: false };
        match session.transact(operation::OPEN_SESSION, &[SESSION_ID], None, None) {
            Ok(_) => {}
            // Left open by an earlier TuneBox that was interrupted: carry on with it.
            Err(MtpError::Refused { code: response::SESSION_ALREADY_OPEN, .. }) => {}
            Err(e) => return Err(e),
        }
        Ok(session)
    }

    pub fn device_info(&mut self) -> Result<DeviceInfo, MtpError> {
        let data = self.receive_data(operation::GET_DEVICE_INFO, &[], DATASET_LIMIT)?;
        DeviceInfo::parse(&data)
    }

    pub fn storage_ids(&mut self) -> Result<Vec<u32>, MtpError> {
        let data = self.receive_data(operation::GET_STORAGE_IDS, &[], DATASET_LIMIT)?;
        Reader::new(&data).u32_array()
    }

    pub fn storage_info(&mut self, storage: u32) -> Result<StorageInfo, MtpError> {
        let data = self.receive_data(operation::GET_STORAGE_INFO, &[storage], DATASET_LIMIT)?;
        StorageInfo::parse(&data)
    }

    /// The handles of the objects directly inside `parent` (`ROOT_PARENT` for the top level).
    pub fn object_handles(&mut self, storage: u32, parent: u32) -> Result<Vec<u32>, MtpError> {
        let data = self.receive_data(operation::GET_OBJECT_HANDLES, &[storage, 0, parent], DATASET_LIMIT)?;
        Reader::new(&data).u32_array()
    }

    pub fn object_info(&mut self, handle: u32) -> Result<ObjectInfo, MtpError> {
        let data = self.receive_data(operation::GET_OBJECT_INFO, &[handle], DATASET_LIMIT)?;
        ObjectInfo::parse(&data)
    }

    /// The bytes of a file, refusing one longer than `limit`.
    pub fn get_object(&mut self, handle: u32, limit: usize) -> Result<Vec<u8>, MtpError> {
        self.receive_data(operation::GET_OBJECT, &[handle], limit)
    }

    pub fn delete_object(&mut self, handle: u32) -> Result<(), MtpError> {
        self.transact(operation::DELETE_OBJECT, &[handle, 0], None, None).map(|_| ())
    }

    /// Makes a folder called `name` in `parent`, returning its handle.
    pub fn create_folder(&mut self, storage: u32, parent: u32, name: &str) -> Result<u32, MtpError> {
        let info = ObjectInfo {
            storage_id: storage,
            format: format::ASSOCIATION,
            association_type: GENERIC_FOLDER,
            parent,
            filename: name.to_owned(),
            ..ObjectInfo::default()
        };
        self.send_object_info(storage, parent, &info)
    }

    /// Sends a file of `len` bytes from `body` as `name` in `parent`, returning its handle.
    ///
    /// The file exists on the device, with its full declared size, from the moment its
    /// information is accepted; only an OK to the transfer itself means the bytes arrived. So
    /// if the transfer fails while the device still answers, the object is deleted before the
    /// error is returned. If the device is gone, the caller must remember that this name may
    /// hold a partial file (TuneBox's sync keeps a journal of them).
    pub fn send_file(
        &mut self,
        storage: u32,
        parent: u32,
        name: &str,
        len: u64,
        body: &mut dyn Read,
    ) -> Result<u32, MtpError> {
        let info = ObjectInfo {
            storage_id: storage,
            format: format_for_name(name),
            compressed_size: u32::try_from(len).unwrap_or(u32::MAX),
            parent,
            filename: name.to_owned(),
            ..ObjectInfo::default()
        };
        let handle = self.send_object_info(storage, parent, &info)?;
        let mut exact = Exact { inner: body, remaining: len };
        match self.transact(operation::SEND_OBJECT, &[], Some(Outgoing { len, body: &mut exact }), None) {
            Ok(_) => Ok(handle),
            Err(error) => {
                if !self.broken
                    && let Err(e) = self.delete_object(handle)
                {
                    log::warn!("could not remove the incomplete “{name}” from the player: {e}");
                }
                Err(error)
            }
        }
    }

    fn send_object_info(&mut self, storage: u32, parent: u32, info: &ObjectInfo) -> Result<u32, MtpError> {
        let dataset = info.encode()?;
        let outgoing = Outgoing { len: dataset.len() as u64, body: &mut dataset.as_slice() };
        let response = self.transact(operation::SEND_OBJECT_INFO, &[storage, parent], Some(outgoing), None)?;
        // Results: the storage, the parent and the new object's handle.
        response.params().get(2).copied().ok_or_else(|| MtpError::Protocol("no handle for the new file".into()))
    }

    fn receive_data(&mut self, code: u16, params: &[u32], limit: usize) -> Result<Vec<u8>, MtpError> {
        let mut data = None;
        self.transact(code, params, None, Some((&mut data, limit)))?;
        data.ok_or_else(|| MtpError::Protocol("the player sent no data".into()))
    }

    /// Performs one operation, marking the session broken if the connection fails.
    fn transact(
        &mut self,
        code: u16,
        params: &[u32],
        outgoing: Option<Outgoing>,
        incoming: Option<(&mut Option<Vec<u8>>, usize)>,
    ) -> Result<Container, MtpError> {
        if self.broken {
            return Err(MtpError::Io(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "the connection to the player was lost",
            )));
        }
        let transaction = self.next_transaction;
        self.next_transaction = self.next_transaction.wrapping_add(1).max(1);
        let result = self.exchange(code, transaction, params, outgoing, incoming);
        if matches!(result, Err(MtpError::Io(_) | MtpError::Protocol(_))) {
            self.broken = true;
        }
        result
    }

    fn exchange(
        &mut self,
        code: u16,
        transaction: u32,
        params: &[u32],
        outgoing: Option<Outgoing>,
        incoming: Option<(&mut Option<Vec<u8>>, usize)>,
    ) -> Result<Container, MtpError> {
        self.transport.send(&container::command(code, transaction, params), &mut std::io::empty())?;
        if let Some(Outgoing { len, body }) = outgoing {
            let head = container::header(container_type::DATA, code, transaction, len);
            self.transport.send(&head, body)?;
        }
        let limit = incoming.as_ref().map_or(DATASET_LIMIT, |(_, limit)| limit.saturating_add(container::HEADER_LEN));
        let mut reply = container::parse(&self.transport.receive(limit)?)?;
        if let Some((data, _)) = incoming
            && reply.kind == container_type::DATA
        {
            check_transaction(&reply, transaction)?;
            *data = Some(std::mem::take(&mut reply.payload));
            reply = container::parse(&self.transport.receive(DATASET_LIMIT)?)?;
        }
        if reply.kind != container_type::RESPONSE {
            return Err(MtpError::Protocol(format!("expected a response, got a container of type {}", reply.kind)));
        }
        check_transaction(&reply, transaction)?;
        if reply.code != response::OK {
            return Err(MtpError::Refused { operation: code, code: reply.code });
        }
        Ok(reply)
    }
}

/// Ends the session when it is dropped, best effort (the device may already be gone), so the
/// device is left as TuneBox found it for the next program.
impl<T: Transport> Drop for Session<T> {
    fn drop(&mut self) {
        if !self.broken
            && let Err(e) = self.transact(operation::CLOSE_SESSION, &[], None, None)
        {
            log::debug!("closing the MTP session: {e}");
        }
    }
}

fn check_transaction(reply: &Container, transaction: u32) -> Result<(), MtpError> {
    if reply.transaction == transaction {
        Ok(())
    } else {
        Err(MtpError::Protocol(format!("a reply to transaction {} arrived during {transaction}", reply.transaction)))
    }
}

/// Yields exactly `remaining` bytes, failing if the source runs short (a file that shrank
/// while being sent), so the device is never told one size and sent another silently.
struct Exact<'a> {
    inner: &'a mut dyn Read,
    remaining: u64,
}

impl Read for Exact<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let max = usize::try_from(self.remaining).unwrap_or(usize::MAX).min(buf.len());
        let read = self.inner.read(buf.get_mut(..max).unwrap_or_default())?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "the file got shorter while being sent",
            ));
        }
        self.remaining -= read as u64;
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::{ROOT_PARENT, operation};
    use crate::simulator::{FIRST_STORAGE, SimulatedDevice};
    use std::collections::VecDeque;

    #[test]
    fn folders_and_files_round_trip() {
        let device = SimulatedDevice::new("Player", 1_000_000);
        let mut session = Session::open(device.connect()).unwrap();
        assert!(session.device_info().unwrap().is_mtp());
        assert_eq!(session.storage_ids().unwrap(), [FIRST_STORAGE]);
        assert_eq!(session.storage_info(FIRST_STORAGE).unwrap().free_space, 1_000_000);

        let music = session.create_folder(FIRST_STORAGE, ROOT_PARENT, "Music").unwrap();
        let song = b"not really a flac".to_vec();
        let handle =
            session.send_file(FIRST_STORAGE, music, "a.flac", song.len() as u64, &mut song.as_slice()).unwrap();
        assert_eq!(session.object_handles(FIRST_STORAGE, music).unwrap(), [handle]);
        let info = session.object_info(handle).unwrap();
        assert_eq!((info.filename.as_str(), info.parent, info.format), ("a.flac", music, crate::codes::format::FLAC));
        assert_eq!(session.get_object(handle, 1024).unwrap(), song);
        assert!(session.get_object(handle, 4).is_err(), "longer than allowed");
        assert_eq!(device.files(), ["Music/a.flac"]);
        drop(session);

        let mut session = Session::open(device.connect()).unwrap();
        session.delete_object(handle).unwrap();
        assert!(device.files().is_empty());
        assert_eq!(session.storage_info(FIRST_STORAGE).unwrap().free_space, 1_000_000);
        drop(session);
        // The first session was broken by the over-long read, so only this one says goodbye.
        assert_eq!(device.count(operation::CLOSE_SESSION), 1, "a session is closed when dropped");
    }

    #[test]
    fn an_unplug_mid_file_breaks_the_session() {
        let device = SimulatedDevice::new("Player", 1_000_000);
        let mut session = Session::open(device.connect()).unwrap();
        device.unplug_after(10_000);
        let song = vec![7u8; 50_000];
        let error = session.send_file(FIRST_STORAGE, ROOT_PARENT, "a.mp3", 50_000, &mut song.as_slice()).unwrap_err();
        assert!(matches!(error, MtpError::Io(_)), "{error}");
        // The worst case: the device kept a partial file that reports its full size.
        assert_eq!(device.file("a.mp3").map(|d| d.len()), Some(10_000));
        assert_eq!(device.reported_size("a.mp3"), Some(50_000));
        assert!(session.storage_ids().is_err(), "a broken session fails at once");
    }

    #[test]
    fn a_full_storage_leaves_nothing_behind() {
        let device = SimulatedDevice::new("Player", 100);
        let mut session = Session::open(device.connect()).unwrap();
        let song = vec![1u8; 500];
        let error = session.send_file(FIRST_STORAGE, ROOT_PARENT, "a.mp3", 500, &mut song.as_slice()).unwrap_err();
        assert!(error.is_full(), "{error}");
        assert!(device.files().is_empty());
        assert!(session.storage_ids().is_ok(), "a refusal leaves the session usable");
    }

    #[test]
    fn a_file_that_shrinks_while_sent_is_not_passed_off_as_complete() {
        let device = SimulatedDevice::new("Player", 1_000_000);
        let mut session = Session::open(device.connect()).unwrap();
        let short = vec![1u8; 10];
        assert!(session.send_file(FIRST_STORAGE, ROOT_PARENT, "a.mp3", 20, &mut short.as_slice()).is_err());
        assert!(session.storage_ids().is_err());
    }

    /// Replies with hand-built packets, recording what was sent.
    struct Scripted {
        sent: Vec<Vec<u8>>,
        replies: VecDeque<Vec<u8>>,
    }

    impl Transport for Scripted {
        fn send(&mut self, head: &[u8], body: &mut dyn Read) -> std::io::Result<()> {
            let mut bytes = head.to_vec();
            body.read_to_end(&mut bytes)?;
            self.sent.push(bytes);
            Ok(())
        }

        fn receive(&mut self, _limit: usize) -> std::io::Result<Vec<u8>> {
            self.replies.pop_front().ok_or_else(|| std::io::ErrorKind::TimedOut.into())
        }
    }

    #[test]
    fn hand_built_storage_ids() {
        let ok = |transaction: u8| vec![12, 0, 0, 0, 3, 0, 0x01, 0x20, transaction, 0, 0, 0];
        let ids = vec![
            24, 0, 0, 0, 2, 0, 0x04, 0x10, 1, 0, 0, 0, // data, GetStorageIDs, transaction 1
            2, 0, 0, 0, 0x01, 0, 0x01, 0, 0x01, 0, 0x02, 0, // two ids
        ];
        let transport = Scripted { sent: Vec::new(), replies: VecDeque::from([ok(0), ids, ok(1)]) };
        let mut session = Session::open(transport).unwrap();
        assert_eq!(session.storage_ids().unwrap(), [0x0001_0001, 0x0002_0001]);
        assert_eq!(session.transport.sent[1], [12, 0, 0, 0, 1, 0, 0x04, 0x10, 1, 0, 0, 0]);
    }

    #[test]
    fn a_reply_to_the_wrong_transaction_is_refused() {
        let wrong = vec![12, 0, 0, 0, 3, 0, 0x01, 0x20, 9, 0, 0, 0];
        let transport = Scripted { sent: Vec::new(), replies: VecDeque::from([wrong]) };
        assert!(matches!(Session::open(transport), Err(MtpError::Protocol(_))));
    }
}
