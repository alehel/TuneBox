//! What is known about a disc's contents: names typed in, looked up, or placeholders.

use crate::toc::DiscToc;
use tunebox_model::{NumberOf, TrackMeta};

/// Album and track names for a disc.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscInfo {
    pub album: String,
    pub artist: String,
    pub genre: String,
    pub year: Option<u32>,
    pub compilation: bool,
    /// Disc 1 of 2, etc.
    pub disc: NumberOf,
    /// Per track, in disc order: (title, artist). An empty artist means the album artist.
    pub tracks: Vec<(String, String)>,
}

impl DiscInfo {
    /// The names shown before any are known: "Audio CD", "Track 01"…
    pub fn placeholder(toc: &DiscToc) -> Self {
        Self {
            album: "Audio CD".into(),
            tracks: toc.audio_tracks().map(|t| (format!("Track {:02}", t.number), String::new())).collect(),
            ..Self::default()
        }
    }

    /// The metadata for audio track `number` (as numbered on the disc).
    pub fn track_meta(&self, toc: &DiscToc, number: u8) -> TrackMeta {
        let audio: Vec<u8> = toc.audio_tracks().map(|t| t.number).collect();
        let index = audio.iter().position(|&n| n == number);
        let (title, track_artist) = index.and_then(|i| self.tracks.get(i)).cloned().unwrap_or_default();
        let artist = if track_artist.is_empty() { self.artist.clone() } else { track_artist };
        TrackMeta {
            title: if title.is_empty() { format!("Track {number:02}") } else { title },
            artist,
            album_artist: if self.compilation { String::new() } else { self.artist.clone() },
            album: self.album.clone(),
            genre: self.genre.clone(),
            year: self.year,
            track: NumberOf { number: index.map(|i| i as u32 + 1), total: Some(audio.len() as u32) },
            disc: self.disc,
            compilation: self.compilation,
            ..TrackMeta::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toc::tests::sample_toc;

    #[test]
    fn placeholder_names_every_audio_track() {
        let info = DiscInfo::placeholder(&sample_toc());
        assert_eq!(info.tracks.len(), 6);
        assert_eq!(info.track_meta(&sample_toc(), 3).title, "Track 03");
    }

    #[test]
    fn track_artist_overrides_album_artist() {
        let toc = sample_toc();
        let mut info = DiscInfo {
            album: "Mix".into(),
            artist: "Various Artists".into(),
            compilation: true,
            ..DiscInfo::placeholder(&toc)
        };
        info.tracks[1] = ("Song".into(), "Guest".into());
        let meta = info.track_meta(&toc, 2);
        assert_eq!((meta.title.as_str(), meta.artist.as_str()), ("Song", "Guest"));
        assert_eq!(meta.track, NumberOf::new(2, 6));
        assert!(meta.compilation);
        assert_eq!(info.track_meta(&toc, 1).artist, "Various Artists");
    }
}
