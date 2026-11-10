//! A simulated MTP device for tests: storages, folders and files in memory, answering the
//! operations TuneBox uses as the specification describes, over an in-memory [`Transport`].
//!
//! It can be told to misbehave the ways real players do: be unplugged part way through a file
//! (keeping the partial object, the worst case), run out of space, or be unplugged outright.
//! Tests keep a [`SimulatedDevice`] (a shared handle) to set it up and look inside afterwards,
//! and call [`SimulatedDevice::connect`] for each connection, as plugging it in again.

use crate::codes::{ROOT_PARENT, container_type, format, operation, response};
use crate::container::{self, Container};
use crate::dataset::{DeviceInfo, ObjectInfo, StorageInfo, Writer};
use crate::transport::Transport;
use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// The first storage's id, as Android numbers it.
pub const FIRST_STORAGE: u32 = 0x0001_0001;

#[derive(Clone, Debug)]
struct Object {
    storage: u32,
    parent: u32,
    name: String,
    folder: bool,
    /// The size given in `SendObjectInfo`; what the device reports, whatever arrived.
    declared: u64,
    data: Vec<u8>,
}

#[derive(Debug)]
struct Storage {
    id: u32,
    description: String,
    capacity: u64,
}

#[derive(Debug)]
struct State {
    info: DeviceInfo,
    storages: Vec<Storage>,
    objects: BTreeMap<u32, Object>,
    next_handle: u32,
    session_open: bool,
    /// The object whose bytes the next `SendObject` carries.
    awaiting_data: Option<u32>,
    connected: bool,
    /// Unplug once this many more bytes of file data have arrived.
    unplug_after: Option<u64>,
    /// Another program holds the device: opening it fails as busy.
    held: bool,
    /// Like a locked Android phone: no storage is shown.
    locked: bool,
    /// Operations performed, for tests to count.
    log: Vec<u16>,
}

impl State {
    fn used(&self, storage: u32) -> u64 {
        self.objects.values().filter(|o| o.storage == storage).map(|o| o.declared).sum()
    }

    fn storage(&self, id: u32) -> Option<&Storage> {
        self.storages.iter().find(|s| s.id == id)
    }

    fn children(&self, storage: u32, parent: u32) -> Vec<u32> {
        let all_levels = parent == 0;
        self.objects
            .iter()
            .filter(|(_, o)| (storage == ROOT_PARENT || o.storage == storage) && (all_levels || o.parent == parent))
            .map(|(h, _)| *h)
            .collect()
    }

    fn delete(&mut self, handle: u32) {
        for child in self.children(ROOT_PARENT, handle) {
            self.delete(child);
        }
        self.objects.remove(&handle);
    }

    fn find(&self, path: &str) -> Option<u32> {
        let mut parent = ROOT_PARENT;
        let mut found = None;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            let handle = self.objects.iter().find(|(_, o)| o.parent == parent && o.name == part).map(|(h, _)| *h)?;
            found = Some(handle);
            parent = handle;
        }
        found
    }

    fn path(&self, mut handle: u32) -> String {
        let mut parts = Vec::new();
        while let Some(object) = self.objects.get(&handle) {
            parts.push(object.name.clone());
            handle = object.parent;
        }
        parts.reverse();
        parts.join("/")
    }
}

/// A shared handle on a simulated device.
#[derive(Clone, Debug)]
pub struct SimulatedDevice {
    state: Arc<Mutex<State>>,
}

/// Handles are equal when they are the same device.
impl PartialEq for SimulatedDevice {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }
}

impl Eq for SimulatedDevice {}

impl SimulatedDevice {
    /// A player called `model` with one storage of `capacity` bytes.
    pub fn new(model: &str, capacity: u64) -> Self {
        let info = DeviceInfo {
            standard_version: 100,
            vendor_extension_id: crate::codes::MTP_VENDOR_EXTENSION,
            vendor_extension_version: 100,
            vendor_extension_desc: "microsoft.com: 1.0;".into(),
            operations: vec![
                operation::GET_DEVICE_INFO,
                operation::OPEN_SESSION,
                operation::CLOSE_SESSION,
                operation::GET_STORAGE_IDS,
                operation::GET_STORAGE_INFO,
                operation::GET_OBJECT_HANDLES,
                operation::GET_OBJECT_INFO,
                operation::GET_OBJECT,
                operation::DELETE_OBJECT,
                operation::SEND_OBJECT_INFO,
                operation::SEND_OBJECT,
            ],
            manufacturer: "Simulated".into(),
            model: model.into(),
            serial_number: "SIM0001".into(),
            ..DeviceInfo::default()
        };
        let state = State {
            info,
            storages: vec![Storage { id: FIRST_STORAGE, description: "Internal storage".into(), capacity }],
            objects: BTreeMap::new(),
            next_handle: 1,
            session_open: false,
            awaiting_data: None,
            connected: true,
            unplug_after: None,
            held: false,
            locked: false,
            log: Vec::new(),
        };
        Self { state: Arc::new(Mutex::new(state)) }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds another storage (an SD card), returning its id.
    pub fn add_storage(&self, description: &str, capacity: u64) -> u32 {
        let mut state = self.lock();
        let id = FIRST_STORAGE + state.storages.len() as u32 * 0x0001_0000;
        state.storages.push(Storage { id, description: description.into(), capacity });
        id
    }

    /// A new connection, as plugging the device in (again).
    pub fn connect(&self) -> SimulatedTransport {
        let mut state = self.lock();
        state.connected = true;
        state.session_open = false;
        state.awaiting_data = None;
        SimulatedTransport { state: Arc::clone(&self.state), replies: VecDeque::new(), command: None }
    }

    /// Unplugs the device once `bytes` more bytes of file data have arrived.
    pub fn unplug_after(&self, bytes: u64) {
        self.lock().unplug_after = Some(bytes);
    }

    /// Makes the device behave as if another program held it (or not).
    pub fn hold(&self, held: bool) {
        self.lock().held = held;
    }

    /// Whether another program holds it: a transport for it must refuse to open, as busy.
    pub fn is_held(&self) -> bool {
        self.lock().held
    }

    /// Hides the storages, as a locked phone does (or shows them again).
    pub fn lock_screen(&self, locked: bool) {
        self.lock().locked = locked;
    }

    /// Unplugs the device now.
    pub fn unplug(&self) {
        self.lock().connected = false;
    }

    /// Puts a file on the first storage as if the listener had (folders are made as needed).
    pub fn put(&self, path: &str, data: &[u8]) {
        let mut state = self.lock();
        let mut parent = ROOT_PARENT;
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        for (index, part) in parts.iter().enumerate() {
            let last = index + 1 == parts.len();
            let existing = state.objects.iter().find(|(_, o)| o.parent == parent && o.name == *part).map(|(h, _)| *h);
            parent = match existing {
                Some(handle) if !last => handle,
                _ => {
                    let handle = state.next_handle;
                    state.next_handle += 1;
                    let data = if last { data.to_vec() } else { Vec::new() };
                    let object = Object {
                        storage: FIRST_STORAGE,
                        parent,
                        name: (*part).into(),
                        folder: !last,
                        declared: data.len() as u64,
                        data,
                    };
                    state.objects.insert(handle, object);
                    handle
                }
            };
        }
    }

    /// The bytes of the file at `path` (on any storage), if there is one.
    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        let state = self.lock();
        let handle = state.find(path)?;
        state.objects.get(&handle).filter(|o| !o.folder).map(|o| o.data.clone())
    }

    /// The size the device reports for the file at `path`, whatever arrived.
    pub fn reported_size(&self, path: &str) -> Option<u64> {
        let state = self.lock();
        state.find(path).and_then(|h| state.objects.get(&h)).map(|o| o.declared)
    }

    /// Every file's path, sorted.
    pub fn files(&self) -> Vec<String> {
        let state = self.lock();
        let mut paths: Vec<String> =
            state.objects.iter().filter(|(_, o)| !o.folder).map(|(h, _)| state.path(*h)).collect();
        paths.sort();
        paths
    }

    /// Every folder's path, sorted.
    pub fn folders(&self) -> Vec<String> {
        let state = self.lock();
        let mut paths: Vec<String> =
            state.objects.iter().filter(|(_, o)| o.folder).map(|(h, _)| state.path(*h)).collect();
        paths.sort();
        paths
    }

    /// How many times `operation` has been performed.
    pub fn count(&self, operation: u16) -> usize {
        self.lock().log.iter().filter(|o| **o == operation).count()
    }
}

/// The host's end of a connection to a [`SimulatedDevice`].
pub struct SimulatedTransport {
    state: Arc<Mutex<State>>,
    replies: VecDeque<Vec<u8>>,
    /// A command waiting for its data phase.
    command: Option<Container>,
}

fn unplugged() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::BrokenPipe, "the player was disconnected")
}

impl SimulatedTransport {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn respond(&mut self, transaction: u32, code: u16, params: &[u32]) {
        self.replies.push_back(container::with_params(container_type::RESPONSE, code, transaction, params));
    }

    fn respond_with_data(&mut self, command: &Container, data: Vec<u8>) {
        let mut bytes =
            container::header(container_type::DATA, command.code, command.transaction, data.len() as u64).to_vec();
        bytes.extend_from_slice(&data);
        self.replies.push_back(bytes);
        self.respond(command.transaction, response::OK, &[]);
    }

    /// Reads a data phase, honouring a pending unplug during a file's bytes: the bytes that
    /// made it are kept.
    fn read_body(&mut self, body: &mut dyn Read, into: &mut Vec<u8>) -> std::io::Result<()> {
        let sending_file = self.command.as_ref().is_some_and(|c| c.code == operation::SEND_OBJECT);
        let mut chunk = [0u8; 4096];
        loop {
            let read = body.read(&mut chunk)?;
            if read == 0 {
                return Ok(());
            }
            let mut state = self.lock();
            let allowed = match state.unplug_after.filter(|_| sending_file) {
                Some(left) if (read as u64) >= left => {
                    state.connected = false;
                    state.unplug_after = None;
                    left as usize
                }
                Some(left) => {
                    state.unplug_after = Some(left - read as u64);
                    read
                }
                None => read,
            };
            into.extend_from_slice(chunk.get(..allowed).unwrap_or_default());
            if !state.connected {
                return Err(unplugged());
            }
        }
    }

    fn handle_command(&mut self, command: Container) {
        let params = command.params();
        let param = |i: usize| params.get(i).copied().unwrap_or(0);
        self.lock().log.push(command.code);
        let session_open = self.lock().session_open;
        if !session_open && !matches!(command.code, operation::OPEN_SESSION | operation::GET_DEVICE_INFO) {
            return self.respond(command.transaction, response::SESSION_NOT_OPEN, &[]);
        }
        match command.code {
            operation::GET_DEVICE_INFO => {
                let data = self.lock().info.encode().unwrap_or_default();
                self.respond_with_data(&command, data);
            }
            operation::OPEN_SESSION => {
                let code = if session_open { response::SESSION_ALREADY_OPEN } else { response::OK };
                self.lock().session_open = true;
                self.respond(command.transaction, code, &[]);
            }
            operation::CLOSE_SESSION => {
                self.lock().session_open = false;
                self.respond(command.transaction, response::OK, &[]);
            }
            operation::GET_STORAGE_IDS => {
                let ids: Vec<u32> = {
                    let state = self.lock();
                    if state.locked { Vec::new() } else { state.storages.iter().map(|s| s.id).collect() }
                };
                let mut w = Writer::default();
                w.u32_array(&ids);
                self.respond_with_data(&command, w.bytes);
            }
            operation::GET_STORAGE_INFO => {
                let info = {
                    let state = self.lock();
                    state.storage(param(0)).map(|s| StorageInfo {
                        storage_type: 3,
                        filesystem_type: 2,
                        max_capacity: s.capacity,
                        free_space: s.capacity.saturating_sub(state.used(s.id)),
                        free_objects: u32::MAX,
                        description: s.description.clone(),
                        ..StorageInfo::default()
                    })
                };
                match info.and_then(|i| i.encode().ok()) {
                    Some(data) => self.respond_with_data(&command, data),
                    None => self.respond(command.transaction, response::INVALID_STORAGE_ID, &[]),
                }
            }
            operation::GET_OBJECT_HANDLES => {
                let handles = self.lock().children(param(0), param(2));
                let mut w = Writer::default();
                w.u32_array(&handles);
                self.respond_with_data(&command, w.bytes);
            }
            operation::GET_OBJECT_INFO => {
                let object = self.lock().objects.get(&param(0)).cloned();
                let info = object.map(|o| ObjectInfo {
                    storage_id: o.storage,
                    format: if o.folder { format::ASSOCIATION } else { crate::codes::format_for_name(&o.name) },
                    compressed_size: u32::try_from(o.declared).unwrap_or(u32::MAX),
                    parent: if o.parent == ROOT_PARENT { 0 } else { o.parent },
                    association_type: if o.folder { crate::codes::GENERIC_FOLDER } else { 0 },
                    filename: o.name,
                    ..ObjectInfo::default()
                });
                match info.and_then(|i| i.encode().ok()) {
                    Some(data) => self.respond_with_data(&command, data),
                    None => self.respond(command.transaction, response::INVALID_OBJECT_HANDLE, &[]),
                }
            }
            operation::GET_OBJECT => {
                let data = self.lock().objects.get(&param(0)).filter(|o| !o.folder).map(|o| o.data.clone());
                match data {
                    Some(data) => self.respond_with_data(&command, data),
                    None => self.respond(command.transaction, response::INVALID_OBJECT_HANDLE, &[]),
                }
            }
            operation::DELETE_OBJECT => {
                let exists = self.lock().objects.contains_key(&param(0));
                if exists {
                    self.lock().delete(param(0));
                    self.respond(command.transaction, response::OK, &[]);
                } else {
                    self.respond(command.transaction, response::INVALID_OBJECT_HANDLE, &[]);
                }
            }
            operation::SEND_OBJECT_INFO | operation::SEND_OBJECT => self.command = Some(command),
            _ => self.respond(command.transaction, response::OPERATION_NOT_SUPPORTED, &[]),
        }
    }

    fn handle_data(&mut self, command: Container, data: Vec<u8>) {
        match command.code {
            operation::SEND_OBJECT_INFO => {
                let params = command.params();
                let (storage, parent) = (params.first().copied().unwrap_or(0), params.get(1).copied().unwrap_or(0));
                let Ok(info) = ObjectInfo::parse(&data) else {
                    return self.respond(command.transaction, response::INVALID_PARAMETER, &[]);
                };
                let mut state = self.lock();
                let Some(capacity) = state.storage(storage).map(|s| s.capacity) else {
                    drop(state);
                    return self.respond(command.transaction, response::INVALID_STORAGE_ID, &[]);
                };
                let parent = if parent == 0 { ROOT_PARENT } else { parent };
                if parent != ROOT_PARENT && !state.objects.get(&parent).is_some_and(|o| o.folder) {
                    drop(state);
                    return self.respond(command.transaction, response::INVALID_PARENT_OBJECT, &[]);
                }
                let declared = u64::from(info.compressed_size);
                if declared > capacity.saturating_sub(state.used(storage)) {
                    drop(state);
                    return self.respond(command.transaction, response::STORE_FULL, &[]);
                }
                let handle = state.next_handle;
                state.next_handle += 1;
                let folder = info.is_folder();
                let object = Object { storage, parent, name: info.filename, folder, declared, data: Vec::new() };
                state.objects.insert(handle, object);
                state.awaiting_data = (!folder).then_some(handle);
                drop(state);
                self.respond(command.transaction, response::OK, &[storage, parent, handle]);
            }
            _ => {
                let mut state = self.lock();
                let Some(handle) = state.awaiting_data.take() else {
                    drop(state);
                    // NoValidObjectInfo.
                    return self.respond(command.transaction, 0x2015, &[]);
                };
                let complete = state.objects.get(&handle).is_some_and(|o| o.declared == data.len() as u64);
                if let Some(object) = state.objects.get_mut(&handle) {
                    object.data = data;
                }
                drop(state);
                let code = if complete { response::OK } else { response::INCOMPLETE_TRANSFER };
                self.respond(command.transaction, code, &[]);
            }
        }
    }
}

impl Transport for SimulatedTransport {
    fn send(&mut self, head: &[u8], body: &mut dyn Read) -> std::io::Result<()> {
        if !self.lock().connected {
            return Err(unplugged());
        }
        let mut bytes = head.to_vec();
        let result = self.read_body(body, &mut bytes);
        let parsed = container::parse(&bytes);
        match (result, parsed) {
            (Err(e), _) => {
                // The device keeps whatever part of the file arrived, as some players do.
                if self.command.as_ref().is_some_and(|c| c.code == operation::SEND_OBJECT) {
                    let mut state = self.lock();
                    if let Some(handle) = state.awaiting_data
                        && let Some(object) = state.objects.get_mut(&handle)
                    {
                        object.data = bytes.get(container::HEADER_LEN..).unwrap_or_default().to_vec();
                    }
                }
                Err(e)
            }
            (Ok(()), Err(e)) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
            (Ok(()), Ok(container)) if container.kind == container_type::COMMAND => {
                self.handle_command(container);
                Ok(())
            }
            (Ok(()), Ok(container)) => {
                match self.command.take() {
                    Some(command) => self.handle_data(command, container.payload),
                    None => self.respond(container.transaction, response::GENERAL_ERROR, &[]),
                }
                Ok(())
            }
        }
    }

    fn receive(&mut self, limit: usize) -> std::io::Result<Vec<u8>> {
        if !self.lock().connected {
            return Err(unplugged());
        }
        let reply = self
            .replies
            .pop_front()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "the player did not answer"))?;
        if reply.len() > limit {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "a reply was longer than allowed"));
        }
        Ok(reply)
    }
}
