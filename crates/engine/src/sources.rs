//! What the track list shows: the tracks of a source, searched, browsed and sorted.

use crate::session::Session;
use chrono::Utc;
use std::path::PathBuf;
use tunebox_library::{BrowseFilter, Browser, Summary};
use tunebox_model::search::{Query, SearchScope};
use tunebox_model::{Column, PlaylistId, SortOrder, TrackId, compare_tracks};

/// An entry in the source list whose contents are library tracks. (The CD has its own view,
/// since its tracks are not in the library until imported.)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// The whole music library.
    Music,
    Playlist(PlaylistId),
    /// The tracks TuneBox has synced to the player mounted at this path.
    Device(PathBuf),
}

/// Everything that decides the rows of the track list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackQuery {
    pub source: Source,
    pub search: String,
    pub scope: SearchScope,
    pub browse: BrowseFilter,
    /// `None` keeps the source's own order (playlist order, or album order for the library).
    pub sort: Option<(Column, SortOrder)>,
}

impl TrackQuery {
    pub fn new(source: Source) -> Self {
        Self { source, search: String::new(), scope: SearchScope::All, browse: BrowseFilter::default(), sort: None }
    }
}

impl Session {
    /// A source's tracks in its own order. Playlists may list a track more than once.
    pub fn source_tracks(&self, source: &Source) -> Vec<TrackId> {
        match source {
            Source::Music => self.in_album_order(self.library.tracks().map(|t| t.id).collect()),
            Source::Playlist(id) => self.library.playlist_tracks(*id, Utc::now()).unwrap_or_default(),
            Source::Device(root) => {
                let on_device = self
                    .devices
                    .iter()
                    .find(|d| d.volume.root == *root)
                    .and_then(|d| d.device.as_ref())
                    .map(|d| d.manifest.files.keys().copied().filter(|id| self.library.track(*id).is_some()).collect())
                    .unwrap_or_default();
                self.in_album_order(on_device)
            }
        }
    }

    fn in_album_order(&self, mut ids: Vec<TrackId>) -> Vec<TrackId> {
        self.sort_ids(&mut ids, Column::Artist, SortOrder::Ascending);
        ids
    }

    fn sort_ids(&self, ids: &mut [TrackId], column: Column, order: SortOrder) {
        ids.sort_by(|a, b| match (self.library.track(*a), self.library.track(*b)) {
            (Some(ta), Some(tb)) => compare_tracks(ta, tb, column, order),
            _ => a.cmp(b),
        });
    }

    /// The rows of the track list.
    pub fn query(&self, query: &TrackQuery) -> Vec<TrackId> {
        let search = Query::new(&query.search, query.scope);
        let mut ids: Vec<TrackId> = self
            .source_tracks(&query.source)
            .into_iter()
            .filter(|id| self.library.track(*id).is_some_and(|t| search.matches(t) && query.browse.matches(t)))
            .collect();
        if let Some((column, order)) = query.sort {
            self.sort_ids(&mut ids, column, order);
        }
        ids
    }

    /// The column browser for a query: built from the searched source, before the browser's own
    /// selection narrows it.
    pub fn browser(&self, query: &TrackQuery) -> Browser {
        let search = Query::new(&query.search, query.scope);
        let ids = self.source_tracks(&query.source);
        let tracks: Vec<_> =
            ids.iter().filter_map(|id| self.library.track(*id)).filter(|t| search.matches(t)).collect();
        Browser::build(&tracks, &query.browse)
    }

    /// Totals for the status bar.
    pub fn summary(&self, ids: &[TrackId]) -> Summary {
        Summary::of(ids.iter().filter_map(|id| self.library.track(*id)))
    }
}
