//! Players reached over MTP (see docs/mtp.md): finding them, and their files as a [`Storage`].
//!
//! MTP names files by handle, not path, and cannot rename or overwrite. [`MtpStorage`] turns
//! paths into handles by listing folders (each once per connection, cached), makes folders as
//! needed, replaces a file by deleting it and sending the new one, and writes the manifest as a
//! new numbered copy before deleting the old, so a complete manifest is always there.
//!
//! Each storage of a device (internal memory, an SD card) is a player of its own.

use crate::device::{Device, DeviceError};
use crate::discover::{Candidate, Volume};
use crate::storage::{MANIFEST_DIR, Storage, manifest_generation, path_parts};
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use tunebox_mtp::codes::ROOT_PARENT;
use tunebox_mtp::usb::UsbPort;
use tunebox_mtp::{MtpError, Session, StorageInfo, Transport};

/// The largest manifest TuneBox reads (a manifest of 100,000 songs is about 20 MB).
const MANIFEST_LIMIT: usize = 64 * 1024 * 1024;

/// How to reach an MTP device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MtpLink {
    Usb(UsbPort),
    /// An in-memory device, for tests.
    #[cfg(any(test, feature = "simulator"))]
    Simulated(tunebox_mtp::simulator::SimulatedDevice),
}

impl MtpLink {
    /// Identifies the device while it stays connected.
    pub fn key(&self) -> String {
        match self {
            MtpLink::Usb(port) => port.key.clone(),
            #[cfg(any(test, feature = "simulator"))]
            MtpLink::Simulated(_) => "simulated".into(),
        }
    }

    /// The name the device gives over USB, before it is opened.
    pub fn name(&self) -> String {
        match self {
            MtpLink::Usb(port) => port.name.clone(),
            #[cfg(any(test, feature = "simulator"))]
            MtpLink::Simulated(_) => "Simulated Player".into(),
        }
    }

    fn open(&self) -> io::Result<Box<dyn Transport>> {
        match self {
            MtpLink::Usb(port) => Ok(Box::new(tunebox_mtp::usb::open(port)?)),
            #[cfg(any(test, feature = "simulator"))]
            MtpLink::Simulated(device) if device.is_held() => {
                Err(io::Error::new(io::ErrorKind::ResourceBusy, "held by another program"))
            }
            #[cfg(any(test, feature = "simulator"))]
            MtpLink::Simulated(device) => Ok(Box::new(device.connect())),
        }
    }
}

/// One storage of an MTP device: what a [`Volume`] of MTP access points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MtpAddress {
    pub link: MtpLink,
    pub storage: u32,
    /// The storage's description, e.g. "Internal shared storage".
    pub description: String,
}

impl MtpAddress {
    /// The identity TuneBox gives this storage in its device list (not a real path).
    pub fn root(&self) -> PathBuf {
        PathBuf::from(format!("mtp:{}/{:08x}", self.link.key(), self.storage))
    }
}

/// The MTP devices connected now, by USB, without opening them.
pub fn mtp_links() -> Vec<MtpLink> {
    tunebox_mtp::usb::ports().into_iter().map(MtpLink::Usb).collect()
}

/// What to tell the listener when an MTP player cannot be reached or stops answering.
pub fn explain(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::ResourceBusy => {
            let holder = if gvfs_holds_an_mtp_device() {
                "The Files app has it open. Eject the player in Files (the eject button beside it), then try again."
            } else if cfg!(target_os = "macos") {
                "Quit Image Capture, Photos or Android File Transfer, then try again."
            } else {
                "Eject it in your file manager (Files or Dolphin) or close the program using it, then try again."
            };
            format!("Another program is using the player. {holder}")
        }
        io::ErrorKind::PermissionDenied => "The system does not let TuneBox open the player. On Linux, a udev rule \
            must give your user access to it; see “Permission” in docs/mtp.md."
            .into(),
        io::ErrorKind::Unsupported => {
            "Syncing MTP players is not supported on this system yet; it works on Linux and macOS.".into()
        }
        io::ErrorKind::NotConnected
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::TimedOut => "The player was disconnected or stopped answering. Connect it again and sync \
            again: songs that did not finish copying are copied then."
            .into(),
        _ => format!("Could not talk to the player: {error}"),
    }
}

/// Whether GNOME's gvfs has an MTP device open: it lists each as `mtp:host=…` in its runtime
/// folder. Looking there touches no device.
fn gvfs_holds_an_mtp_device() -> bool {
    let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") else { return false };
    std::fs::read_dir(Path::new(&runtime).join("gvfs"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| entry.file_name().to_string_lossy().starts_with("mtp:host="))
}

fn unavailable(error: impl Into<io::Error>) -> DeviceError {
    DeviceError::Unavailable(explain(&error.into()))
}

/// Opens the storage at `address` for reading and writing.
pub fn connect(address: &MtpAddress) -> Result<MtpStorage<Box<dyn Transport>>, DeviceError> {
    let transport = address.link.open().map_err(unavailable)?;
    let session = Session::open(transport).map_err(unavailable)?;
    Ok(MtpStorage::new(session, address.storage))
}

/// Opens the device at `link` and describes each writable storage as a player, with its
/// TuneBox manifest if it has one. A device that cannot be opened, or shows no storage (an
/// Android phone that is locked or not set to file transfer), is still described, with the
/// problem, so the listener learns what to do. Cameras (PTP without MTP) are left out.
pub fn probe(link: &MtpLink) -> Vec<Candidate> {
    match probe_storages(link) {
        Ok(candidates) => candidates,
        Err(problem) => {
            let address = MtpAddress { link: link.clone(), storage: 0, description: String::new() };
            let volume = Volume {
                root: address.root(),
                label: link.name(),
                total_bytes: 0,
                free_bytes: 0,
                removable: true,
                access: crate::discover::Access::Mtp(address),
            };
            vec![Candidate { volume, device: None, problem: Some(problem) }]
        }
    }
}

fn probe_storages(link: &MtpLink) -> Result<Vec<Candidate>, String> {
    let transport = link.open().map_err(|e| explain(&e))?;
    let mut session = Session::open(transport).map_err(|e| explain(&e.into()))?;
    let info = session.device_info().map_err(|e| explain(&e.into()))?;
    if !info.is_mtp() {
        return Ok(Vec::new());
    }
    let name = info.display_name();
    let ids = session.storage_ids().map_err(|e| explain(&e.into()))?;
    let storages: Vec<(u32, StorageInfo)> = ids
        .into_iter()
        .filter_map(|id| session.storage_info(id).ok().map(|info| (id, info)))
        .filter(|(_, info)| info.writable())
        .collect();
    if storages.is_empty() {
        return Err("The player shows no storage TuneBox can write to. If it is a phone, unlock it and choose \
            “File transfer” (or allow access to its data) when it asks."
            .into());
    }
    let several = storages.len() > 1;
    let mut storage = MtpStorage::new(session, 0);
    let mut candidates = Vec::new();
    for (id, info) in storages {
        storage.select(id);
        let address = MtpAddress { link: link.clone(), storage: id, description: info.description.clone() };
        let root = address.root();
        let (device, problem) = match Device::read_from(&mut storage, &root) {
            Ok(device) => (device, None),
            Err(e) => {
                log::warn!("ignoring the TuneBox manifest on {name}: {e}");
                (None, None)
            }
        };
        let label = if several && !info.description.is_empty() {
            format!("{name} ({})", info.description)
        } else {
            name.clone()
        };
        let volume = Volume {
            root,
            label,
            total_bytes: info.max_capacity,
            free_bytes: info.free_space,
            removable: true,
            access: crate::discover::Access::Mtp(address),
        };
        candidates.push(Candidate { volume, device, problem });
    }
    Ok(candidates)
}

/// A folder or file seen on the device.
#[derive(Clone, Copy, Debug)]
struct Entry {
    handle: u32,
    folder: bool,
}

/// One storage of an MTP device, as a [`Storage`].
pub struct MtpStorage<T: Transport> {
    session: Session<T>,
    storage: u32,
    /// The contents of each folder listed so far (`ROOT_PARENT` for the top level).
    folders: HashMap<u32, BTreeMap<String, Entry>>,
}

impl<T: Transport> MtpStorage<T> {
    pub fn new(session: Session<T>, storage: u32) -> Self {
        Self { session, storage, folders: HashMap::new() }
    }

    fn select(&mut self, storage: u32) {
        self.storage = storage;
        self.folders.clear();
    }

    /// The contents of `folder`, listed from the device the first time.
    fn contents(&mut self, folder: u32) -> Result<&mut BTreeMap<String, Entry>, MtpError> {
        if !self.folders.contains_key(&folder) {
            let mut contents = BTreeMap::new();
            for handle in self.session.object_handles(self.storage, folder)? {
                let info = self.session.object_info(handle)?;
                contents.insert(info.filename.clone(), Entry { handle, folder: info.is_folder() });
            }
            self.folders.insert(folder, contents);
        }
        self.folders.get_mut(&folder).ok_or_else(|| MtpError::Protocol("a folder listing went missing".into()))
    }

    /// The entry called `name` in `folder`: the exact name, or else one differing only in
    /// case (many players' storage ignores case, and would refuse a second "music").
    fn child(&mut self, folder: u32, name: &str) -> Result<Option<(String, Entry)>, MtpError> {
        let contents = self.contents(folder)?;
        if let Some(entry) = contents.get(name) {
            return Ok(Some((name.to_owned(), *entry)));
        }
        let lower = name.to_lowercase();
        Ok(contents.iter().find(|(n, _)| n.to_lowercase() == lower).map(|(n, e)| (n.clone(), *e)))
    }

    /// The handle of the folder `parts`, made as needed.
    fn folder(&mut self, parts: &[&str], create: bool) -> Result<Option<u32>, MtpError> {
        let mut current = ROOT_PARENT;
        for part in parts {
            current = match self.child(current, part)? {
                Some((_, entry)) if entry.folder => entry.handle,
                Some(_) => return Err(MtpError::Protocol(format!("“{part}” is a file, not a folder"))),
                None if create => {
                    let handle = self.session.create_folder(self.storage, current, part)?;
                    self.contents(current)?.insert((*part).to_owned(), Entry { handle, folder: true });
                    self.folders.insert(handle, BTreeMap::new());
                    handle
                }
                None => return Ok(None),
            };
        }
        Ok(Some(current))
    }

    /// Sends `len` bytes from `body` as the file at `path`, replacing any file there.
    fn send(&mut self, path: &str, len: u64, body: &mut dyn io::Read) -> Result<(), MtpError> {
        let parts = path_parts(path);
        let Some((name, folders)) = parts.split_last() else {
            return Err(MtpError::Protocol(format!("“{path}” is not a file name")));
        };
        let parent = self.folder(folders, true)?.unwrap_or(ROOT_PARENT);
        if let Some((existing, entry)) = self.child(parent, name)? {
            if entry.folder {
                return Err(MtpError::Protocol(format!("“{path}” is a folder")));
            }
            self.session.delete_object(entry.handle)?;
            self.contents(parent)?.remove(&existing);
        }
        let handle = self.session.send_file(self.storage, parent, name, len, body)?;
        self.contents(parent)?.insert((*name).to_owned(), Entry { handle, folder: false });
        Ok(())
    }

    /// Removes the file at `path`, then each folder above it that is left empty.
    fn remove(&mut self, path: &str) -> Result<bool, MtpError> {
        let parts = path_parts(path);
        let Some((name, folders)) = parts.split_last() else { return Ok(false) };
        let Some(parent) = self.folder(folders, false)? else { return Ok(false) };
        let Some((existing, entry)) = self.child(parent, name)? else { return Ok(false) };
        if entry.folder {
            return Err(MtpError::Protocol(format!("“{path}” is a folder")));
        }
        self.session.delete_object(entry.handle)?;
        self.contents(parent)?.remove(&existing);
        // Deleting a folder deletes what is in it, so each is listed afresh from the device and
        // only removed when that listing is empty.
        for depth in (1..=folders.len()).rev() {
            let Some(folder) = self.folder(folders.get(..depth).unwrap_or_default(), false)? else { break };
            if !self.session.object_handles(self.storage, folder)?.is_empty() {
                break;
            }
            self.session.delete_object(folder)?;
            self.folders.remove(&folder);
            let above = self.folder(folders.get(..depth - 1).unwrap_or_default(), false)?.unwrap_or(ROOT_PARENT);
            if let Some(contents) = self.folders.get_mut(&above) {
                contents.retain(|_, entry| entry.handle != folder);
            }
        }
        Ok(true)
    }

    /// The manifests in `.tunebox`, newest first.
    fn manifests(&mut self) -> Result<Vec<(u64, String, u32)>, MtpError> {
        let Some(dir) = self.folder(&[MANIFEST_DIR], false)? else { return Ok(Vec::new()) };
        let mut found: Vec<(u64, String, u32)> = self
            .contents(dir)?
            .iter()
            .filter(|(_, entry)| !entry.folder)
            .filter_map(|(name, entry)| Some((manifest_generation(name)?, name.clone(), entry.handle)))
            .collect();
        found.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        Ok(found)
    }
}

impl<T: Transport> Storage for MtpStorage<T> {
    fn read_manifest(&mut self) -> io::Result<Option<Vec<u8>>> {
        for (_, name, handle) in self.manifests()? {
            let bytes = self.session.get_object(handle, MANIFEST_LIMIT)?;
            // A copy cut off while being written does not parse: the one before it is used.
            if serde_json::from_slice::<serde::de::IgnoredAny>(&bytes).is_ok() {
                return Ok(Some(bytes));
            }
            log::warn!("skipping the unfinished manifest {name} on the player");
        }
        Ok(None)
    }

    fn write_manifest(&mut self, bytes: &[u8]) -> io::Result<()> {
        let older = self.manifests()?;
        let next = older.first().map_or(1, |(generation, _, _)| generation + 1);
        let path = format!("{MANIFEST_DIR}/device-{next}.json");
        self.send(&path, bytes.len() as u64, &mut &bytes[..])?;
        for (_, name, handle) in older {
            self.session.delete_object(handle)?;
            if let Some(dir) = self.folder(&[MANIFEST_DIR], false)? {
                self.contents(dir)?.remove(&name);
            }
        }
        Ok(())
    }

    fn put_file(&mut self, source: &Path, path: &str) -> io::Result<()> {
        let file = std::fs::File::open(source)?;
        let len = file.metadata()?.len();
        let mut reader = io::BufReader::with_capacity(256 * 1024, file);
        Ok(self.send(path, len, &mut reader)?)
    }

    fn write_file(&mut self, path: &str, bytes: &[u8]) -> io::Result<()> {
        Ok(self.send(path, bytes.len() as u64, &mut &bytes[..])?)
    }

    fn remove_file(&mut self, path: &str) -> io::Result<bool> {
        Ok(self.remove(path)?)
    }

    fn free_bytes(&mut self) -> io::Result<Option<u64>> {
        Ok(Some(self.session.storage_info(self.storage)?.free_space))
    }
}
