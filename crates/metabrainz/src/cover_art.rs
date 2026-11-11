//! Front covers from the Cover Art Archive.
//!
//! The archive keeps pictures per release (one edition) and points each release group (the
//! album across its editions) at one release's pictures. TuneBox asks for the release's own
//! front cover first, since the listener chose that edition, and falls back to the release
//! group's when the release has none: another edition's cover is a better fit than nothing.
//! [`Cover::from`] says which was used.
//!
//! For a cover to keep, TuneBox reads the release's image list and takes the front image's
//! 1200-pixel version ([`COVER_SIDE`]); for the picker, the 250-pixel thumbnail.

use crate::client::{WebClient, WebError};
use serde::Deserialize;
use std::collections::BTreeMap;

/// The Cover Art Archive.
pub const ARCHIVE: &str = "https://coverartarchive.org";

/// The size of cover TuneBox keeps. The archive serves 250, 500 and 1200 pixel versions and the
/// original. 1200 is sharp on any screen a cover is shown on and below TuneBox's 1400-pixel
/// limit, so it is stored as served; originals are often multi-megabyte scans, copied into
/// every song of the album for no visible gain.
pub const COVER_SIDE: u32 = 1200;

/// The size of the thumbnails the picker shows.
pub const THUMBNAIL_SIDE: u32 = 250;

/// Image lists are small.
const INDEX_LIMIT: u64 = 2_000_000;
/// A 1200-pixel JPEG is well under 2 MB; the limit leaves room for an original when no
/// smaller version exists, and refuses anything absurd.
const IMAGE_LIMIT: u64 = 30_000_000;
const THUMBNAIL_LIMIT: u64 = 2_000_000;

/// A release, and the release group it belongs to when known.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReleaseRef {
    pub release: String,
    pub release_group: Option<String>,
}

/// Whose cover a picture is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverOf {
    /// The chosen release's own.
    Release,
    /// The release group's: another edition of the same album.
    ReleaseGroup,
}

/// A fetched picture (its bytes as served: normally JPEG, sometimes PNG).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cover {
    pub data: Vec<u8>,
    pub from: CoverOf,
}

impl CoverOf {
    fn path(self) -> &'static str {
        match self {
            CoverOf::Release => "release",
            CoverOf::ReleaseGroup => "release-group",
        }
    }
}

/// The front cover of `release` at about [`COVER_SIDE`] pixels, or of its release group when
/// the release has none. `Ok(None)`: the archive has no front cover for either.
pub fn fetch_front_cover(client: &dyn WebClient, release: &ReleaseRef) -> Result<Option<Cover>, WebError> {
    for (of, id) in candidates(release)? {
        let index = match client.get(&index_url(of, id), INDEX_LIMIT) {
            Ok(body) => body,
            Err(WebError::NotFound) => continue,
            Err(e) => return Err(e),
        };
        let Some(front) = parse_index(&index)? else { continue };
        let Some(url) = front.url_for(COVER_SIDE) else { continue };
        match client.get(&url, IMAGE_LIMIT) {
            Ok(data) => return Ok(Some(Cover { data, from: of })),
            // Listed but gone: the archive can lag behind its own list.
            Err(WebError::NotFound) => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

/// The [`THUMBNAIL_SIDE`] front thumbnail of `release`, or of its release group. One request
/// each: the archive redirects `front-250` to the picture.
pub fn fetch_thumbnail(client: &dyn WebClient, release: &ReleaseRef) -> Result<Option<Cover>, WebError> {
    for (of, id) in candidates(release)? {
        match client.get(&thumbnail_url(of, id), THUMBNAIL_LIMIT) {
            Ok(data) => return Ok(Some(Cover { data, from: of })),
            Err(WebError::NotFound) => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

/// Where to look, in order. Ids go into URLs, so a release id that is not one is refused; a
/// release group id that is not one is passed over.
fn candidates(release: &ReleaseRef) -> Result<Vec<(CoverOf, &str)>, WebError> {
    if !crate::is_mbid(&release.release) {
        return Err(WebError::BadReply(format!("“{}” is not a MusicBrainz release id", release.release)));
    }
    let mut places = vec![(CoverOf::Release, release.release.as_str())];
    if let Some(group) = release.release_group.as_deref().filter(|id| crate::is_mbid(id)) {
        places.push((CoverOf::ReleaseGroup, group));
    }
    Ok(places)
}

/// The list of a release's (or release group's) images.
pub fn index_url(of: CoverOf, id: &str) -> String {
    format!("{ARCHIVE}/{}/{id}", of.path())
}

/// The front thumbnail of a release (or release group), redirected to the picture.
pub fn thumbnail_url(of: CoverOf, id: &str) -> String {
    format!("{ARCHIVE}/{}/{id}/front-{THUMBNAIL_SIDE}", of.path())
}

/// The front image in an image list, if it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontImage {
    pub image: String,
    /// By size: "250", "500", "1200", and the older names "small" (250) and "large" (500).
    pub thumbnails: BTreeMap<String, String>,
}

impl FrontImage {
    /// The largest version no bigger than `side`, else the smallest larger one, else the
    /// original. Only addresses on the archive's own servers are used.
    pub fn url_for(&self, side: u32) -> Option<String> {
        let sized = |name: &str| match name {
            "small" => Some(250),
            "large" => Some(500),
            other => other.parse::<u32>().ok(),
        };
        let mut versions: Vec<(u32, &String)> =
            self.thumbnails.iter().filter_map(|(name, url)| sized(name).map(|s| (s, url))).collect();
        versions.sort_by_key(|(s, _)| *s);
        let fitting = versions.iter().rev().find(|(s, _)| *s <= side);
        let larger = versions.iter().find(|(s, _)| *s > side);
        fitting.or(larger).map(|(_, url)| url.as_str()).into_iter().chain([self.image.as_str()]).find_map(trusted)
    }
}

/// `url` made HTTPS, if it is on the Cover Art Archive's or the Internet Archive's servers.
fn trusted(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    let ours = host == "coverartarchive.org" || host == "archive.org" || host.ends_with(".archive.org");
    ours.then(|| format!("https://{rest}"))
}

/// Reads an image list and finds the front image. `Ok(None)` when it has none.
pub fn parse_index(json: &[u8]) -> Result<Option<FrontImage>, WebError> {
    let index: Index = serde_json::from_slice(json).map_err(|e| WebError::BadReply(e.to_string()))?;
    let is_front = |image: &&Image| image.front || image.types.iter().any(|t| t == "Front");
    Ok(index
        .images
        .into_iter()
        .find(|image| is_front(&image))
        .map(|image| FrontImage { image: image.image, thumbnails: image.thumbnails }))
}

#[derive(Deserialize)]
struct Index {
    #[serde(default)]
    images: Vec<Image>,
}

#[derive(Deserialize)]
struct Image {
    #[serde(default)]
    front: bool,
    #[serde(default)]
    types: Vec<String>,
    #[serde(default)]
    image: String,
    #[serde(default)]
    thumbnails: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::FakeWeb;

    const RELEASE: &str = "11111111-2222-3333-4444-555555555555";
    const GROUP: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

    /// A hand-written image list in the archive's shape (a fictional release): a back cover
    /// first, then the front.
    const INDEX: &str = r#"{
      "release": "https://musicbrainz.org/release/11111111-2222-3333-4444-555555555555",
      "images": [
        {"front": false, "back": true, "types": ["Back"], "approved": true,
         "image": "http://coverartarchive.org/release/11111111-2222-3333-4444-555555555555/2.jpg",
         "thumbnails": {"250": "http://coverartarchive.org/release/11111111-2222-3333-4444-555555555555/2-250.jpg"}},
        {"front": true, "back": false, "types": ["Front"], "approved": true, "id": 1,
         "image": "http://coverartarchive.org/release/11111111-2222-3333-4444-555555555555/1.jpg",
         "thumbnails": {
           "250": "http://coverartarchive.org/release/11111111-2222-3333-4444-555555555555/1-250.jpg",
           "500": "http://coverartarchive.org/release/11111111-2222-3333-4444-555555555555/1-500.jpg",
           "1200": "http://coverartarchive.org/release/11111111-2222-3333-4444-555555555555/1-1200.jpg",
           "small": "http://coverartarchive.org/release/11111111-2222-3333-4444-555555555555/1-250.jpg",
           "large": "http://coverartarchive.org/release/11111111-2222-3333-4444-555555555555/1-500.jpg"
         }}
      ]
    }"#;

    fn release(group: Option<&str>) -> ReleaseRef {
        ReleaseRef { release: RELEASE.into(), release_group: group.map(Into::into) }
    }

    #[test]
    fn finds_the_front_image_and_the_size_to_keep() {
        let front = parse_index(INDEX.as_bytes()).unwrap().unwrap();
        assert!(front.image.ends_with("/1.jpg"));
        let url = |side| front.url_for(side).unwrap();
        assert_eq!(url(1200), format!("{ARCHIVE}/release/{RELEASE}/1-1200.jpg"), "and made HTTPS");
        assert!(url(600).ends_with("/1-500.jpg"));
        assert!(url(100).ends_with("/1-250.jpg"), "the smallest when none is small enough");
        let old = FrontImage { image: "https://ia800.us.archive.org/x.jpg".into(), thumbnails: BTreeMap::new() };
        assert_eq!(old.url_for(1200).as_deref(), Some("https://ia800.us.archive.org/x.jpg"));
    }

    #[test]
    fn no_front_cover_and_strange_lists() {
        assert_eq!(parse_index(br#"{"images": []}"#).unwrap(), None);
        assert_eq!(parse_index(br#"{"images": [{"front": false, "types": ["Back"], "image": "x"}]}"#).unwrap(), None);
        assert!(matches!(parse_index(b"Service Unavailable"), Err(WebError::BadReply(_))));
        let elsewhere = FrontImage {
            image: "https://evil.example/archive.org/x.jpg".into(),
            thumbnails: BTreeMap::from([("1200".into(), "file:///etc/passwd".into())]),
        };
        assert_eq!(elsewhere.url_for(1200), None, "only the archives' own servers are asked");
    }

    #[test]
    fn fetches_the_releases_own_cover() {
        let image = format!("{ARCHIVE}/release/{RELEASE}/1-1200.jpg");
        let fake = FakeWeb::default()
            .reply(&index_url(CoverOf::Release, RELEASE), Ok(INDEX.as_bytes()))
            .reply(&image, Ok(b"jpeg"));
        let cover = fetch_front_cover(&fake, &release(Some(GROUP))).unwrap().unwrap();
        assert_eq!((cover.data.as_slice(), cover.from), (&b"jpeg"[..], CoverOf::Release));
        assert_eq!(fake.asked(), [index_url(CoverOf::Release, RELEASE), image], "the group is not asked");
    }

    #[test]
    fn falls_back_to_the_release_group_then_to_nothing() {
        let group_image = "https://archive.org/download/mbid-x/front-1200.jpg";
        let group_index =
            format!(r#"{{"images": [{{"front": true, "image": "x", "thumbnails": {{"1200": "{group_image}"}}}}]}}"#);
        let fake = FakeWeb::default()
            .reply(&index_url(CoverOf::ReleaseGroup, GROUP), Ok(group_index.as_bytes()))
            .reply(group_image, Ok(b"png"));
        let cover = fetch_front_cover(&fake, &release(Some(GROUP))).unwrap().unwrap();
        assert_eq!(cover.from, CoverOf::ReleaseGroup);
        assert_eq!(fetch_front_cover(&FakeWeb::default(), &release(None)).unwrap(), None);
        assert_eq!(fetch_front_cover(&FakeWeb::default(), &release(Some(GROUP))).unwrap(), None);
    }

    #[test]
    fn errors_are_reported_and_bad_ids_refused() {
        let busy = FakeWeb::default().reply(&index_url(CoverOf::Release, RELEASE), Err(WebError::Busy));
        assert_eq!(fetch_front_cover(&busy, &release(Some(GROUP))), Err(WebError::Busy));
        assert_eq!(busy.asked().len(), 1, "a failure is not hidden by trying the group");
        let fake = FakeWeb::default();
        let bad = ReleaseRef { release: "../admin".into(), release_group: None };
        assert!(matches!(fetch_front_cover(&fake, &bad), Err(WebError::BadReply(_))));
        assert!(fake.asked().is_empty());
    }

    #[test]
    fn thumbnails_fall_back_to_the_group() {
        let fake = FakeWeb::default().reply(&thumbnail_url(CoverOf::ReleaseGroup, GROUP), Ok(b"thumb"));
        let thumb = fetch_thumbnail(&fake, &release(Some(GROUP))).unwrap().unwrap();
        assert_eq!(thumb.from, CoverOf::ReleaseGroup);
        assert_eq!(
            fake.asked(),
            [format!("{ARCHIVE}/release/{RELEASE}/front-250"), format!("{ARCHIVE}/release-group/{GROUP}/front-250")]
        );
    }
}
