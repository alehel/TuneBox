//! The audio CD in the drive: reading it, naming its tracks, and importing it.

use crate::EngineError;
use crate::jobs::{JobId, JobKind};
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use chrono::Utc;
use std::collections::BTreeSet;
use std::sync::Arc;
use tunebox_rip::{CdDrive, DiscInfo, DiscToc, DriveIdentity, RipReport, RipRequest};

/// A disc in a drive, and what the listener has decided about it.
#[derive(Clone)]
pub struct CdState {
    pub drive: Arc<dyn CdDrive>,
    pub toc: DiscToc,
    pub disc_id: String,
    pub info: DiscInfo,
    /// Other releases a lookup found, for the listener to choose from.
    pub alternatives: Vec<DiscInfo>,
    /// The import checkbox of each audio track, in disc order.
    pub include: Vec<bool>,
    /// Audio track numbers imported in this session.
    pub imported: BTreeSet<u8>,
}

impl std::fmt::Debug for CdState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdState")
            .field("drive", &self.drive.name())
            .field("disc_id", &self.disc_id)
            .field("info", &self.info)
            .finish()
    }
}

impl CdState {
    /// Reads the disc in `drive`. `Ok(None)` when the drive is empty.
    pub fn read(drive: Arc<dyn CdDrive>) -> Result<Option<CdState>, String> {
        let toc = match drive.read_toc() {
            Ok(toc) => toc,
            Err(tunebox_rip::DriveError::NoDisc | tunebox_rip::DriveError::NotAudio) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let info = DiscInfo::placeholder(&toc);
        let include = vec![true; info.tracks.len()];
        Ok(Some(CdState {
            disc_id: tunebox_rip::disc_id(&toc),
            drive,
            toc,
            info,
            alternatives: Vec::new(),
            include,
            imported: BTreeSet::new(),
        }))
    }

    /// The disc's audio track numbers, in order.
    pub fn audio_numbers(&self) -> Vec<u8> {
        self.toc.audio_tracks().map(|t| t.number).collect()
    }

    /// The name shown in the source list: the album, once known.
    pub fn title(&self) -> &str {
        &self.info.album
    }
}

impl Session {
    pub fn cd(&self) -> Option<&CdState> {
        self.cd.as_ref()
    }

    /// Reads the disc in `drive` in the background.
    pub fn read_disc(&mut self, drive: Arc<dyn CdDrive>) -> JobId {
        self.jobs.spawn(JobKind::ReadDisc, "Reading the disc".into(), move |_| Outcome::DiscRead(CdState::read(drive)))
    }

    /// Looks for an audio CD in this computer's drives in the background. Returns `None` while a
    /// disc is being read or imported. Frontends call it every few seconds.
    #[cfg(feature = "drives")]
    pub fn scan_drives(&mut self) -> Option<JobId> {
        if self.jobs.is_running(JobKind::ReadDisc) || self.jobs.is_running(JobKind::Rip) {
            return None;
        }
        Some(self.jobs.spawn(JobKind::ReadDisc, "Looking for discs".into(), |_| {
            let drives = tunebox_rip::PhysicalDrive::list().unwrap_or_default();
            let Some(drive) = drives.into_iter().find(|d| d.has_audio_cd) else { return Outcome::DiscRead(Ok(None)) };
            Outcome::DiscRead(CdState::read(Arc::new(drive)))
        }))
    }

    pub(crate) fn finish_disc_read(&mut self, result: Result<Option<CdState>, String>) {
        match result {
            // The same disc again: keep the names and choices already made.
            Ok(Some(disc)) if self.cd.as_ref().is_some_and(|cd| cd.disc_id == disc.disc_id) => {}
            Ok(Some(disc)) => {
                self.cd = Some(disc);
                #[cfg(feature = "musicbrainz")]
                if self.prefs.lookup_cd_names_automatically {
                    let _ = self.lookup_disc_names();
                }
            }
            Ok(None) if !self.jobs.is_running(JobKind::Rip) => self.cd = None,
            Ok(None) => {}
            Err(message) => self.notices.push(Notice::error(format!("Could not read the disc: {message}"))),
        }
    }

    /// Forgets the disc (after the frontend ejects it).
    pub fn eject_disc(&mut self) {
        self.cd = None;
        self.content_changed();
    }

    /// Replaces the album and track names (typed in, or chosen among lookup results).
    pub fn set_disc_info(&mut self, info: DiscInfo) {
        if let Some(cd) = self.cd.as_mut() {
            cd.info = info;
        }
        self.content_changed();
    }

    pub fn set_cd_track_included(&mut self, index: usize, include: bool) {
        if let Some(flag) = self.cd.as_mut().and_then(|cd| cd.include.get_mut(index)) {
            *flag = include;
        }
    }

    /// Looks up the disc's names on MusicBrainz ("Get Track Names").
    #[cfg(feature = "musicbrainz")]
    pub fn lookup_disc_names(&mut self) -> Result<JobId, EngineError> {
        let disc_id = self.cd.as_ref().map(|cd| cd.disc_id.clone()).ok_or(EngineError::NoDisc)?;
        if self.jobs.is_running(JobKind::LookupNames) {
            return Err(EngineError::Busy("Looking up track names"));
        }
        Ok(self.jobs.spawn(JobKind::LookupNames, "Getting track names".into(), move |ctx| {
            ctx.progress(0.0, "Contacting MusicBrainz…");
            let result = tunebox_rip::musicbrainz::lookup(&disc_id).map_err(|e| e.to_string());
            Outcome::NamesFound { disc_id, result }
        }))
    }

    #[cfg(feature = "musicbrainz")]
    pub(crate) fn finish_lookup(&mut self, disc_id: &str, result: Result<Vec<DiscInfo>, String>) {
        let Some(cd) = self.cd.as_mut().filter(|cd| cd.disc_id == disc_id) else { return };
        match result {
            Ok(found) if found.is_empty() => self
                .notices
                .push(Notice::info("This disc is not in the MusicBrainz database. You can type its names in.")),
            Ok(mut found) => {
                let mut best = found.remove(0);
                // Keep a name for every track on this disc even if the release lists fewer.
                let count = cd.info.tracks.len();
                best.tracks.resize(count.max(best.tracks.len()), (String::new(), String::new()));
                best.tracks.truncate(count);
                cd.info = best;
                cd.alternatives = found;
            }
            Err(message) => self.notices.push(Notice::warning(format!("Could not get track names. {message}"))),
        }
    }

    /// The read offset the next import from the drive in use corrects, in samples, and which
    /// drive it is saved for. `None` without a disc.
    pub fn cd_read_offset(&self) -> Option<(DriveIdentity, i32)> {
        let identity = self.cd.as_ref()?.drive.identity();
        let offset = self.prefs.read_offset(identity.key());
        Some((identity, offset))
    }

    /// Saves the read offset of the drive in use (see `docs/accuraterip.md`). Values far outside
    /// any real drive's are refused with an explanation.
    pub fn set_cd_read_offset(&mut self, samples: i64) -> Result<(), EngineError> {
        let identity = self.cd.as_ref().ok_or(EngineError::NoDisc)?.drive.identity();
        let offset = tunebox_rip::check_read_offset(samples).map_err(EngineError::ReadOffset)?;
        let mut prefs = self.prefs.clone();
        if offset == 0 {
            prefs.read_offsets.remove(identity.key());
        } else {
            prefs.read_offsets.insert(identity.key().to_string(), offset);
        }
        self.set_preferences(prefs);
        Ok(())
    }

    /// Imports the checked tracks of the disc into the media folder and the library.
    pub fn import_cd(&mut self) -> Result<JobId, EngineError> {
        if self.jobs.is_running(JobKind::Rip) {
            return Err(EngineError::Busy("Importing the CD"));
        }
        if !crate::files::may_write(&self.prefs, &self.prefs.media_folder) {
            return Err(EngineError::Protected(self.prefs.media_folder.clone()));
        }
        let cd = self.cd.as_ref().ok_or(EngineError::NoDisc)?;
        let tracks: Vec<u8> = cd
            .audio_numbers()
            .into_iter()
            .zip(&cd.include)
            .filter(|(_, include)| **include)
            .map(|(number, _)| number)
            .collect();
        let request = RipRequest {
            toc: cd.toc.clone(),
            tracks,
            info: cd.info.clone(),
            format: self.prefs.rip_format,
            media_folder: self.prefs.media_folder.clone(),
            read_offset: self.prefs.read_offset(cd.drive.identity().key()),
        };
        let drive = Arc::clone(&cd.drive);
        let title = format!("Importing “{}”", cd.info.album);
        Ok(self.jobs.spawn(JobKind::Rip, title, move |ctx| {
            let report = tunebox_rip::rip(drive.as_ref(), &request, Utc::now(), ctx.cancel_flag(), &mut |p| {
                ctx.progress(p.overall(), format!("Importing “{}” ({} of {})", p.title, p.index + 1, p.count));
            });
            Outcome::Ripped(report)
        }))
    }

    pub(crate) fn finish_rip(&mut self, report: RipReport) {
        let count = report.tracks.len();
        for track in report.tracks {
            let number = track.meta.track.number;
            self.library.add_track(track);
            if let (Some(cd), Some(n)) = (self.cd.as_mut(), number) {
                // The tag holds the position among audio tracks; map it back to the disc number.
                if let Some(disc_number) = cd.audio_numbers().get(n.saturating_sub(1) as usize) {
                    cd.imported.insert(*disc_number);
                }
            }
        }
        if count > 0 {
            self.library_changed();
        }
        let verb = if report.cancelled { "Stopped after importing" } else { "Imported" };
        self.notices.push(Notice::info(format!("{verb} {count} tracks from the CD.")));
        for (track, reason) in report.failures {
            self.notices.push(Notice::warning(format!("Track {track} could not be imported: {reason}")));
        }
    }
}
