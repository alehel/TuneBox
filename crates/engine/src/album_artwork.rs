//! "Find Album Artwork…": covers for albums in the library, from MusicBrainz and the Cover
//! Art Archive.
//!
//! 1. [`Session::find_album_artwork`] searches MusicBrainz for releases (editions) matching
//!    an artist and album title. Nothing is fetched before the listener asks.
//! 2. The releases found become [`CoverCandidate`]s. A frontend that shows them asks for their
//!    250-pixel thumbnails; the session then fetches them one at a time in the background
//!    (each the release's own front cover, or its release group's when it has none).
//! 3. The listener chooses one; [`Session::use_found_artwork`] fetches that release's front
//!    cover (see `tunebox_metabrainz::cover_art` for the size) and hands it to the usual
//!    artwork job, which writes it into the songs (or keeps it, for read-only folders).
//!    Nothing is applied without that choice.

use crate::EngineError;
use crate::jobs::{JobId, JobKind};
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use std::sync::Arc;
use tunebox_metabrainz::cover_art::{fetch_front_cover, fetch_thumbnail};
use tunebox_metabrainz::search::search_releases;
use tunebox_metabrainz::{CoverOf, ReleaseCandidate, ReleaseRef};
use tunebox_model::TrackId;

/// An open search for an album's artwork.
#[derive(Clone, Debug)]
pub struct ArtworkSearch {
    /// The songs the chosen cover is for.
    pub tracks: Vec<TrackId>,
    pub artist: String,
    pub album: String,
    pub state: SearchState,
    /// Whether the releases' thumbnails are wanted (a picker shows them; the CLI does not).
    thumbnails: bool,
    /// Tells this search's replies from those of a search it replaced.
    generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchState {
    Searching,
    /// The releases found, best match first (empty: MusicBrainz knows none).
    Found(Vec<CoverCandidate>),
    Failed(String),
}

/// A release the cover could come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverCandidate {
    pub release: ReleaseCandidate,
    pub thumbnail: Thumbnail,
}

impl CoverCandidate {
    pub fn release_ref(&self) -> ReleaseRef {
        ReleaseRef { release: self.release.id.clone(), release_group: self.release.release_group_id.clone() }
    }
}

/// A candidate's thumbnail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Thumbnail {
    #[default]
    NotFetched,
    Fetching,
    /// The picture as served (JPEG or PNG), and whether it is the release's own.
    Found {
        data: Arc<Vec<u8>>,
        from: CoverOf,
    },
    /// The archive has no cover for this release or its release group.
    NoneFound,
    Failed(String),
}

pub(crate) struct SearchReply {
    generation: u64,
    result: Result<Vec<ReleaseCandidate>, String>,
}

pub(crate) struct ThumbnailReply {
    generation: u64,
    release_id: String,
    result: Result<Option<(Vec<u8>, CoverOf)>, String>,
}

pub(crate) struct ReleaseCoverReply {
    tracks: Vec<TrackId>,
    title: String,
    result: Result<Option<(Vec<u8>, CoverOf)>, String>,
}

impl Session {
    /// The artist and album to search for, from `ids`: the album of the first song that has
    /// one, and its album artist (or artist). A compilation is searched by title alone.
    pub fn album_search_terms(&self, ids: &[TrackId]) -> (String, String) {
        let tracks: Vec<_> = ids.iter().filter_map(|id| self.library.track(*id)).collect();
        let Some(track) = tracks.iter().find(|t| !t.meta.album.trim().is_empty()).or(tracks.first()) else {
            return (String::new(), String::new());
        };
        let meta = &track.meta;
        let artist = match (meta.compilation, meta.album_artist.trim()) {
            (true, _) => "",
            (false, "") => meta.artist.trim(),
            (false, album_artist) => album_artist,
        };
        (artist.to_owned(), meta.album.trim().to_owned())
    }

    /// Searches MusicBrainz for releases of `album` by `artist`, for the cover of `ids`, in the
    /// background. With `thumbnails`, each release's thumbnail is then fetched too. Replaces
    /// any search open before.
    pub fn find_album_artwork(
        &mut self,
        ids: &[TrackId],
        artist: &str,
        album: &str,
        thumbnails: bool,
    ) -> Result<JobId, EngineError> {
        if album.trim().is_empty() {
            return Err(EngineError::Artwork(
                "There is no album name to search for. Type the album's title in.".into(),
            ));
        }
        if self.jobs.is_running(JobKind::FindArtwork) {
            return Err(EngineError::Busy(JobKind::FindArtwork.noun()));
        }
        let generation = self.artwork_search.as_ref().map_or(1, |s| s.generation.wrapping_add(1));
        self.artwork_search = Some(ArtworkSearch {
            tracks: ids.to_vec(),
            artist: artist.trim().to_owned(),
            album: album.trim().to_owned(),
            state: SearchState::Searching,
            thumbnails,
            generation,
        });
        let (artist, album) = (artist.trim().to_owned(), album.trim().to_owned());
        let web = Arc::clone(&self.web);
        let title = format!("Looking for the artwork of “{album}”");
        Ok(self.jobs.spawn(JobKind::FindArtwork, title, move |ctx| {
            ctx.progress(0.0, "Searching MusicBrainz…");
            let result = search_releases(web.as_ref(), &artist, &album).map_err(|e| e.to_string());
            Outcome::ReleasesFound(SearchReply { generation, result })
        }))
    }

    /// The open artwork search, if any.
    pub fn artwork_search(&self) -> Option<&ArtworkSearch> {
        self.artwork_search.as_ref()
    }

    /// Closes the artwork search; thumbnails not yet fetched are not.
    pub fn close_artwork_search(&mut self) {
        self.artwork_search = None;
    }

    /// Fetches the front cover of `release_id`, one of the open search's releases, and makes it
    /// the cover of the search's songs.
    pub fn use_found_artwork(&mut self, release_id: &str) -> Result<JobId, EngineError> {
        let search = self.artwork_search.as_ref().ok_or_else(|| {
            EngineError::Artwork("There is no artwork search open. Find Album Artwork… first.".into())
        })?;
        let SearchState::Found(candidates) = &search.state else {
            return Err(EngineError::Artwork("The search has not found any releases yet.".into()));
        };
        let candidate = candidates
            .iter()
            .find(|c| c.release.id == release_id)
            .ok_or_else(|| EngineError::Artwork(format!("“{release_id}” is not one of the releases found.")))?;
        let (release, tracks) = (candidate.release_ref(), search.tracks.clone());
        self.use_release_artwork(&tracks, &release)
    }

    /// Fetches the front cover of `release` (or of its release group, when the release has
    /// none) and makes it the cover of `ids`, in the background. For a release chosen
    /// elsewhere, such as on the command line.
    pub fn use_release_artwork(&mut self, ids: &[TrackId], release: &ReleaseRef) -> Result<JobId, EngineError> {
        if !tunebox_metabrainz::is_mbid(&release.release) {
            return Err(EngineError::Artwork(format!(
                "“{}” is not a MusicBrainz release id (they look like 6a09041b-0f79-3278-88d9-0552dd28d37f).",
                release.release
            )));
        }
        if ids.is_empty() {
            return Err(EngineError::Artwork("No songs were chosen for the cover.".into()));
        }
        if let Some(kind) = [JobKind::FetchCover, JobKind::Artwork].into_iter().find(|k| self.jobs.is_running(*k)) {
            return Err(EngineError::Busy(kind.noun()));
        }
        let title = self
            .artwork_search
            .as_ref()
            .and_then(|s| match &s.state {
                SearchState::Found(found) => found.iter().find(|c| c.release.id == release.release),
                _ => None,
            })
            .map(|c| c.release.title.clone())
            .unwrap_or_else(|| self.album_search_terms(ids).1);
        let (web, release, tracks) = (Arc::clone(&self.web), release.clone(), ids.to_vec());
        Ok(self.jobs.spawn(JobKind::FetchCover, format!("Getting the cover of “{title}”"), move |ctx| {
            ctx.progress(0.0, "Contacting the Cover Art Archive…");
            let result = fetch_front_cover(web.as_ref(), &release)
                .map(|found| found.map(|cover| (cover.data, cover.from)))
                .map_err(|e| e.to_string());
            Outcome::ReleaseCoverFetched(ReleaseCoverReply { tracks, title, result })
        }))
    }

    pub(crate) fn finish_release_search(&mut self, reply: SearchReply) {
        let Some(search) = self.artwork_search.as_mut().filter(|s| s.generation == reply.generation) else { return };
        search.state = match reply.result {
            Ok(found) => SearchState::Found(
                found.into_iter().map(|release| CoverCandidate { release, thumbnail: Thumbnail::NotFetched }).collect(),
            ),
            Err(message) => SearchState::Failed(format!("Could not search MusicBrainz: {message}.")),
        };
    }

    /// Starts fetching the next thumbnail the open search wants, one at a time so the archive
    /// is not asked for ten pictures at once. Called every tick.
    pub(crate) fn fetch_next_thumbnail(&mut self) {
        if self.jobs.is_running(JobKind::Thumbnail) {
            return;
        }
        let Some(search) = self.artwork_search.as_mut().filter(|s| s.thumbnails) else { return };
        let SearchState::Found(candidates) = &mut search.state else { return };
        let Some(next) = candidates.iter_mut().find(|c| c.thumbnail == Thumbnail::NotFetched) else { return };
        next.thumbnail = Thumbnail::Fetching;
        let (release, generation) = (next.release_ref(), search.generation);
        let web = Arc::clone(&self.web);
        self.jobs.spawn(JobKind::Thumbnail, "Getting cover thumbnails".into(), move |_| {
            let result = fetch_thumbnail(web.as_ref(), &release)
                .map(|found| found.map(|cover| (cover.data, cover.from)))
                .map_err(|e| e.to_string());
            Outcome::ThumbnailFetched(ThumbnailReply { generation, release_id: release.release, result })
        });
    }

    pub(crate) fn finish_thumbnail(&mut self, reply: ThumbnailReply) {
        let Some(search) = self.artwork_search.as_mut().filter(|s| s.generation == reply.generation) else { return };
        let SearchState::Found(candidates) = &mut search.state else { return };
        let Some(candidate) = candidates.iter_mut().find(|c| c.release.id == reply.release_id) else { return };
        candidate.thumbnail = match reply.result {
            Ok(Some((data, from))) if tunebox_tags::looks_like_image(&data) => {
                Thumbnail::Found { data: Arc::new(data), from }
            }
            Ok(Some(_)) => Thumbnail::Failed("the archive sent something that is not a picture".into()),
            Ok(None) => Thumbnail::NoneFound,
            Err(message) => Thumbnail::Failed(message),
        };
    }

    pub(crate) fn finish_release_cover(&mut self, reply: ReleaseCoverReply) {
        let title = reply.title;
        match reply.result {
            Ok(Some((data, from))) => {
                if !tunebox_tags::looks_like_image(&data) {
                    self.notices.push(Notice::warning(format!(
                        "The Cover Art Archive sent something that is not a picture for “{title}”, so no cover was changed."
                    )));
                    return;
                }
                if from == CoverOf::ReleaseGroup {
                    self.notices.push(Notice::info(format!(
                        "This edition of “{title}” has no cover of its own in the Cover Art Archive, so TuneBox uses the album's cover from another edition."
                    )));
                }
                if let Err(e) = self.set_artwork_data(&reply.tracks, data) {
                    self.notices.push(Notice::warning(format!("The cover of “{title}” was not changed: {e}")));
                }
            }
            Ok(None) => self.notices.push(Notice::info(format!(
                "The Cover Art Archive has no cover for “{title}”. Choose another release, or choose a picture yourself."
            ))),
            Err(message) => {
                self.notices.push(Notice::warning(format!("Could not get the cover of “{title}”: {message}.")))
            }
        }
    }

    /// A search or thumbnail job that stopped unexpectedly leaves nothing waiting for it.
    pub(crate) fn forget_failed_search_job(&mut self, kind: JobKind, message: &str) {
        let Some(search) = self.artwork_search.as_mut() else { return };
        match (&mut search.state, kind) {
            (state @ SearchState::Searching, JobKind::FindArtwork) => {
                *state = SearchState::Failed(format!("Could not search MusicBrainz: {message}"));
            }
            (SearchState::Found(candidates), JobKind::Thumbnail) => {
                for candidate in candidates.iter_mut().filter(|c| c.thumbnail == Thumbnail::Fetching) {
                    candidate.thumbnail = Thumbnail::Failed(message.to_owned());
                }
            }
            _ => {}
        }
    }
}
