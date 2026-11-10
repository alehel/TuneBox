//! A pretend MusicBrainz and Cover Art Archive for demo mode, so the demo (and the screenshots
//! made from it) can look up the virtual CD and find album artwork without the network. It
//! answers in the services' own reply formats with made-up releases; the pictures are the demo
//! library's own procedurally drawn covers.

use std::collections::HashMap;
use tunebox_engine::{WebClient, WebError};

const MUSICBRAINZ: &str = "https://musicbrainz.org/ws/2";
const ARCHIVE: &str = "https://coverartarchive.org";

/// The virtual CD's release and release group.
pub const DISC_RELEASE: &str = "d15c0000-0000-4000-8000-000000000001";
const DISC_GROUP: &str = "d15c0000-0000-4000-8000-0000000000a1";

/// The editions of "Paper Boats" the album artwork search finds.
pub const PAPER_BOATS_CD: &str = "9a9e7b0a-0000-4000-8000-000000000001";
const PAPER_BOATS_JAPAN: &str = "9a9e7b0a-0000-4000-8000-000000000004";
const PAPER_BOATS_GROUP: &str = "9a9e7b0a-0000-4000-8000-0000000000a1";

pub struct DemoWeb {
    disc: (String, String, Vec<String>),
    /// Pictures by "release:<id>" or "release-group:<id>".
    pictures: HashMap<String, Vec<u8>>,
}

impl DemoWeb {
    /// `disc`: the virtual CD's album, artist and track titles. `album_cover` is the cover of
    /// "Paper Boats", `other_cover` stands in for a reissue's different artwork, and
    /// `disc_cover` is the virtual CD's.
    pub fn new(
        disc: (String, String, Vec<String>),
        album_cover: Vec<u8>,
        other_cover: Vec<u8>,
        disc_cover: Vec<u8>,
    ) -> Self {
        let pictures = HashMap::from([
            (format!("release:{PAPER_BOATS_CD}"), album_cover.clone()),
            (format!("release-group:{PAPER_BOATS_GROUP}"), album_cover),
            (format!("release:{PAPER_BOATS_JAPAN}"), other_cover),
            (format!("release:{DISC_RELEASE}"), disc_cover),
        ]);
        Self { disc, pictures }
    }

    fn disc_reply(&self) -> String {
        let (album, artist, titles) = &self.disc;
        let tracks: Vec<String> = titles.iter().map(|t| format!(r#"{{"title": {}}}"#, quoted(t))).collect();
        format!(
            r#"{{"releases": [{{"id": "{DISC_RELEASE}", "release-group": {{"id": "{DISC_GROUP}"}},
                "title": {}, "date": "2011-05-02", "artist-credit": [{{"name": {}}}],
                "genres": [{{"name": "folk", "count": 2}}],
                "media": [{{"position": 1, "tracks": [{}]}}]}}]}}"#,
            quoted(album),
            quoted(artist),
            tracks.join(", ")
        )
    }

    fn picture(&self, kind: &str, id: &str) -> Result<Vec<u8>, WebError> {
        self.pictures.get(&format!("{kind}:{id}")).cloned().ok_or(WebError::NotFound)
    }
}

impl WebClient for DemoWeb {
    fn get(&self, url: &str, _limit: u64) -> Result<Vec<u8>, WebError> {
        if url.starts_with(&format!("{MUSICBRAINZ}/discid/")) {
            return Ok(self.disc_reply().into_bytes());
        }
        if let Some(query) = url.strip_prefix(&format!("{MUSICBRAINZ}/release/?query=")) {
            let found = if query.contains("Paper%20Boats") { PAPER_BOATS } else { r#"{"releases": []}"# };
            return Ok(found.as_bytes().to_vec());
        }
        let path = url.strip_prefix(ARCHIVE).ok_or(WebError::NotFound)?;
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        match parts[..] {
            [kind, id, "front-250"] => self.picture(kind, id),
            [kind, id, "demo-1200.jpg"] => self.picture(kind, id),
            [kind, id] => {
                self.picture(kind, id)?;
                let image = format!("{ARCHIVE}/{kind}/{id}/demo-1200.jpg");
                Ok(format!(
                    r#"{{"images": [{{"front": true, "image": "{image}", "thumbnails": {{"1200": "{image}"}}}}]}}"#
                )
                .into_bytes())
            }
            _ => Err(WebError::NotFound),
        }
    }
}

/// `text` as a JSON string.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Four editions of the demo album "Paper Boats": the CD with its cover, a vinyl edition with
/// none of its own (the album's is shown), a cassette with none at all, and a Japanese CD with
/// different artwork.
const PAPER_BOATS: &str = r#"{"releases": [
  {"id": "9a9e7b0a-0000-4000-8000-000000000001", "score": 100, "title": "Paper Boats",
   "artist-credit": [{"name": "The Harbour Lanterns"}], "release-group": {"id": "9a9e7b0a-0000-4000-8000-0000000000a1"},
   "date": "1997-03-14", "country": "GB", "track-count": 6, "media": [{"format": "CD"}],
   "label-info": [{"label": {"name": "Driftwood Records"}}]},
  {"id": "9a9e7b0a-0000-4000-8000-000000000002", "score": 96, "title": "Paper Boats",
   "artist-credit": [{"name": "The Harbour Lanterns"}], "release-group": {"id": "9a9e7b0a-0000-4000-8000-0000000000a1"},
   "date": "2017-04-22", "country": "XE", "track-count": 6, "media": [{"format": "12\" Vinyl"}],
   "label-info": [{"label": {"name": "Driftwood Records"}}], "disambiguation": "20th anniversary"},
  {"id": "9a9e7b0a-0000-4000-8000-000000000003", "score": 90, "title": "Paper Boats",
   "artist-credit": [{"name": "The Harbour Lanterns"}],
   "date": "1997", "country": "PL", "track-count": 6, "media": [{"format": "Cassette"}]},
  {"id": "9a9e7b0a-0000-4000-8000-000000000004", "score": 88, "title": "Paper Boats",
   "artist-credit": [{"name": "The Harbour Lanterns"}], "release-group": {"id": "9a9e7b0a-0000-4000-8000-0000000000a1"},
   "date": "1998-01-21", "country": "JP", "track-count": 8, "media": [{"format": "CD"}],
   "label-info": [{"label": {"name": "Seabird Japan"}}], "disambiguation": "with bonus tracks"}
]}"#;

#[cfg(test)]
mod tests {
    use super::*;

    const PAPER_BOATS_VINYL: &str = "9a9e7b0a-0000-4000-8000-000000000002";
    const PAPER_BOATS_CASSETTE: &str = "9a9e7b0a-0000-4000-8000-000000000003";

    #[test]
    fn answers_like_the_services() {
        let web = DemoWeb::new(("Album".into(), "Band".into(), vec!["One".into()]), vec![1], vec![2], vec![3]);
        let disc = web.get(&format!("{MUSICBRAINZ}/discid/abc?inc=x"), 1000).unwrap();
        assert!(String::from_utf8(disc).unwrap().contains(DISC_RELEASE));
        assert_eq!(web.get(&format!("{ARCHIVE}/release/{PAPER_BOATS_CD}/front-250"), 10), Ok(vec![1]));
        assert_eq!(web.get(&format!("{ARCHIVE}/release/{PAPER_BOATS_VINYL}/front-250"), 10), Err(WebError::NotFound));
        assert_eq!(web.get(&format!("{ARCHIVE}/release/{PAPER_BOATS_CASSETTE}"), 10), Err(WebError::NotFound));
        let index = web.get(&format!("{ARCHIVE}/release/{DISC_RELEASE}"), 1000).unwrap();
        assert!(String::from_utf8(index).unwrap().contains("demo-1200.jpg"));
        assert_eq!(quoted("a \"b\" \\ c"), r#""a \"b\" \\ c""#);
    }
}
