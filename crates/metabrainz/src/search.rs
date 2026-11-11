//! Finding the releases (editions) of an album on MusicBrainz, by artist and title.
//!
//! The listener chooses among them in "Find Album Artwork…", so each candidate carries what
//! tells editions apart: date, country, format, track count, label and MusicBrainz's own note.

use crate::client::{WebClient, WebError};
use serde::Deserialize;

/// How many releases a search returns: enough to find the right edition, few enough to show
/// each with its cover.
pub const CANDIDATES: usize = 10;

/// Search replies are small; anything bigger is not one.
const REPLY_LIMIT: u64 = 2_000_000;

/// One release that may be the album.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReleaseCandidate {
    /// The release's MusicBrainz id.
    pub id: String,
    /// The release group's: the album across all its editions.
    pub release_group_id: Option<String>,
    pub title: String,
    pub artist: String,
    /// "1997", "1997-07-01"…, or empty.
    pub date: String,
    /// "GB", "US", "XE" (Europe)…, or empty.
    pub country: String,
    /// "CD", "2×CD", "CD + DVD", "12\" Vinyl"…, or empty.
    pub format: String,
    pub track_count: Option<u32>,
    pub label: String,
    pub catalog_number: String,
    /// MusicBrainz's note telling this edition from others ("remastered", "deluxe edition").
    pub disambiguation: String,
    /// How well it matched, 0–100.
    pub score: u32,
}

impl ReleaseCandidate {
    /// What tells this edition apart, in one line: "1997 · GB · CD · 12 tracks · Parlophone".
    pub fn details(&self) -> String {
        let tracks = match self.track_count {
            Some(1) => "1 track".to_owned(),
            Some(n) => format!("{n} tracks"),
            None => String::new(),
        };
        [self.date.as_str(), &self.country, &self.format, &tracks, &self.label, &self.disambiguation]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// The search for releases called `album` by `artist` (any artist when empty).
pub fn release_search_url(artist: &str, album: &str) -> String {
    let mut query = format!("release:{}", phrase(album));
    if !artist.trim().is_empty() {
        query.push_str(&format!(" AND artist:{}", phrase(artist)));
    }
    format!("{}/release/?query={}&limit={CANDIDATES}&fmt=json", crate::MUSICBRAINZ, percent_encode(&query))
}

/// Releases matching `album` by `artist`, best first. Empty when MusicBrainz knows none.
pub fn search_releases(client: &dyn WebClient, artist: &str, album: &str) -> Result<Vec<ReleaseCandidate>, WebError> {
    match client.get(&release_search_url(artist, album), REPLY_LIMIT) {
        Ok(body) => parse_release_search(&body),
        Err(WebError::NotFound) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Turns a release search reply into candidates, leaving out any whose id is not an id.
pub fn parse_release_search(json: &[u8]) -> Result<Vec<ReleaseCandidate>, WebError> {
    let reply: Reply = serde_json::from_slice(json).map_err(|e| WebError::BadReply(e.to_string()))?;
    Ok(reply.releases.into_iter().filter(|r| crate::is_mbid(&r.id)).map(candidate).collect())
}

fn candidate(release: Release) -> ReleaseCandidate {
    let label = release.label_info.iter().find_map(|l| l.label.as_ref().map(|l| l.name.clone())).unwrap_or_default();
    let catalog_number = release.label_info.iter().find_map(|l| l.catalog_number.clone()).unwrap_or_default();
    ReleaseCandidate {
        release_group_id: release.release_group.map(|g| g.id).filter(|id| crate::is_mbid(id)),
        title: release.title,
        artist: release
            .artist_credit
            .iter()
            .map(|c| format!("{}{}", c.name, c.joinphrase))
            .collect::<String>()
            .trim()
            .to_owned(),
        date: release.date,
        country: release.country.unwrap_or_default(),
        format: format_of(&release.media),
        track_count: release.track_count,
        label,
        catalog_number,
        disambiguation: release.disambiguation,
        score: release.score.unwrap_or(0),
        id: release.id,
    }
}

/// "2×CD" for two CDs, "CD + DVD" for a mix, "" when unknown.
fn format_of(media: &[Medium]) -> String {
    let mut runs: Vec<(String, usize)> = Vec::new();
    for medium in media {
        let name = medium.format.clone().unwrap_or_else(|| "(unknown)".into());
        match runs.iter_mut().find(|(n, _)| *n == name) {
            Some((_, count)) => *count += 1,
            None => runs.push((name, 1)),
        }
    }
    if runs.iter().all(|(name, _)| name == "(unknown)") {
        return String::new();
    }
    runs.into_iter()
        .map(|(name, count)| if count == 1 { name } else { format!("{count}×{name}") })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// A Lucene phrase: in quotes, with quotes and backslashes inside escaped.
fn phrase(text: &str) -> String {
    let escaped: String =
        text.trim().chars().flat_map(|c| if c == '"' || c == '\\' { vec!['\\', c] } else { vec![c] }).collect();
    format!("\"{escaped}\"")
}

/// Percent-encodes everything but the unreserved characters of RFC 3986.
fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    releases: Vec<Release>,
}

#[derive(Deserialize)]
struct Release {
    id: String,
    #[serde(default)]
    score: Option<u32>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    disambiguation: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    country: Option<String>,
    #[serde(rename = "track-count", default)]
    track_count: Option<u32>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<Credit>,
    #[serde(rename = "release-group", default)]
    release_group: Option<Group>,
    #[serde(rename = "label-info", default)]
    label_info: Vec<LabelInfo>,
    #[serde(default)]
    media: Vec<Medium>,
}

#[derive(Deserialize)]
struct Credit {
    name: String,
    #[serde(default)]
    joinphrase: String,
}

#[derive(Deserialize)]
struct Group {
    id: String,
}

#[derive(Deserialize)]
struct LabelInfo {
    #[serde(rename = "catalog-number", default)]
    catalog_number: Option<String>,
    #[serde(default)]
    label: Option<Label>,
}

#[derive(Deserialize)]
struct Label {
    name: String,
}

#[derive(Deserialize)]
struct Medium {
    #[serde(default)]
    format: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::FakeWeb;

    /// A hand-written reply in the shape of the MusicBrainz search service (fictional releases).
    const REPLY: &str = r#"{
      "created": "2026-01-01T00:00:00.000Z", "count": 3, "offset": 0,
      "releases": [
        {
          "id": "11111111-2222-3333-4444-555555555555", "score": 100,
          "title": "Tidewater", "status": "Official", "disambiguation": "remastered",
          "artist-credit": [{"name": "The Lanterns", "joinphrase": " & "}, {"name": "Mara"}],
          "release-group": {"id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", "primary-type": "Album"},
          "date": "2019-03-01", "country": "XE", "track-count": 14,
          "label-info": [{"catalog-number": "LNT 001", "label": {"id": "x", "name": "Harbour Records"}}],
          "media": [{"format": "CD", "track-count": 7}, {"format": "CD", "track-count": 7}]
        },
        {
          "id": "66666666-7777-8888-9999-000000000000", "score": 87, "title": "Tidewater",
          "artist-credit": [{"name": "The Lanterns"}],
          "media": [{"format": "12\" Vinyl"}, {"format": "DVD"}]
        },
        {"id": "not-an-id", "title": "Broken"}
      ]
    }"#;

    #[test]
    fn reads_what_tells_editions_apart() {
        let found = parse_release_search(REPLY.as_bytes()).unwrap();
        assert_eq!(found.len(), 2, "the entry without a real id is left out");
        let first = &found[0];
        assert_eq!(first.artist, "The Lanterns & Mara");
        assert_eq!(first.release_group_id.as_deref(), Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"));
        assert_eq!(first.format, "2×CD");
        assert_eq!((first.label.as_str(), first.catalog_number.as_str()), ("Harbour Records", "LNT 001"));
        assert_eq!(first.details(), "2019-03-01 · XE · 2×CD · 14 tracks · Harbour Records · remastered");
        let second = &found[1];
        assert_eq!(second.format, "12\" Vinyl + DVD");
        assert_eq!((second.release_group_id.as_ref(), second.track_count, second.score), (None, None, 87));
        assert_eq!(second.details(), "12\" Vinyl + DVD");
    }

    #[test]
    fn bad_and_empty_replies() {
        assert!(parse_release_search(br#"{"releases": []}"#).unwrap().is_empty());
        assert!(parse_release_search(br#"{"count": 0}"#).unwrap().is_empty());
        assert!(matches!(parse_release_search(b"<html>busy</html>"), Err(WebError::BadReply(_))));
    }

    #[test]
    fn the_query_is_quoted_and_encoded() {
        let url = release_search_url("Sigur \"Rós\"", "Ágætis byrjun & more");
        assert!(url.starts_with("https://musicbrainz.org/ws/2/release/?query=release%3A%22"), "{url}");
        assert!(url.contains("%C3%81g%C3%A6tis%20byrjun%20%26%20more%22"), "{url}");
        assert!(url.contains("%20AND%20artist%3A%22Sigur%20%5C%22R%C3%B3s%5C%22%22"), "{url}");
        assert!(url.ends_with("&limit=10&fmt=json"));
        assert!(!release_search_url("  ", "X").contains("artist"));
    }

    #[test]
    fn searches_through_the_client() {
        let url = release_search_url("The Lanterns", "Tidewater");
        let fake = FakeWeb::default().reply(&url, Ok(REPLY.as_bytes()));
        assert_eq!(search_releases(&fake, "The Lanterns", "Tidewater").unwrap().len(), 2);
        assert_eq!(fake.asked(), [url]);
        let busy = FakeWeb::default().reply(&release_search_url("A", "B"), Err(WebError::Busy));
        assert_eq!(search_releases(&busy, "A", "B"), Err(WebError::Busy));
    }
}
