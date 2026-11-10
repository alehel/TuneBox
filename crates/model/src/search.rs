//! The search field: every word typed must appear in one of the track's text fields.

use crate::Track;

/// Which fields the search field looks in (the magnifier menu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchScope {
    #[default]
    All,
    Artists,
    Albums,
    Composers,
    Songs,
}

/// A parsed query: lowercase words, all of which must match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    words: Vec<String>,
    scope: SearchScope,
}

impl Query {
    pub fn new(text: &str, scope: SearchScope) -> Self {
        Self { words: text.split_whitespace().map(str::to_lowercase).collect(), scope }
    }

    /// An empty query matches everything.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn matches(&self, track: &Track) -> bool {
        if self.words.is_empty() {
            return true;
        }
        let haystack = self.haystack(track);
        self.words.iter().all(|word| haystack.contains(word.as_str()))
    }

    /// The searched fields joined into one lowercase string.
    fn haystack(&self, track: &Track) -> String {
        let m = &track.meta;
        let fields: Vec<&str> = match self.scope {
            SearchScope::All => vec![track.title(), &m.artist, &m.album_artist, &m.album, &m.genre, &m.composer],
            SearchScope::Artists => vec![&m.artist, &m.album_artist],
            SearchScope::Albums => vec![&m.album],
            SearchScope::Composers => vec![&m.composer],
            SearchScope::Songs => vec![track.title()],
        };
        fields.join("\u{1f}").to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::fixtures::track;

    #[test]
    fn every_word_must_match_some_field() {
        let t = track(1, "Harbour Lights", "The Lanterns", "Night Ferry");
        assert!(Query::new("lanterns ferry", SearchScope::All).matches(&t));
        assert!(Query::new("HARBOUR", SearchScope::All).matches(&t));
        assert!(!Query::new("lanterns tram", SearchScope::All).matches(&t));
    }

    #[test]
    fn scope_limits_fields() {
        let t = track(1, "Harbour Lights", "The Lanterns", "Night Ferry");
        assert!(!Query::new("ferry", SearchScope::Artists).matches(&t));
        assert!(Query::new("ferry", SearchScope::Albums).matches(&t));
    }

    #[test]
    fn empty_query_matches_all() {
        assert!(Query::new("   ", SearchScope::All).matches(&track(1, "a", "b", "c")));
    }
}
