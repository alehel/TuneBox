//! Players: finding them, setting them up, and syncing them.

use crate::EngineError;
use crate::jobs::{JobId, JobKind};
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use chrono::Utc;
use std::path::Path;
use std::time::UNIX_EPOCH;
use tunebox_model::{Track, format_bytes};
use tunebox_sync::{Device, PlaylistFile, SyncItem, SyncMode, SyncReport, SyncSettings, Volume};

/// A player in the source list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceEntry {
    pub volume: Volume,
    /// Set once TuneBox manages the player.
    pub device: Option<Device>,
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

    /// Updates the device list from the volumes currently mounted (the frontend asks the
    /// platform for them every few seconds).
    pub fn refresh_devices(&mut self, volumes: &[Volume]) {
        let found = tunebox_sync::scan(volumes);
        let devices: Vec<DeviceEntry> =
            found.into_iter().map(|c| DeviceEntry { volume: c.volume, device: c.device }).collect();
        if devices != self.devices {
            self.devices = devices;
            self.content_changed();
        }
    }

    /// Sets up the player at `root` for syncing under `name` ("Set up as a TuneBox device").
    pub fn register_device(&mut self, root: &Path, name: &str) -> Result<(), EngineError> {
        let device = Device::register(root, name).map_err(|e| EngineError::Device(e.to_string()))?;
        if let Some(entry) = self.devices.iter_mut().find(|d| d.volume.root == root) {
            entry.device = Some(device);
        }
        self.content_changed();
        Ok(())
    }

    /// Changes a managed player's sync settings and saves them on the player.
    pub fn set_device_settings(&mut self, root: &Path, settings: SyncSettings) -> Result<(), EngineError> {
        let device = self.managed_device_mut(root)?;
        device.manifest.settings = settings;
        let saved = device.save().map_err(|e| EngineError::Device(e.to_string()));
        self.content_changed();
        saved
    }

    pub fn rename_device(&mut self, root: &Path, name: &str) -> Result<(), EngineError> {
        let device = self.managed_device_mut(root)?;
        device.manifest.name = name.trim().to_owned();
        let saved = device.save().map_err(|e| EngineError::Device(e.to_string()));
        self.content_changed();
        saved
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
        let entry =
            self.device(root).ok_or_else(|| EngineError::Device("The player is no longer connected.".into()))?;
        let mut device = entry
            .device
            .clone()
            .ok_or_else(|| EngineError::Device("This player is not set up for syncing yet.".into()))?;
        let free_bytes = entry.volume.free_bytes;
        let settings = device.manifest.settings.clone();
        let (tracks, playlists) = self.sync_selection(&settings);
        let root = root.to_path_buf();
        let title = format!("Syncing “{}”", device.manifest.name);
        Ok(self.jobs.spawn(JobKind::Sync, title, move |ctx| {
            ctx.progress(0.0, "Preparing to sync…");
            let items: Vec<SyncItem> = tracks
                .iter()
                .map(|t| SyncItem::from_track(t, &settings.music_folder, modified_secs(&t.location)))
                .collect();
            let plan = tunebox_sync::plan(&items, playlists, &device.manifest);
            let result = if plan.fits(free_bytes) {
                tunebox_sync::execute(&mut device, &plan, ctx.cancel_flag(), &mut |p| {
                    ctx.progress(p.done as f32 / p.total.max(1) as f32, p.message);
                })
                .map(|report| (device, report))
                .map_err(|e| e.to_string())
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
                }
                let text = if report.cancelled {
                    format!("Sync of “{name}” stopped. {} songs copied, {} removed.", report.copied, report.deleted)
                } else {
                    format!("“{name}” is up to date. {} songs copied, {} removed.", report.copied, report.deleted)
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

/// A file's modification time in whole seconds, or 0 when unknown (then any change in size
/// still triggers a re-copy).
fn modified_secs(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}
