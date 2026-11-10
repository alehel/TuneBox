//! Star ratings.

use serde::{Deserialize, Serialize};

/// Zero to five stars. Zero means "not rated".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rating(u8);

impl Rating {
    pub const MAX_STARS: u8 = 5;
    pub const UNRATED: Rating = Rating(0);

    /// A rating of `stars`, clamped to 0–5.
    pub fn from_stars(stars: u8) -> Self {
        Rating(stars.min(Self::MAX_STARS))
    }

    pub fn stars(self) -> u8 {
        self.0
    }

    pub fn is_rated(self) -> bool {
        self.0 > 0
    }

    /// "★★★☆☆" style text, for places that cannot draw stars.
    pub fn glyphs(self) -> String {
        (0..Self::MAX_STARS).map(|i| if i < self.0 { '★' } else { '·' }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_to_five() {
        assert_eq!(Rating::from_stars(9).stars(), 5);
        assert_eq!(Rating::from_stars(3).glyphs(), "★★★··");
    }
}
