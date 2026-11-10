//! Setting and removing songs' covers (Get Info's artwork, `tunebox-cli artwork`).
//!
//! Writing a cover into every song of an album is slow, so it runs as a job: the picture is
//! prepared once (`tunebox_tags::prepare_cover`), then written into each file atomically. A song
//! that fails is reported and the others carry on. What each song gets depends on where it is:
//!
//! - in a writable place, the cover is written into (or removed from) its file;
//! - in a read-only library folder, the file is never touched; TuneBox keeps the cover itself
//!   (`kept_artwork`, per album) and shows it as the song's. Removing forgets the kept cover; a
//!   cover inside such a file stays, and the listener is told so;
//! - missing, or on a drive or share that is not connected, the song is skipped and the
//!   listener told: touching a disconnected share could hang.

use crate::EngineError;
use crate::file_check::FileStatus;
use crate::jobs::{JobId, JobKind};
use crate::kept_artwork::{AlbumKey, KeptArtwork, album_key};
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use crate::view::LibraryView;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tunebox_model::TrackId;
use tunebox_tags::Artwork;

/// What to do to the covers.
enum Change {
    /// Set this image (an image file's contents, not prepared yet).
    Set(Vec<u8>),
    Remove,
}

/// What a cover job did, for `Session::finish_artwork`.
#[derive(Debug, Default)]
pub(crate) struct ArtworkReport {
    removing: bool,
    /// The image could not be used, so nothing was changed.
    unusable_picture: Option<String>,
    /// Songs whose files were changed, with where they were and their new size.
    written: Vec<(TrackId, PathBuf, Option<u64>)>,
    failures: Vec<(PathBuf, String)>,
    /// Read-only songs whose cover TuneBox now keeps (or no longer keeps).
    kept: usize,
    kept_failure: Option<String>,
    /// Read-only songs without an album name: kept covers are per album, so they have none.
    without_album: usize,
    unavailable: usize,
    missing: usize,
    /// Every album whose cover may look different now.
    albums: BTreeSet<AlbumKey>,
}

impl Session {
    /// Sets the front cover of `ids` to the image in `image_file`, in the background. JPEG and
    /// PNG are used as they are; other images are converted, and large ones scaled down.
    /// Fails at once if the file cannot be read or is not an image.
    pub fn set_artwork(&mut self, ids: &[TrackId], image_file: &Path) -> Result<JobId, EngineError> {
        let data = std::fs::read(image_file)
            .map_err(|e| EngineError::Artwork(format!("{} could not be read: {e}", image_file.display())))?;
        if !tunebox_tags::looks_like_image(&data) {
            return Err(EngineError::Artwork(format!(
                "{}: {}",
                image_file.display(),
                tunebox_tags::PictureError::NotAnImage
            )));
        }
        self.start_artwork(ids, Change::Set(data))
    }

    /// Removes the covers of `ids`, in the background.
    pub fn remove_artwork(&mut self, ids: &[TrackId]) -> Result<JobId, EngineError> {
        self.start_artwork(ids, Change::Remove)
    }

    /// Where the cover of `id` is read from (see [`crate::CoverSource`]).
    pub fn cover_source(&self, id: TrackId) -> Option<crate::CoverSource> {
        self.view().cover_source(id)
    }

    /// The albums whose covers changed since the last call, so a frontend can show them anew.
    pub fn take_artwork_changes(&mut self) -> Vec<AlbumKey> {
        std::mem::take(&mut self.artwork_changes)
    }

    fn start_artwork(&mut self, ids: &[TrackId], change: Change) -> Result<JobId, EngineError> {
        if self.jobs.is_running(JobKind::Artwork) {
            return Err(EngineError::Busy(JobKind::Artwork.noun()));
        }
        let mut report = ArtworkReport { removing: matches!(change, Change::Remove), ..ArtworkReport::default() };
        let mut files = Vec::new();
        let mut kept_albums: BTreeSet<AlbumKey> = BTreeSet::new();
        for track in ids.iter().filter_map(|id| self.library.track(*id)) {
            let album = album_key(track);
            match self.file_status(track.id) {
                FileStatus::Unavailable(_) => report.unavailable += 1,
                FileStatus::Missing => report.missing += 1,
                FileStatus::Present if crate::files::may_write(&self.prefs, &track.location) => {
                    files.push((track.id, track.location.clone()));
                    report.albums.insert(album);
                }
                FileStatus::Present if track.meta.album.trim().is_empty() => report.without_album += 1,
                FileStatus::Present => {
                    report.kept += 1;
                    kept_albums.insert(album.clone());
                    report.albums.insert(album);
                }
            }
        }
        let kept = KeptArtwork::beside(&self.paths.library_file);
        let songs = files.len() + report.kept;
        let title = match (&change, songs) {
            (Change::Set(_), 1) => "Setting the cover of 1 song".to_owned(),
            (Change::Set(_), n) => format!("Setting the cover of {n} songs"),
            (Change::Remove, 1) => "Removing the cover of 1 song".to_owned(),
            (Change::Remove, n) => format!("Removing the covers of {n} songs"),
        };
        Ok(self.jobs.spawn(JobKind::Artwork, title, move |ctx| {
            ctx.progress(0.0, "Preparing the picture…");
            let cover = match change {
                Change::Set(data) => match tunebox_tags::prepare_cover(&data) {
                    Ok(cover) => Some(cover),
                    Err(e) => {
                        report.unusable_picture = Some(e.to_string());
                        report.albums.clear();
                        return Outcome::ArtworkChanged(report);
                    }
                },
                Change::Remove => None,
            };
            for album in &kept_albums {
                let result = match &cover {
                    Some(cover) => kept.put(album, cover),
                    None => kept.remove(album),
                };
                if let Err(e) = result {
                    report.kept_failure = Some(e.to_string());
                }
            }
            if report.kept_failure.is_some() {
                report.kept = 0;
            }
            let total = files.len();
            for (done, (id, location)) in files.into_iter().enumerate() {
                if ctx.cancel_flag().load(Ordering::Relaxed) {
                    break;
                }
                let name = location.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                ctx.progress(done as f32 / total.max(1) as f32, name);
                let result = match &cover {
                    Some(cover) => tunebox_tags::write_artwork(&location, cover),
                    None => tunebox_tags::remove_artwork(&location),
                };
                match result {
                    Ok(()) => {
                        // A new cover changes the file's size; the Size column should say so.
                        let size = std::fs::metadata(&location).ok().map(|m| m.len());
                        report.written.push((id, location, size));
                    }
                    Err(e) => report.failures.push((location, e.to_string())),
                }
            }
            Outcome::ArtworkChanged(report)
        }))
    }

    pub(crate) fn finish_artwork(&mut self, report: ArtworkReport) {
        if let Some(reason) = report.unusable_picture {
            self.notices.push(Notice::error(format!("The cover was not changed: {reason}.")));
            return;
        }
        let mut sizes_changed = false;
        for (id, location, size) in &report.written {
            let still_there = self.library.track(*id).is_some_and(|t| t.location == *location);
            if let (true, Some(size)) = (still_there, size) {
                sizes_changed |= self.library.set_file_size(*id, *size).is_ok();
            }
        }
        if sizes_changed {
            self.library_changed();
        }
        self.artwork_changes.extend(report.albums.iter().cloned());
        for text in summary(&report) {
            self.notices.push(Notice::info(text));
        }
        if let Some(reason) = &report.kept_failure {
            self.notices.push(Notice::warning(format!(
                "TuneBox could not keep the cover for the songs in read-only library folders: {reason}"
            )));
        }
        if let Some((path, reason)) = report.failures.first() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let text = match report.failures.len() {
                1 => format!("The cover of “{name}” could not be changed, so it was left as it is: {reason}"),
                n => format!(
                    "The covers of {n} songs could not be changed and were left as they are. The first, “{name}”: {reason}"
                ),
            };
            self.notices.push(Notice::warning(text));
        }
    }
}

impl LibraryView<'_> {
    /// Saves the cover of `id` to the file `out`, as it is (JPEG or PNG; [`Artwork::extension`]
    /// says which). Returns the cover, or `None` when the song has none and nothing was saved.
    /// `out` is the listener's, so a read-only library folder refuses it.
    pub fn export_cover(&self, id: TrackId, out: &Path) -> Result<Option<Artwork>, EngineError> {
        if !crate::files::may_write(self.prefs, out) {
            return Err(EngineError::Protected(out.to_path_buf()));
        }
        let source = self.cover_source(id).ok_or(tunebox_library::LibraryError::NoSuchTrack(id))?;
        let Some(cover) = source.load().map_err(EngineError::Artwork)? else { return Ok(None) };
        craft_atomic::atomic_write(out, &cover.data)
            .map_err(|e| EngineError::Artwork(format!("{} could not be saved: {e}", out.display())))?;
        Ok(Some(cover))
    }
}

/// The notices after a cover job, one sentence each: what was changed, and what was left.
fn summary(report: &ArtworkReport) -> Vec<String> {
    let songs = |n: usize| if n == 1 { "1 song".to_owned() } else { format!("{n} songs") };
    let mut lines = Vec::new();
    match (report.written.len(), report.removing) {
        (0, _) => {}
        (n, false) => lines.push(format!("Set the cover of {}.", songs(n))),
        (n, true) => lines.push(format!("Removed the cover of {}.", songs(n))),
    }
    match (report.kept, report.removing) {
        (0, _) => {}
        (1, false) => lines.push(
            "1 song is in a read-only library folder: TuneBox keeps its cover itself and leaves the file as it is."
                .to_owned(),
        ),
        (n, false) => lines.push(format!(
            "{n} songs are in a read-only library folder: TuneBox keeps their cover itself and leaves the files as they are."
        )),
        (n, true) => lines.push(format!(
            "{} in a read-only library folder no longer {} a cover kept by TuneBox. A cover inside {} stays, since TuneBox does not change {}.",
            songs(n),
            if n == 1 { "has" } else { "have" },
            if n == 1 { "its file" } else { "their files" },
            if n == 1 { "that file" } else { "those files" },
        )),
    }
    let left = [
        (
            report.without_album,
            "it is in a read-only library folder and has no album name, which TuneBox keeps covers by",
            "they are in a read-only library folder and have no album name, which TuneBox keeps covers by",
        ),
        (report.missing, "its file is missing", "their files are missing"),
        (
            report.unavailable,
            "the drive or share it is on is not connected",
            "the drive or share they are on is not connected",
        ),
    ];
    for (count, one, many) in left {
        match count {
            0 => {}
            1 => lines.push(format!("1 song was left as it is: {one}.")),
            n => lines.push(format!("{n} songs were left as they are: {many}.")),
        }
    }
    if lines.is_empty() && report.failures.is_empty() && report.kept_failure.is_none() {
        lines.push("No covers were changed.".to_owned());
    }
    lines
}
