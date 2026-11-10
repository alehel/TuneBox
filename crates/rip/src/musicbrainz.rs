//! Track names from MusicBrainz, looked up by disc id.
//!
//! The request is only made when the listener asks for track names (or turns on automatic
//! lookup). Nothing about the listener is sent: the request carries the disc id, and the
//! client (`tunebox-metabrainz`) an application user agent, as MusicBrainz asks.

use crate::info::DiscInfo;
use serde::Deserialize;
use tunebox_metabrainz::{WebClient, WebError};
use tunebox_model::NumberOf;

/// Disc replies are small; anything bigger is not one.
const REPLY_LIMIT: u64 = 5_000_000;

/// Why a lookup failed.
#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    #[error("MusicBrainz could not be asked: {0}")]
    Network(WebError),
    #[error("MusicBrainz sent a reply TuneBox could not read: {0}")]
    BadReply(String),
}

/// The lookup of the releases with this disc (and their release groups, for covers).
pub fn lookup_url(disc_id: &str) -> String {
    // Disc ids are base64 with `._-`: nothing in them needs escaping in a URL path.
    let disc_id: String = disc_id.chars().filter(|c| c.is_ascii_alphanumeric() || "._-".contains(*c)).collect();
    format!(
        "{}/discid/{disc_id}?inc=artist-credits+recordings+genres+release-groups&fmt=json",
        tunebox_metabrainz::MUSICBRAINZ
    )
}

/// Every release MusicBrainz knows with this disc, best first. Empty when the disc is unknown.
pub fn lookup(client: &dyn WebClient, disc_id: &str) -> Result<Vec<DiscInfo>, LookupError> {
    let body = match client.get(&lookup_url(disc_id), REPLY_LIMIT) {
        Ok(body) => body,
        Err(WebError::NotFound) => return Ok(Vec::new()),
        Err(e) => return Err(LookupError::Network(e)),
    };
    parse(&String::from_utf8_lossy(&body), disc_id)
}

/// Turns a `discid` reply into disc infos for the medium carrying `disc_id`.
pub fn parse(json: &str, disc_id: &str) -> Result<Vec<DiscInfo>, LookupError> {
    let reply: Reply = serde_json::from_str(json).map_err(|e| LookupError::BadReply(e.to_string()))?;
    Ok(reply.releases.iter().filter_map(|release| release_info(release, disc_id)).collect())
}

fn release_info(release: &Release, disc_id: &str) -> Option<DiscInfo> {
    let medium_count = release.media.len() as u32;
    let medium =
        release.media.iter().find(|m| m.discs.iter().any(|d| d.id == disc_id)).or_else(|| release.media.first())?;
    let artist = credit(&release.artist_credit);
    let compilation = artist == "Various Artists";
    let tracks = medium
        .tracks
        .iter()
        .map(|t| {
            let track_artist = credit(&t.artist_credit);
            (t.title.clone(), if track_artist == artist { String::new() } else { track_artist })
        })
        .collect();
    let genre = release.genres.iter().max_by_key(|g| g.count).map(|g| title_case(&g.name)).unwrap_or_default();
    Some(DiscInfo {
        album: release.title.clone(),
        artist,
        genre,
        year: release.date.get(..4).and_then(|y| y.parse().ok()),
        compilation,
        disc: if medium_count > 1 {
            NumberOf { number: medium.position, total: Some(medium_count) }
        } else {
            NumberOf::default()
        },
        tracks,
        release_id: Some(release.id.clone()).filter(|id| tunebox_metabrainz::is_mbid(id)),
        release_group_id: release
            .release_group
            .as_ref()
            .map(|g| g.id.clone())
            .filter(|id| tunebox_metabrainz::is_mbid(id)),
    })
}

/// "The Band feat. Guest" from a list of credits and their join phrases.
fn credit(credits: &[Credit]) -> String {
    credits.iter().map(|c| format!("{}{}", c.name, c.joinphrase)).collect::<String>().trim().to_owned()
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    releases: Vec<Release>,
}

#[derive(Deserialize)]
struct Release {
    #[serde(default)]
    id: String,
    #[serde(rename = "release-group", default)]
    release_group: Option<ReleaseGroup>,
    title: String,
    #[serde(default)]
    date: String,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<Credit>,
    #[serde(default)]
    media: Vec<Medium>,
    #[serde(default)]
    genres: Vec<Genre>,
}

#[derive(Deserialize)]
struct ReleaseGroup {
    id: String,
}

#[derive(Deserialize)]
struct Medium {
    position: Option<u32>,
    #[serde(default)]
    discs: Vec<Disc>,
    #[serde(default)]
    tracks: Vec<Track>,
}

#[derive(Deserialize)]
struct Disc {
    id: String,
}

#[derive(Deserialize)]
struct Track {
    title: String,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<Credit>,
}

#[derive(Deserialize)]
struct Credit {
    name: String,
    #[serde(default)]
    joinphrase: String,
}

#[derive(Deserialize)]
struct Genre {
    name: String,
    #[serde(default)]
    count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-written reply in the shape of the MusicBrainz web service (fictional release).
    const REPLY: &str = r#"{
      "releases": [{
        "id": "11111111-2222-3333-4444-555555555555",
        "release-group": {"id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"},
        "title": "Tidewater",
        "date": "2009-05-04",
        "artist-credit": [{"name": "The Lanterns", "joinphrase": ""}],
        "genres": [{"name": "indie folk", "count": 3}, {"name": "rock", "count": 1}],
        "media": [
          {"position": 1, "discs": [{"id": "other"}], "tracks": [{"title": "Wrong Disc"}]},
          {"position": 2, "discs": [{"id": "ABC-"}], "tracks": [
            {"title": "Low Tide", "artist-credit": [{"name": "The Lanterns", "joinphrase": ""}]},
            {"title": "Breakwater", "artist-credit": [{"name": "The Lanterns", "joinphrase": " feat. "}, {"name": "Mara", "joinphrase": ""}]}
          ]}
        ]
      }]
    }"#;

    #[test]
    fn picks_the_medium_with_this_disc() {
        let infos = parse(REPLY, "ABC-").unwrap();
        assert_eq!(infos.len(), 1);
        let info = &infos[0];
        assert_eq!(info.album, "Tidewater");
        assert_eq!(info.artist, "The Lanterns");
        assert_eq!(info.year, Some(2009));
        assert_eq!(info.genre, "Indie Folk");
        assert_eq!(info.disc, NumberOf::new(2, 2));
        assert_eq!(info.release_id.as_deref(), Some("11111111-2222-3333-4444-555555555555"));
        assert_eq!(info.release_group_id.as_deref(), Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"));
        assert_eq!(
            info.tracks,
            [("Low Tide".into(), String::new()), ("Breakwater".into(), "The Lanterns feat. Mara".into())]
        );
    }

    #[test]
    fn unknown_disc_and_bad_replies() {
        assert!(parse(r#"{"releases": []}"#, "x").unwrap().is_empty());
        assert!(matches!(parse("<html>", "x"), Err(LookupError::BadReply(_))));
        let without_id = parse(r#"{"releases": [{"id": "?", "title": "T", "media": [{"tracks": []}]}]}"#, "x").unwrap();
        assert_eq!(without_id[0].release_id, None, "an id that is not one is not kept");
    }

    #[test]
    fn the_disc_id_cannot_change_the_request() {
        let url = lookup_url("ab/../C_d.e-");
        assert!(url.starts_with("https://musicbrainz.org/ws/2/discid/ab..C_d.e-?inc="), "{url}");
        assert!(url.contains("release-groups"));
    }
}
