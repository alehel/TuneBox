//! The column browser: Genres, Artists and Albums above the track list, each narrowing the next.

use std::collections::BTreeSet;
use tunebox_model::Track;

/// What is selected in the browser. `None` is the "All" row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrowseFilter {
    pub genre: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

impl BrowseFilter {
    pub fn is_empty(&self) -> bool {
        self.genre.is_none() && self.artist.is_none() && self.album.is_none()
    }

    pub fn matches(&self, track: &Track) -> bool {
        let m = &track.meta;
        self.genre.as_deref().is_none_or(|g| m.genre == g)
            && self.artist.as_deref().is_none_or(|a| m.filing_artist() == a)
            && self.album.as_deref().is_none_or(|a| m.album == a)
    }
}

/// The three browser columns' contents, sorted, without the "All" rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Browser {
    pub genres: Vec<String>,
    pub artists: Vec<String>,
    pub albums: Vec<String>,
}

impl Browser {
    /// Builds the browser for `tracks`: artists are those in the selected genre, albums those of
    /// the selected genre and artist. Blank values are left out.
    pub fn build(tracks: &[&Track], filter: &BrowseFilter) -> Self {
        let collect = |pick: &dyn Fn(&Track) -> Option<&str>| -> Vec<String> {
            let set: BTreeSet<(String, &str)> = tracks
                .iter()
                .filter_map(|t| pick(t))
                .filter(|s| !s.is_empty())
                .map(|s| (s.to_lowercase(), s))
                .collect();
            let mut values: Vec<String> = set.into_iter().map(|(_, s)| s.to_owned()).collect();
            values.dedup();
            values
        };
        let genre_ok = |t: &Track| filter.genre.as_deref().is_none_or(|g| t.meta.genre == g);
        let artist_ok = |t: &Track| filter.artist.as_deref().is_none_or(|a| t.meta.filing_artist() == a);
        Self {
            genres: collect(&|t| Some(t.meta.genre.as_str())),
            artists: collect(&|t| genre_ok(t).then_some(t.meta.filing_artist())),
            albums: collect(&|t| (genre_ok(t) && artist_ok(t)).then_some(t.meta.album.as_str())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Library;
    use crate::library::tests::new_track;

    #[test]
    fn browser_narrows_by_genre_then_artist() {
        let mut lib = Library::new();
        let mut jazz = new_track("/1.mp3", "a", "Miles", "Blue");
        jazz.meta.genre = "Jazz".into();
        lib.add_track(jazz);
        lib.add_track(new_track("/2.mp3", "b", "Ann", "Pop One"));
        lib.add_track(new_track("/3.mp3", "c", "Ann", "Pop Two"));
        lib.add_track(new_track("/4.mp3", "d", "bob", "Pop Three"));

        let tracks: Vec<_> = lib.tracks().collect();
        let all = Browser::build(&tracks, &BrowseFilter::default());
        assert_eq!(all.genres, ["Jazz", "Pop"]);
        assert_eq!(all.artists, ["Ann", "bob", "Miles"]);

        let filter = BrowseFilter { genre: Some("Pop".into()), artist: Some("Ann".into()), album: None };
        let pop = Browser::build(&tracks, &filter);
        assert_eq!(pop.artists, ["Ann", "bob"]);
        assert_eq!(pop.albums, ["Pop One", "Pop Two"]);
        assert_eq!(lib.tracks().filter(|t| filter.matches(t)).count(), 2);
    }
}
