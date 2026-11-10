//! Disc identifiers computed from the table of contents.

use crate::toc::{DiscToc, FRAMES_PER_SECOND};
use base64::Engine as _;
use sha1::{Digest, Sha1};

/// The lead-in, in sectors, which disc ids count from.
const LEAD_IN: u32 = 150;

/// The MusicBrainz disc id: SHA-1 of the first and last track numbers, the lead-out and 99
/// track offsets in fixed-width hex, in a URL-safe base64 alphabet.
pub fn disc_id(toc: &DiscToc) -> String {
    let audio: Vec<_> = toc.audio_tracks().collect();
    let first = audio.first().map_or(1, |t| t.number);
    let last = audio.last().map_or(0, |t| t.number);
    let mut text = format!("{first:02X}{last:02X}{:08X}", toc.audio_leadout() + LEAD_IN);
    for number in 1..=99u8 {
        let offset = audio.iter().find(|t| t.number == number).map_or(0, |t| t.start_lba + LEAD_IN);
        text.push_str(&format!("{offset:08X}"));
    }
    let digest = Sha1::digest(text.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest).replace('+', ".").replace('/', "_").replace('=', "-")
}

/// The older FreeDB/CDDB id, still printed on many disc databases.
pub fn freedb_id(toc: &DiscToc) -> u32 {
    let digit_sum = |mut n: u32| {
        let mut sum = 0;
        while n > 0 {
            sum += n % 10;
            n /= 10;
        }
        sum
    };
    let seconds = |lba: u32| (lba + LEAD_IN) / FRAMES_PER_SECOND;
    let checksum: u32 = toc.tracks.iter().map(|t| digit_sum(seconds(t.start_lba))).sum();
    let first = toc.tracks.first().map_or(0, |t| seconds(t.start_lba));
    let length = seconds(toc.leadout_lba).saturating_sub(first);
    ((checksum % 0xFF) << 24) | (length << 8) | (toc.tracks.len() as u32 & 0xFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toc::tests::sample_toc;

    #[test]
    fn matches_the_musicbrainz_documentation_example() {
        assert_eq!(disc_id(&sample_toc()), "49HHV7Eb8UKF3aQiNmu1GR8vKTY-");
    }

    #[test]
    fn freedb_id_packs_checksum_length_and_count() {
        let id = freedb_id(&sample_toc());
        assert_eq!(id & 0xFF, 6);
        assert_eq!((id >> 8) & 0xFFFF, (95_312 + 150) / 75 - 2);
    }
}
