//! What the track list shows: a source, and the search, browse and sort applied to it. The
//! reading itself is in `view`.

use crate::session::Session;
use crate::view::LibraryView;
use std::path::PathBuf;
use tunebox_library::{BrowseFilter, Browser, Summary};
use tunebox_model::search::SearchScope;
use tunebox_model::{Column, PlaylistId, SortOrder, TrackId};

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
    /// A read-only look at the library, as the track list and the command line read it.
    pub fn view(&self) -> LibraryView<'_> {
        LibraryView { library: &self.library, prefs: &self.prefs, devices: &self.devices }
    }

    /// A source's tracks in its own order (see [`LibraryView::source_tracks`]).
    pub fn source_tracks(&self, source: &Source) -> Vec<TrackId> {
        self.view().source_tracks(source)
    }

    /// The rows of the track list (see [`LibraryView::query`]).
    pub fn query(&self, query: &TrackQuery) -> Vec<TrackId> {
        self.view().query(query)
    }

    /// The column browser for a query (see [`LibraryView::browser`]).
    pub fn browser(&self, query: &TrackQuery) -> Browser {
        self.view().browser(query)
    }

    /// Totals for the status bar.
    pub fn summary(&self, ids: &[TrackId]) -> Summary {
        self.view().summary(ids)
    }
}
