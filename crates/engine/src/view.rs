//! Reading the library: a source's tracks, searched, browsed and sorted, and their totals.
//! [`LibraryView`] borrows what it reads, so the running session (`Session::view`) and a
//! read-only snapshot (`LibrarySnapshot::view`) answer the same questions the same way.

use crate::devices::DeviceEntry;
use crate::kept_artwork::{CoverSource, KeptArtwork};
use crate::prefs::Preferences;
use crate::sources::{Source, TrackQuery};
use chrono::Utc;
use tunebox_library::{Browser, Library, Summary};
use tunebox_model::search::Query;
use tunebox_model::{Column, MediaKind, SortOrder, Track, TrackId, compare_tracks};

/// A read-only look at a library. It cannot change anything.
#[derive(Clone, Copy)]
pub struct LibraryView<'a> {
    pub(crate) library: &'a Library,
    pub(crate) prefs: &'a Preferences,
    /// Connected players, for `Source::Device`. A snapshot has none.
    pub(crate) devices: &'a [DeviceEntry],
    /// Where the library is kept; TuneBox's own covers are kept beside it.
    pub(crate) library_file: &'a std::path::Path,
}

impl<'a> LibraryView<'a> {
    pub fn library(&self) -> &'a Library {
        self.library
    }

    pub fn preferences(&self) -> &'a Preferences {
        self.prefs
    }

    pub fn track(&self, id: TrackId) -> Option<&'a Track> {
        self.library.track(id)
    }

    /// A source's tracks in its own order. Playlists may list a track more than once.
    pub fn source_tracks(&self, source: &Source) -> Vec<TrackId> {
        match source {
            Source::Music => self.in_album_order(self.of_kind(Some(MediaKind::Music))),
            Source::Audiobooks => self.in_album_order(self.of_kind(Some(MediaKind::Audiobook))),
            Source::Library => self.in_album_order(self.of_kind(None)),
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

    /// The tracks of one media kind, or all of them.
    fn of_kind(&self, kind: Option<MediaKind>) -> Vec<TrackId> {
        self.library.tracks().filter(|t| kind.is_none_or(|k| t.kind == k)).map(|t| t.id).collect()
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

    /// Where the cover of `id` is read from: TuneBox's kept cover for a song in a read-only
    /// library folder that has one, else the song's file (see `kept_artwork`).
    pub fn cover_source(&self, id: TrackId) -> Option<CoverSource> {
        let track = self.library.track(id)?;
        Some(KeptArtwork::beside(self.library_file).source(self.prefs, track))
    }

    /// Totals for the status bar.
    pub fn summary(&self, ids: &[TrackId]) -> Summary {
        Summary::of(ids.iter().filter_map(|id| self.library.track(*id)))
    }
}
