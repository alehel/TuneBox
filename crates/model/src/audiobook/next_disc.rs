//! What a newly inserted disc is to the book: the next one, one already in it, or another.
//!
//! When the book's details came from MusicBrainz, the release lists every disc id of every
//! medium, so a disc's place in the set is known offline, without asking again.

use super::book::{Book, TrackRef};

/// What the inserted disc is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscCheck {
    /// The next disc of the book. `known` when MusicBrainz confirms its place in the set.
    Next { known: bool },
    /// A disc already in the book, as disc `disc`. `missing` are its tracks not ripped yet
    /// (all of them when it is the disc waiting for Rip Disc).
    InBook { disc: u16, missing: Vec<TrackRef> },
    /// Another disc of the same release: MusicBrainz says it is disc `says`, and the book
    /// expects disc `expected`.
    OtherDisc { says: u32, expected: u16 },
    /// Not a disc of the release the book came from.
    Unrelated,
}

impl Book {
    /// What the disc with this MusicBrainz disc id is to the book.
    pub fn check_disc(&self, disc_id: &str) -> DiscCheck {
        if let Some((index, disc)) = self.discs.iter().enumerate().find(|(_, d)| d.disc_id == disc_id) {
            let number = u16::try_from(index + 1).unwrap_or(u16::MAX);
            let missing = disc.tracks.iter().filter(|t| !t.staged).map(|t| TrackRef::new(number, t.number)).collect();
            return DiscCheck::InBook { disc: number, missing };
        }
        if self.release_media.is_empty() {
            return DiscCheck::Next { known: false };
        }
        let expected = self.next_disc_number();
        match self.release_media.iter().find(|m| m.disc_ids.iter().any(|id| id == disc_id)) {
            Some(medium) if medium.position == u32::from(expected) => DiscCheck::Next { known: true },
            Some(medium) => DiscCheck::OtherDisc { says: medium.position, expected },
            // MusicBrainz may not know every pressing's disc id: a disc of the right length
            // could still belong. The listener decides.
            None => DiscCheck::Unrelated,
        }
    }

    /// How many discs MusicBrainz lists for the release, when known.
    pub fn release_disc_count(&self) -> Option<usize> {
        (!self.release_media.is_empty()).then_some(self.release_media.len())
    }
}

#[cfg(test)]
mod tests {
    use super::super::book::ReleaseMedium;
    use super::super::book::tests::book;
    use super::*;

    fn medium(position: u32, id: &str) -> ReleaseMedium {
        ReleaseMedium { position, disc_ids: vec![id.into()], track_titles: vec![] }
    }

    #[test]
    fn without_musicbrainz_any_new_disc_is_the_next() {
        let book = book(&[&[10]]);
        assert_eq!(book.check_disc("new"), DiscCheck::Next { known: false });
        assert_eq!(book.release_disc_count(), None);
    }

    #[test]
    fn a_disc_already_in_the_book_is_recognised_with_what_is_missing() {
        let mut book = book(&[&[10, 20], &[30]]);
        assert_eq!(book.check_disc("disc-1"), DiscCheck::InBook { disc: 1, missing: vec![] });
        book.discs[1].tracks[0].staged = false;
        assert_eq!(book.check_disc("disc-2"), DiscCheck::InBook { disc: 2, missing: vec![TrackRef::new(2, 1)] });
    }

    #[test]
    fn musicbrainz_places_the_disc_in_the_set() {
        let mut book = book(&[&[10]]);
        book.release_media = vec![medium(1, "disc-1"), medium(2, "second"), medium(3, "third")];
        assert_eq!(book.check_disc("second"), DiscCheck::Next { known: true });
        assert_eq!(book.check_disc("third"), DiscCheck::OtherDisc { says: 3, expected: 2 });
        assert_eq!(book.check_disc("stranger"), DiscCheck::Unrelated);
        assert_eq!(book.release_disc_count(), Some(3));
    }
}
