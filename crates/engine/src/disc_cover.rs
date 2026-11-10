//! The CD's cover, from the Cover Art Archive.
//!
//! Once a disc's names have come from MusicBrainz, TuneBox knows the release they belong to,
//! and the archive may have its front cover. It is fetched when the listener asks ("Get
//! Cover"), or by itself after a lookup if the listener turned that on in Preferences (off by
//! default). The cover is shown in the CD view before importing, and goes into the imported
//! songs through the usual artwork job (`artwork`): into the songs being imported when it
//! arrives first, or into those already imported when it arrives later.

use crate::EngineError;
use crate::jobs::{JobId, JobKind};
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use std::sync::Arc;
use tunebox_metabrainz::cover_art::fetch_front_cover;
use tunebox_metabrainz::{CoverOf, ReleaseRef};
use tunebox_model::TrackId;
use tunebox_tags::Artwork;

/// What TuneBox knows of the disc's cover.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DiscCover {
    /// Not asked for (or the listener chose another release since).
    #[default]
    NotAsked,
    Fetching,
    /// The cover, prepared for songs (JPEG or PNG, at most 1400 pixels a side).
    Found {
        artwork: Artwork,
        from: CoverOf,
    },
    /// The archive has no front cover for this release or its release group.
    NoneFound,
    /// The request failed; the message says why.
    Failed(String),
}

/// What a cover job hands back.
pub(crate) struct DiscCoverReply {
    disc_id: String,
    release_id: String,
    /// The listener asked (rather than the automatic setting): then "no cover" is said aloud.
    asked: bool,
    result: Result<Option<(Artwork, CoverOf)>, String>,
}

impl Session {
    /// Fetches the front cover of the release the disc's names came from ("Get Cover"), in the
    /// background. Fails at once when there is no disc, or its names were not looked up.
    pub fn fetch_disc_cover(&mut self) -> Result<JobId, EngineError> {
        self.start_disc_cover(true)
    }

    /// After a lookup, or choosing another release: fetches the cover if the listener turned
    /// that on in Preferences.
    pub(crate) fn fetch_disc_cover_if_wanted(&mut self) {
        let wanted = self.prefs.fetch_cd_covers_automatically
            && self.cd.as_ref().is_some_and(|cd| cd.info.release_id.is_some() && cd.cover == DiscCover::NotAsked);
        if wanted && let Err(e) = self.start_disc_cover(false) {
            log::info!("the disc's cover was not fetched: {e}");
        }
    }

    fn start_disc_cover(&mut self, asked: bool) -> Result<JobId, EngineError> {
        let cd = self.cd.as_ref().ok_or(EngineError::NoDisc)?;
        let release_id = cd.info.release_id.clone().ok_or(EngineError::NoRelease)?;
        if self.jobs.is_running(JobKind::FetchCover) {
            return Err(EngineError::Busy(JobKind::FetchCover.noun()));
        }
        let release = ReleaseRef { release: release_id.clone(), release_group: cd.info.release_group_id.clone() };
        let disc_id = cd.disc_id.clone();
        let title = format!("Getting the cover of “{}”", cd.info.album);
        if let Some(cd) = self.cd.as_mut() {
            cd.cover = DiscCover::Fetching;
        }
        let web = Arc::clone(&self.web);
        Ok(self.jobs.spawn(JobKind::FetchCover, title, move |ctx| {
            ctx.progress(0.0, "Contacting the Cover Art Archive…");
            let result = fetch_front_cover(web.as_ref(), &release).map_err(|e| e.to_string()).and_then(|found| {
                found
                    .map(|cover| {
                        tunebox_tags::prepare_cover(&cover.data)
                            .map(|artwork| (artwork, cover.from))
                            .map_err(|e| format!("the Cover Art Archive sent a picture TuneBox cannot use: {e}"))
                    })
                    .transpose()
            });
            Outcome::DiscCoverFetched(DiscCoverReply { disc_id, release_id, asked, result })
        }))
    }

    pub(crate) fn finish_disc_cover(&mut self, reply: DiscCoverReply) {
        let Some(cd) = self.cd.as_mut() else { return };
        // The disc was changed, or another release chosen, while the cover was on its way.
        if cd.disc_id != reply.disc_id || cd.info.release_id.as_deref() != Some(reply.release_id.as_str()) {
            return;
        }
        let album = cd.info.album.clone();
        match reply.result {
            Ok(Some((artwork, from))) => {
                cd.cover = DiscCover::Found { artwork, from };
                let imported = cd.imported_songs.clone();
                if from == CoverOf::ReleaseGroup {
                    self.notices.push(Notice::info(format!(
                        "This edition of “{album}” has no cover in the Cover Art Archive, so TuneBox uses the album's cover from another edition."
                    )));
                }
                self.put_disc_cover_into(&imported);
            }
            Ok(None) => {
                cd.cover = DiscCover::NoneFound;
                if reply.asked {
                    self.notices.push(Notice::info(format!("The Cover Art Archive has no cover for “{album}”.")));
                }
            }
            Err(message) => {
                cd.cover = DiscCover::Failed(message.clone());
                self.notices.push(Notice::warning(format!("Could not get the cover of “{album}”: {message}.")));
            }
        }
    }

    /// Puts the disc's cover, if one was found, into `songs` (just imported from it).
    pub(crate) fn put_disc_cover_into(&mut self, songs: &[TrackId]) {
        let Some(DiscCover::Found { artwork, .. }) = self.cd.as_ref().map(|cd| &cd.cover) else { return };
        if songs.is_empty() {
            return;
        }
        let data = artwork.data.clone();
        if let Err(e) = self.set_artwork_data(songs, data) {
            self.notices.push(Notice::warning(format!(
                "The cover was not put into the imported songs ({e}). Use Find Album Artwork… on them later."
            )));
        }
    }

    /// A cover job that stopped unexpectedly leaves nothing waiting for it.
    pub(crate) fn forget_failed_cover_job(&mut self, kind: JobKind, message: &str) {
        match kind {
            JobKind::FetchCover => {
                if let Some(cd) = self.cd.as_mut().filter(|cd| cd.cover == crate::DiscCover::Fetching) {
                    cd.cover = DiscCover::Failed(message.to_owned());
                }
            }
            JobKind::FindArtwork | JobKind::Thumbnail => self.forget_failed_search_job(kind, message),
            _ => {}
        }
    }
}
