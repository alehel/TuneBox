//! What the window is showing: the selected source, the search, the browser, sorting,
//! selection, and the rows derived from them (cached until anything they depend on changes).

use crate::full_screen::FullScreen;
use std::collections::HashSet;
use std::path::PathBuf;
use tunebox_engine::model::{Column, PlaylistId, SortOrder, TrackId};
use tunebox_engine::{BrowseFilter, Browser, Session, Source, Summary, TrackQuery};

/// An entry of the source list.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SourceItem {
    Music,
    /// The audiobooks in the library.
    Audiobooks,
    /// The audiobook being ripped and edited.
    Book,
    Playlist(PlaylistId),
    Device(PathBuf),
    Cd,
}

impl SourceItem {
    /// The engine source behind this entry. The CD and the book in progress have none: their
    /// tracks are not in the library.
    pub fn source(&self) -> Option<Source> {
        match self {
            SourceItem::Music => Some(Source::Music),
            SourceItem::Audiobooks => Some(Source::Audiobooks),
            SourceItem::Playlist(id) => Some(Source::Playlist(*id)),
            SourceItem::Device(root) => Some(Source::Device(root.clone())),
            SourceItem::Cd | SourceItem::Book => None,
        }
    }
}

/// List or album grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Albums,
}

/// Selected tracks, plus the row a Shift-click extends from.
#[derive(Clone, Debug, Default)]
pub struct Selection {
    ids: HashSet<TrackId>,
    anchor: Option<usize>,
}

impl Selection {
    pub fn contains(&self, id: TrackId) -> bool {
        self.ids.contains(&id)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn clear(&mut self) {
        self.ids.clear();
        self.anchor = None;
    }

    /// A plain click.
    pub fn select_only(&mut self, index: usize, id: TrackId) {
        self.ids.clear();
        self.ids.insert(id);
        self.anchor = Some(index);
    }

    /// ⌘/Ctrl-click.
    pub fn toggle(&mut self, index: usize, id: TrackId) {
        if !self.ids.remove(&id) {
            self.ids.insert(id);
        }
        self.anchor = Some(index);
    }

    /// Shift-click: everything from the anchor to `index`.
    pub fn extend_to(&mut self, index: usize, rows: &[TrackId]) {
        let anchor = self.anchor.unwrap_or(index);
        let (from, to) = (anchor.min(index), anchor.max(index));
        self.ids = rows.get(from..=to).unwrap_or_default().iter().copied().collect();
    }

    pub fn select_all(&mut self, rows: &[TrackId]) {
        self.ids = rows.iter().copied().collect();
    }

    pub fn set(&mut self, ids: impl IntoIterator<Item = TrackId>) {
        self.ids = ids.into_iter().collect();
        self.anchor = None;
    }

    /// The selected tracks in row order.
    pub fn in_order(&self, rows: &[TrackId]) -> Vec<TrackId> {
        let mut seen = HashSet::new();
        rows.iter().copied().filter(|id| self.ids.contains(id) && seen.insert(*id)).collect()
    }

    /// Row positions of the selected tracks (every copy, for playlists that repeat a track).
    pub fn positions(&self, rows: &[TrackId]) -> Vec<usize> {
        rows.iter().enumerate().filter(|(_, id)| self.ids.contains(id)).map(|(i, _)| i).collect()
    }
}

/// What the rows were computed from; rows are recomputed when this changes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RowsKey {
    revision: u64,
    query: TrackQuery,
}

/// The window's view state.
pub struct ViewState {
    pub source: SourceItem,
    pub search: String,
    pub browse: BrowseFilter,
    pub sort: Option<(Column, SortOrder)>,
    pub view_mode: ViewMode,
    pub selection: Selection,
    /// The artwork pane at the bottom of the source list.
    pub show_artwork: bool,
    /// The visualizer fills the middle of the window instead of the source.
    pub show_visualizer: bool,
    /// The visualizer filling the screen, and how the window was before.
    pub full_screen: FullScreen,
    /// Set to move keyboard focus to the search field on the next frame.
    pub focus_search: bool,
    /// The audiobook page: its selection, folded chapters, a name being typed.
    pub book: crate::book_state::BookView,
    key: Option<RowsKey>,
    /// Whether the library holds audiobooks, at the session revision it was counted.
    audiobooks: std::cell::Cell<Option<(u64, bool)>>,
    rows: Vec<TrackId>,
    browser: Browser,
    summary: Summary,
}

impl ViewState {
    pub fn new() -> Self {
        Self {
            source: SourceItem::Music,
            search: String::new(),
            browse: BrowseFilter::default(),
            sort: None,
            view_mode: ViewMode::List,
            selection: Selection::default(),
            show_artwork: true,
            show_visualizer: false,
            full_screen: FullScreen::default(),
            focus_search: false,
            book: crate::book_state::BookView::default(),
            key: None,
            audiobooks: std::cell::Cell::new(None),
            rows: Vec::new(),
            browser: Browser::default(),
            summary: Summary::default(),
        }
    }

    fn query(&self) -> Option<TrackQuery> {
        let mut query = TrackQuery::new(self.source.source()?);
        query.search = self.search.clone();
        query.browse = self.browse.clone();
        query.sort = self.sort;
        Some(query)
    }

    /// Recomputes the rows, browser and summary if anything they depend on changed.
    pub fn refresh(&mut self, session: &Session) {
        let Some(query) = self.query() else {
            self.rows.clear();
            self.key = None;
            return;
        };
        let key = RowsKey { revision: session.revision(), query };
        if self.key.as_ref() == Some(&key) {
            return;
        }
        self.rows = session.query(&key.query);
        self.browser = session.browser(&key.query);
        self.summary = session.summary(&self.rows);
        self.key = Some(key);
    }

    /// Whether the library holds audiobooks (counted again only when the library changed).
    pub fn has_audiobooks(&self, session: &Session) -> bool {
        let revision = session.revision();
        if let Some((counted, has)) = self.audiobooks.get()
            && counted == revision
        {
            return has;
        }
        let has = session.library().tracks().any(|t| t.kind == tunebox_engine::model::MediaKind::Audiobook);
        self.audiobooks.set(Some((revision, has)));
        has
    }

    pub fn rows(&self) -> &[TrackId] {
        &self.rows
    }

    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    pub fn summary(&self) -> Summary {
        self.summary
    }

    /// Shows another source, starting afresh: no browser selection, its own order, nothing
    /// selected.
    pub fn select_source(&mut self, source: SourceItem) {
        if self.source != source {
            self.source = source;
            self.browse = BrowseFilter::default();
            self.sort = None;
            self.selection.clear();
        }
    }

    /// The sort a header click asks for: the column's natural direction first, then flipped.
    pub fn sort_by(&mut self, column: Column) {
        self.sort = match self.sort {
            Some((current, order)) if current == column => Some((column, order.flipped())),
            _ => Some((column, column.natural_order())),
        };
    }

    pub fn selected_tracks(&self) -> Vec<TrackId> {
        self.selection.in_order(&self.rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_click_selects_a_range_from_the_anchor() {
        let rows: Vec<TrackId> = (1..=6).map(TrackId).collect();
        let mut selection = Selection::default();
        selection.select_only(1, rows[1]);
        selection.extend_to(4, &rows);
        assert_eq!(selection.in_order(&rows), &rows[1..=4]);
        selection.toggle(2, rows[2]);
        assert_eq!(selection.positions(&rows), [1, 3, 4]);
    }

    #[test]
    fn header_clicks_flip_the_order() {
        let mut view = ViewState::new();
        view.sort_by(Column::Plays);
        assert_eq!(view.sort, Some((Column::Plays, SortOrder::Descending)));
        view.sort_by(Column::Plays);
        assert_eq!(view.sort, Some((Column::Plays, SortOrder::Ascending)));
        view.sort_by(Column::Name);
        assert_eq!(view.sort, Some((Column::Name, SortOrder::Ascending)));
    }
}
