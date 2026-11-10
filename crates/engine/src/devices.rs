//! Players: finding them, setting them up, and syncing them. Mass-storage players come from
//! the mounted volumes the frontend passes to [`Session::refresh_devices`]; MTP players from
//! [`find_mtp_players`] (slow: it opens each device), which the app runs on a thread of its
//! own and passes to [`Session::refresh_mtp_devices`]. Both then work the same way.

use crate::EngineError;
use crate::jobs::{JobId, JobKind};
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use chrono::Utc;
use std::path::Path;
use std::time::UNIX_EPOCH;
use tunebox_model::{Track, format_bytes};
use tunebox_sync::{
    Candidate, Device, DeviceError, PlayerModel, PlaylistFile, SyncItem, SyncMode, SyncReport, SyncSettings, Volume,
};

/// A player in the source list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceEntry {
    pub volume: Volume,
    /// Set once TuneBox manages the player.
    pub device: Option<Device>,
    /// Why it cannot be used now and what to do (an MTP player another program holds).
    pub problem: Option<String>,
}

impl From<Candidate> for DeviceEntry {
    fn from(c: Candidate) -> Self {
        DeviceEntry { volume: c.volume, device: c.device, problem: c.problem }
    }
}

/// The MTP players connected now, each storage as a player of its own. Opens every MTP device,
/// so it takes a moment: call it off the interface's thread.
pub fn find_mtp_players() -> Vec<Candidate> {
    tunebox_sync::mtp::mtp_links().iter().flat_map(tunebox_sync::mtp::probe).collect()
}

/// A device error as the listener should read it: for MTP players, what happened to the
/// connection and what to do.
fn device_message(volume: &Volume, error: DeviceError) -> String {
    match error {
        DeviceError::Io(e) if volume.is_mtp() => tunebox_sync::mtp::explain(&e),
        other => other.to_string(),
    }
}

impl DeviceEntry {
    /// The name shown in the source list.
    pub fn name(&self) -> &str {
        match &self.device {
            Some(device) => &device.manifest.name,
            None if !self.volume.label.is_empty() => &self.volume.label,
            None => "Music Player",
        }
    }
}

impl Session {
    pub fn devices(&self) -> &[DeviceEntry] {
        &self.devices
    }

    pub fn device(&self, root: &Path) -> Option<&DeviceEntry> {
        self.devices.iter().find(|d| d.volume.root == root)
    }

    /// Updates the mass-storage players from the volumes currently mounted (the frontend asks
    /// the platform for them every few seconds). MTP players are left as they are.
    pub fn refresh_devices(&mut self, volumes: &[Volume]) {
        let mut devices: Vec<DeviceEntry> = tunebox_sync::scan(volumes).into_iter().map(DeviceEntry::from).collect();
        let drives: Vec<Volume> = volumes
            .iter()
            .filter(|v| v.removable && !devices.iter().any(|d| d.volume.root == v.root))
            .cloned()
            .collect();
        if drives != self.drives {
            self.drives = drives;
            self.content_changed();
        }
        devices.extend(self.devices.iter().filter(|d| d.volume.is_mtp()).cloned());
        self.set_devices(devices);
    }

    /// Updates the MTP players from what [`find_mtp_players`] found. A player already listed
    /// keeps what this session knows of it (it may have been set up or synced since it was
    /// found); players no longer found are dropped.
    pub fn refresh_mtp_devices(&mut self, found: &[Candidate]) {
        let mut devices: Vec<DeviceEntry> = self.devices.iter().filter(|d| !d.volume.is_mtp()).cloned().collect();
        for candidate in found {
            let known = self.devices.iter().find(|d| d.volume.root == candidate.volume.root);
            devices.push(known.cloned().unwrap_or_else(|| DeviceEntry::from(candidate.clone())));
        }
        self.set_devices(devices);
    }

    fn set_devices(&mut self, devices: Vec<DeviceEntry>) {
        if devices != self.devices {
            self.devices = devices;
            self.content_changed();
        }
    }

    /// Removable drives that are not players yet (a blank microSD card in a card reader), which
    /// "Set Up Drive for Syncing…" offers.
    pub fn drives(&self) -> &[Volume] {
        &self.drives
    }

    /// Sets up the player at `root` (a listed player, or one of [`Session::drives`]) for
    /// syncing under `name`, for `player`. The choice is recorded on the device, so a card is
    /// recognised in any reader. A device set up before keeps its record and settings; only its
    /// name and player change.
    pub fn register_device(&mut self, root: &Path, name: &str, player: PlayerModel) -> Result<(), EngineError> {
        let volume = match self.drives.iter().find(|v| v.root == root) {
            Some(drive) => drive.clone(),
            None => self.connected_volume(root)?,
        };
        let registered = volume.storage().and_then(|mut storage| {
            let mut device = Device::register_on(&mut *storage, root, name)?;
            let settings = &mut device.manifest.settings;
            if device.manifest.files.is_empty() && settings.player != player {
                // Newly set up: start from that player's defaults.
                *settings =
                    SyncSettings { music_folder: settings.music_folder.clone(), ..SyncSettings::for_player(player) };
            }
            settings.player = player;
            device.save_to(&mut *storage)?;
            Ok(device)
        });
        let device = registered.map_err(|e| EngineError::Device(device_message(&volume, e)))?;
        match self.devices.iter_mut().find(|d| d.volume.root == root) {
            Some(entry) => entry.device = Some(device),
            None => {
                self.drives.retain(|d| d.root != root);
                self.devices.push(DeviceEntry { volume, device: Some(device), problem: None });
            }
        }
        self.content_changed();
        Ok(())
    }

    /// Changes a managed player's sync settings and saves them on the player.
    pub fn set_device_settings(&mut self, root: &Path, settings: SyncSettings) -> Result<(), EngineError> {
        self.change_device(root, |device| device.manifest.settings = settings)
    }

    pub fn rename_device(&mut self, root: &Path, name: &str) -> Result<(), EngineError> {
        self.change_device(root, |device| device.manifest.name = name.trim().to_owned())
    }

    /// Changes a managed player's manifest and saves it on the player. A mounted player is
    /// written at once. An MTP player must be opened over USB first, which can take seconds, so
    /// the change applies here at once and is written by a job (see [`Session::save_device`]).
    fn change_device(&mut self, root: &Path, change: impl FnOnce(&mut Device)) -> Result<(), EngineError> {
        let volume = self.connected_volume(root)?;
        let device = self.managed_device_mut(root)?;
        change(device);
        if volume.is_mtp() {
            self.content_changed();
            self.save_device(root);
            return Ok(());
        }
        let saved = volume
            .storage()
            .and_then(|mut storage| device.save_to(&mut *storage))
            .map_err(|e| EngineError::Device(device_message(&volume, e)));
        self.content_changed();
        saved
    }

    /// Writes an MTP player's manifest as it is now, in the background. One save runs at a
    /// time (the player can only be opened once); a player changed meanwhile is saved again
    /// when it ends, with its latest settings.
    fn save_device(&mut self, root: &Path) {
        if self.jobs.is_running(JobKind::SaveDevice) {
            if !self.unsaved_devices.iter().any(|r| r == root) {
                self.unsaved_devices.push(root.to_path_buf());
            }
            return;
        }
        let Some(entry) = self.device(root) else { return };
        let Some(device) = entry.device.clone() else { return };
        let volume = entry.volume.clone();
        let root = root.to_path_buf();
        let title = format!("Saving the settings of “{}”", device.manifest.name);
        self.jobs.spawn(JobKind::SaveDevice, title, move |_| {
            let result = volume
                .storage()
                .and_then(|mut storage| device.save_to(&mut *storage))
                .map_err(|e| device_message(&volume, e));
            Outcome::DeviceSaved { root, result }
        });
    }

    pub(crate) fn finish_device_save(&mut self, root: &Path, result: Result<(), String>) {
        if let Err(message) = result {
            let name = self.device(root).map_or_else(|| "the player".to_owned(), |d| format!("“{}”", d.name()));
            // The session keeps the change, and a sync writes the whole manifest.
            self.notices.push(Notice::warning(format!(
                "The settings were not saved on {name}. {message} They will be saved with the next sync."
            )));
        }
        // Players that went away meanwhile are no longer listed and are skipped.
        if !self.unsaved_devices.is_empty() {
            let next = self.unsaved_devices.remove(0);
            self.save_device(&next);
        }
    }

    /// The volume of a connected player, ready to be written. An MTP player is held by a sync
    /// while one runs, so it cannot be opened again until that ends.
    fn connected_volume(&self, root: &Path) -> Result<Volume, EngineError> {
        let volume = self
            .device(root)
            .map(|entry| entry.volume.clone())
            .ok_or_else(|| EngineError::Device("The player is no longer connected.".into()))?;
        if volume.is_mtp() && self.jobs.is_running(JobKind::Sync) {
            return Err(EngineError::Busy("A sync"));
        }
        Ok(volume)
    }

    /// Drops a player from the list (after the frontend has ejected it).
    pub fn forget_device(&mut self, root: &Path) {
        self.devices.retain(|d| d.volume.root != root);
        self.content_changed();
    }

    fn managed_device_mut(&mut self, root: &Path) -> Result<&mut Device, EngineError> {
        self.devices
            .iter_mut()
            .find(|d| d.volume.root == root)
            .and_then(|d| d.device.as_mut())
            .ok_or_else(|| EngineError::Device("This player is not set up for syncing yet.".into()))
    }

    /// Syncs a managed player in the background.
    pub fn sync_device(&mut self, root: &Path) -> Result<JobId, EngineError> {
        if self.jobs.is_running(JobKind::Sync) {
            return Err(EngineError::Busy("A sync"));
        }
        // A save holds an MTP player open; the sync would find it busy.
        if self.jobs.is_running(JobKind::SaveDevice) || !self.unsaved_devices.is_empty() {
            return Err(EngineError::Busy("Saving a player's settings"));
        }
        let entry =
            self.device(root).ok_or_else(|| EngineError::Device("The player is no longer connected.".into()))?;
        let mut device = entry
            .device
            .clone()
            .ok_or_else(|| EngineError::Device("This player is not set up for syncing yet.".into()))?;
        if let Some(problem) = &entry.problem {
            return Err(EngineError::Device(problem.clone()));
        }
        let volume = entry.volume.clone();
        let settings = device.manifest.settings.clone();
        let (tracks, playlists) = self.sync_selection(&settings);
        // Songs known to be missing or out of reach are not even looked at: a share that is not
        // connected could stall the sync.
        let readable: Vec<bool> =
            tracks.iter().map(|t| *self.file_status(t.id) == crate::file_check::FileStatus::Present).collect();
        let root = root.to_path_buf();
        let title = format!("Syncing “{}”", device.manifest.name);
        Ok(self.jobs.spawn(JobKind::Sync, title, move |ctx| {
            ctx.progress(0.0, "Preparing to sync…");
            let items: Vec<SyncItem> = tracks
                .iter()
                .zip(readable)
                .map(|(t, readable)| {
                    let modified = if readable { modified_secs(&t.location) } else { None };
                    SyncItem::from_track(t, &settings.music_folder, modified)
                })
                .collect();
            let mut storage = match volume.storage() {
                Ok(storage) => storage,
                Err(e) => return Outcome::Synced { root, result: Err(device_message(&volume, e)) },
            };
            // An MTP player says how much room it has; a mounted one was measured when listed.
            let free_bytes = storage.free_bytes().ok().flatten().unwrap_or(volume.free_bytes);
            let plan = tunebox_sync::plan(&items, playlists, &device.manifest);
            let result = if plan.fits(free_bytes) {
                tunebox_sync::execute_on(&mut device, &mut *storage, &plan, ctx.cancel_flag(), &mut |p| {
                    ctx.progress(p.done as f32 / p.total.max(1) as f32, p.message);
                })
                .map(|report| (device, report))
                .map_err(|e| device_message(&volume, e))
            } else {
                Err(format!(
                    "There is not enough free space on the player: the sync needs {} more than the {} available.",
                    format_bytes(plan.bytes_to_copy.saturating_sub(plan.bytes_to_free)),
                    format_bytes(free_bytes)
                ))
            };
            Outcome::Synced { root, result }
        }))
    }

    /// The tracks and playlists a player's settings select.
    fn sync_selection(&self, settings: &SyncSettings) -> (Vec<Track>, Vec<PlaylistFile>) {
        let now = Utc::now();
        let playlist_ids: Vec<_> = match &settings.mode {
            SyncMode::Everything => self.library.playlists().iter().map(|p| p.id).collect(),
            SyncMode::Playlists(ids) => ids.clone(),
        };
        let playlists: Vec<PlaylistFile> = playlist_ids
            .iter()
            .filter_map(|id| {
                let playlist = self.library.playlist(*id)?;
                let tracks = self.library.playlist_tracks(*id, now).ok()?;
                Some(PlaylistFile { name: playlist.name.clone(), tracks })
            })
            .collect();
        let wanted = |t: &&Track| !settings.only_checked || t.checked;
        let tracks: Vec<Track> = match &settings.mode {
            SyncMode::Everything => self.library.tracks().filter(wanted).cloned().collect(),
            SyncMode::Playlists(_) => {
                let mut ids: Vec<_> = playlists.iter().flat_map(|p| p.tracks.iter().copied()).collect();
                ids.sort();
                ids.dedup();
                ids.iter().filter_map(|id| self.library.track(*id)).filter(wanted).cloned().collect()
            }
        };
        (tracks, playlists)
    }

    pub(crate) fn finish_sync(&mut self, root: &Path, result: Result<(Device, SyncReport), String>) {
        match result {
            Ok((device, report)) => {
                let name = device.manifest.name.clone();
                if let Some(entry) = self.devices.iter_mut().find(|d| d.volume.root == root) {
                    entry.device = Some(device);
                    if let Some(free) = report.free_bytes {
                        entry.volume.free_bytes = free;
                    }
                }
                let text = if report.cancelled {
                    format!("Sync of “{name}” stopped. {} songs copied, {} removed.", report.copied, report.deleted)
                } else {
                    format!("“{name}” is up to date. {} songs copied, {} removed.", report.copied, report.deleted)
                };
                let text = match report.unplayable {
                    0 => text,
                    n => {
                        let player = device_player(self.device(root));
                        let songs = if n == 1 { "1 song was".to_owned() } else { format!("{n} songs were") };
                        let their = if n == 1 { "its" } else { "their" };
                        format!("{text} {songs} left off: the {player} cannot play {their} format.")
                    }
                };
                let text = match report.unavailable {
                    0 => text,
                    1 => format!(
                        "{text} 1 song could not be read (missing, or its drive or share is not connected); its copy on the player was kept."
                    ),
                    n => format!(
                        "{text} {n} songs could not be read (missing, or their drive or share is not connected); their copies on the player were kept."
                    ),
                };
                self.notices.push(Notice::info(text));
                if let Some(first) = report.failures.first() {
                    let more = report.failures.len() - 1;
                    let suffix = if more > 0 { format!(" ({more} more problems)") } else { String::new() };
                    self.notices.push(Notice::warning(format!("{first}{suffix}")));
                }
            }
            Err(message) => self.notices.push(Notice::error(format!("The sync did not complete. {message}"))),
        }
    }
}

/// The name of the player a device is synced for, for messages.
fn device_player(entry: Option<&DeviceEntry>) -> &'static str {
    match entry.and_then(|e| e.device.as_ref()).map(|d| d.manifest.settings.player) {
        Some(model) if model != PlayerModel::Generic => model.label(),
        _ => "player",
    }
}

/// A file's modification time in whole seconds; `None` when the file cannot be read. A file
/// whose system keeps no times reads as 0 (then any change in size still triggers a re-copy).
fn modified_secs(path: &Path) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(metadata.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()))
}
