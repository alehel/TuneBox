//! Covers from the Cover Art Archive, end to end, with a pretend web: nothing here touches the
//! network. The CD's cover (fetched on request or by the opt-in setting, shown, put into the
//! imported songs) and "Find Album Artwork…" for library albums.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tunebox_audio::{LosslessFormat, PcmWriter};
use tunebox_engine::model::{NumberOf, TrackId, TrackMeta};
use tunebox_engine::{
    CoverOf, DiscCover, NullOutput, Player, ReleaseRef, SearchState, Session, SessionPaths, Source, Thumbnail,
    TrackQuery, VirtualDrive, WebClient, WebError,
};

const RELEASE: &str = "11111111-2222-3333-4444-555555555555";
const GROUP: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const OTHER_RELEASE: &str = "66666666-7777-8888-9999-000000000000";
const ARCHIVE: &str = "https://coverartarchive.org";

/// A reply, for every URL starting with a prefix.
type Route = (String, Result<Vec<u8>, WebError>);

/// A pretend web: replies chosen by the start of the URL, and every URL asked for.
#[derive(Default)]
struct FakeWeb {
    routes: Mutex<Vec<Route>>,
    asked: Mutex<Vec<String>>,
}

impl FakeWeb {
    fn route(&self, prefix: &str, reply: Result<Vec<u8>, WebError>) {
        self.routes.lock().unwrap().insert(0, (prefix.to_owned(), reply));
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl WebClient for FakeWeb {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, WebError> {
        self.asked.lock().unwrap().push(url.to_owned());
        let routes = self.routes.lock().unwrap();
        let reply = routes.iter().find(|(prefix, _)| url.starts_with(prefix.as_str())).map(|(_, r)| r.clone());
        let body = reply.unwrap_or(Err(WebError::NotFound))?;
        if body.len() as u64 > limit {
            return Err(WebError::TooLarge(limit));
        }
        Ok(body)
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    session: Session,
    web: Arc<FakeWeb>,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let paths = SessionPaths {
        library_file: dir.path().join("TuneBox/Library.json"),
        preferences_file: dir.path().join("TuneBox/Preferences.json"),
        default_media_folder: dir.path().join("TuneBox/Media"),
    };
    let mut session = Session::open(paths, Player::with_output(NullOutput { speed: 1.0 })).unwrap();
    let web = Arc::new(FakeWeb::default());
    session.use_web_client(web.clone());
    Fixture { dir, session, web }
}

fn tick_until(session: &mut Session, mut done: impl FnMut(&Session) -> bool) {
    let start = Instant::now();
    while !done(session) {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        session.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn settle(session: &mut Session) {
    tick_until(session, |s| s.jobs().is_empty());
    // Finishing one job can start the next (thumbnails, writing a fetched cover).
    session.tick();
    tick_until(session, |s| s.jobs().is_empty());
}

fn notices_text(session: &mut Session) -> String {
    session.take_notices().into_iter().map(|n| n.text).collect::<Vec<_>>().join(" ")
}

/// A small JPEG picture of one colour.
fn jpeg(colour: [u8; 3]) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(32, 32, image::Rgb(colour));
    let mut data = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut data), image::ImageFormat::Jpeg).unwrap();
    data
}

/// The archive's image list for `id`, whose front cover is at `image`.
fn index(image: &str) -> Vec<u8> {
    format!(r#"{{"images": [{{"front": true, "types": ["Front"], "image": "{image}", "thumbnails": {{"1200": "{image}"}}}}]}}"#)
        .into_bytes()
}

/// A MusicBrainz disc reply naming the disc "Tidewater" by The Lanterns (fictional).
fn disc_reply() -> Vec<u8> {
    format!(
        r#"{{"releases": [{{"id": "{RELEASE}", "release-group": {{"id": "{GROUP}"}}, "title": "Tidewater",
            "date": "2009", "artist-credit": [{{"name": "The Lanterns"}}],
            "media": [{{"position": 1, "tracks": [{{"title": "Low Tide"}}, {{"title": "Breakwater"}}]}}]}}]}}"#
    )
    .into_bytes()
}

/// Serves the disc lookup, and the release's front cover in `colour`.
fn serve_disc_and_cover(web: &FakeWeb, colour: [u8; 3]) -> Vec<u8> {
    let picture = jpeg(colour);
    let image = format!("{ARCHIVE}/release/{RELEASE}/1-1200.jpg");
    web.route("https://musicbrainz.org/ws/2/discid/", Ok(disc_reply()));
    web.route(&format!("{ARCHIVE}/release/{RELEASE}"), Ok(index(&image)));
    web.route(&image, Ok(picture.clone()));
    picture
}

fn insert_disc(f: &mut Fixture) {
    f.session.read_disc(Arc::new(VirtualDrive::new("Virtual Drive", vec![1, 1])));
    tick_until(&mut f.session, |s| s.cd().is_some());
}

fn look_up_names(f: &mut Fixture) {
    f.session.lookup_disc_names().unwrap();
    settle(&mut f.session);
    assert_eq!(f.session.cd().unwrap().info.release_id.as_deref(), Some(RELEASE));
}

fn disc_cover(session: &Session) -> DiscCover {
    session.cd().unwrap().cover.clone()
}

fn cover_of(session: &Session, id: TrackId) -> Option<Vec<u8>> {
    session.cover_source(id)?.load().unwrap().map(|artwork| artwork.data)
}

#[test]
fn nothing_is_requested_unless_the_listener_asks() {
    let mut f = fixture();
    serve_disc_and_cover(&f.web, [10, 20, 30]);
    let mut prefs = f.session.preferences().clone();
    prefs.fetch_cd_covers_automatically = true; // but not the lookup of names
    f.session.set_preferences(prefs);
    insert_disc(&mut f);
    for _ in 0..20 {
        f.session.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(disc_cover(&f.session), DiscCover::NotAsked);
    assert!(f.web.asked().is_empty(), "asked: {:?}", f.web.asked());
    assert!(f.session.fetch_disc_cover().unwrap_err().to_string().contains("Get Track Names first"));
    assert!(f.web.asked().is_empty());
}

#[test]
fn the_discs_cover_is_shown_before_importing_and_goes_into_the_imported_songs() {
    let mut f = fixture();
    let picture = serve_disc_and_cover(&f.web, [200, 40, 90]);
    insert_disc(&mut f);
    look_up_names(&mut f);
    assert_eq!(disc_cover(&f.session), DiscCover::NotAsked, "looking up names does not fetch the cover");
    assert_eq!(f.web.asked().len(), 1);

    f.session.fetch_disc_cover().unwrap();
    settle(&mut f.session);
    let DiscCover::Found { artwork, from } = disc_cover(&f.session) else { panic!("{:?}", disc_cover(&f.session)) };
    assert_eq!((artwork.data, from), (picture.clone(), CoverOf::Release), "a fitting JPEG is kept as served");

    f.session.import_cd().unwrap();
    settle(&mut f.session);
    let songs: Vec<TrackId> = f.session.query(&TrackQuery::new(Source::Music));
    assert_eq!(songs.len(), 2);
    for id in songs {
        assert_eq!(cover_of(&f.session, id), Some(picture.clone()));
    }
    // Fetching again is answered from what was fetched this session.
    let asked = f.web.asked().len();
    f.session.fetch_disc_cover().unwrap();
    settle(&mut f.session);
    assert_eq!(f.web.asked().len(), asked);
}

#[test]
fn a_cover_that_arrives_after_importing_goes_into_the_imported_songs() {
    let mut f = fixture();
    let picture = serve_disc_and_cover(&f.web, [10, 120, 60]);
    insert_disc(&mut f);
    look_up_names(&mut f);
    f.session.import_cd().unwrap();
    settle(&mut f.session);
    let songs: Vec<TrackId> = f.session.query(&TrackQuery::new(Source::Music));
    assert!(songs.iter().all(|id| cover_of(&f.session, *id).is_none()));
    f.session.fetch_disc_cover().unwrap();
    settle(&mut f.session);
    for id in songs {
        assert_eq!(cover_of(&f.session, id), Some(picture.clone()));
    }
}

#[test]
fn with_the_setting_on_the_cover_follows_the_lookup() {
    let mut f = fixture();
    serve_disc_and_cover(&f.web, [90, 90, 200]);
    let mut prefs = f.session.preferences().clone();
    prefs.lookup_cd_names_automatically = true;
    prefs.fetch_cd_covers_automatically = true;
    f.session.set_preferences(prefs);
    insert_disc(&mut f);
    settle(&mut f.session);
    assert!(matches!(disc_cover(&f.session), DiscCover::Found { .. }), "{:?}", disc_cover(&f.session));
}

fn looked_up_disc() -> Fixture {
    let mut f = fixture();
    f.web.route("https://musicbrainz.org/ws/2/discid/", Ok(disc_reply()));
    insert_disc(&mut f);
    look_up_names(&mut f);
    f.session.take_notices();
    f
}

#[test]
fn failures_are_reported_and_can_be_tried_again() {
    let mut f = looked_up_disc();
    for (error, words) in [
        (WebError::Busy, "busy right now"),
        (WebError::TimedOut, "took too long"),
        (WebError::Offline("the server's name could not be looked up".into()), "Check the internet connection"),
        (WebError::TooLarge(30_000_000), "larger than 30 MB"),
    ] {
        f.web.route(&format!("{ARCHIVE}/release/"), Err(error));
        f.session.fetch_disc_cover().unwrap();
        settle(&mut f.session);
        assert!(matches!(disc_cover(&f.session), DiscCover::Failed(ref m) if m.contains(words)));
        let text = notices_text(&mut f.session);
        assert!(text.starts_with("Could not get the cover of “Tidewater”") && text.contains(words), "{text}");
    }
    // Failures are not remembered: the next try asks again, and this time finds it.
    let image = format!("{ARCHIVE}/release/{RELEASE}/1-1200.jpg");
    f.web.route(&format!("{ARCHIVE}/release/{RELEASE}"), Ok(index(&image)));
    f.web.route(&image, Ok(jpeg([1, 2, 3])));
    f.session.fetch_disc_cover().unwrap();
    settle(&mut f.session);
    assert!(matches!(disc_cover(&f.session), DiscCover::Found { .. }));
}

#[test]
fn a_disc_without_a_cover_has_none_said_quietly() {
    let mut f = looked_up_disc();
    // 404 for the release and for its release group.
    f.session.fetch_disc_cover().unwrap();
    settle(&mut f.session);
    assert_eq!(disc_cover(&f.session), DiscCover::NoneFound);
    let notices = f.session.take_notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].level, tunebox_engine::NoticeLevel::Info);
    assert!(notices[0].text.contains("has no cover for “Tidewater”"), "{}", notices[0].text);
    assert_eq!(
        f.web.asked()[1..],
        [format!("{ARCHIVE}/release/{RELEASE}"), format!("{ARCHIVE}/release-group/{GROUP}")]
    );
}

#[test]
fn a_reply_that_is_not_a_picture_is_refused() {
    let mut f = looked_up_disc();
    let image = format!("{ARCHIVE}/release/{RELEASE}/1-1200.jpg");
    f.web.route(&format!("{ARCHIVE}/release/{RELEASE}"), Ok(index(&image)));
    f.web.route(&image, Ok(b"<html>Service Unavailable</html>".to_vec()));
    f.session.fetch_disc_cover().unwrap();
    settle(&mut f.session);
    assert!(matches!(disc_cover(&f.session), DiscCover::Failed(ref m) if m.contains("cannot use")));
}

/// A tagged FLAC of half a second.
fn write_song(path: &Path, title: &str, artist: &str, album: &str, track: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut writer = PcmWriter::create(path, LosslessFormat::Flac, 8_000, 1).unwrap();
    writer.write(&[vec![0.1; 4_000]]).unwrap();
    writer.finish().unwrap();
    let meta = TrackMeta {
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        track: NumberOf::new(track, 2),
        ..TrackMeta::default()
    };
    tunebox_tags::write(path, &meta).unwrap();
}

fn import_album(f: &mut Fixture) -> Vec<TrackId> {
    let downloads = f.dir.path().join("Downloads");
    write_song(&downloads.join("a.flac"), "Low Tide", "The Lanterns", "Tidewater", 1);
    write_song(&downloads.join("b.flac"), "Breakwater", "The Lanterns", "Tidewater", 2);
    f.session.import(vec![downloads]).unwrap();
    tick_until(&mut f.session, |s| s.library().len() == 2);
    f.session.query(&TrackQuery::new(Source::Music))
}

/// Two editions of "Tidewater": the first with its own cover, the second without (its release
/// group has one).
fn serve_search(web: &FakeWeb) -> (Vec<u8>, Vec<u8>) {
    let reply = format!(
        r#"{{"releases": [
            {{"id": "{RELEASE}", "score": 100, "title": "Tidewater", "artist-credit": [{{"name": "The Lanterns"}}],
              "release-group": {{"id": "{GROUP}"}}, "date": "2009", "country": "GB", "track-count": 2,
              "media": [{{"format": "CD"}}]}},
            {{"id": "{OTHER_RELEASE}", "score": 90, "title": "Tidewater", "artist-credit": [{{"name": "The Lanterns"}}],
              "release-group": {{"id": "{GROUP}"}}, "date": "2010", "country": "US", "track-count": 2,
              "media": [{{"format": "12\" Vinyl"}}]}}
        ]}}"#
    );
    web.route("https://musicbrainz.org/ws/2/release/?query=", Ok(reply.into_bytes()));
    let (own, group) = (jpeg([250, 200, 0]), jpeg([0, 100, 250]));
    web.route(&format!("{ARCHIVE}/release/{RELEASE}/front-250"), Ok(own.clone()));
    web.route(&format!("{ARCHIVE}/release-group/{GROUP}/front-250"), Ok(group.clone()));
    (own, group)
}

fn found(session: &Session) -> Vec<tunebox_engine::CoverCandidate> {
    match &session.artwork_search().unwrap().state {
        SearchState::Found(candidates) => candidates.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn finding_album_artwork_shows_the_editions_and_applies_only_the_chosen_cover() {
    let mut f = fixture();
    let songs = import_album(&mut f);
    let (own, group_thumb) = serve_search(&f.web);
    assert!(f.web.asked().is_empty(), "importing asks nothing");

    let (artist, album) = f.session.album_search_terms(&songs);
    assert_eq!((artist.as_str(), album.as_str()), ("The Lanterns", "Tidewater"));
    f.session.find_album_artwork(&songs, &artist, &album, true).unwrap();
    settle(&mut f.session);
    let candidates = found(&f.session);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].release.details(), "2009 · GB · CD · 2 tracks");
    assert_eq!(candidates[0].thumbnail, Thumbnail::Found { data: Arc::new(own), from: CoverOf::Release });
    assert_eq!(
        candidates[1].thumbnail,
        Thumbnail::Found { data: Arc::new(group_thumb), from: CoverOf::ReleaseGroup },
        "an edition without a cover shows the album's"
    );
    assert!(songs.iter().all(|id| cover_of(&f.session, *id).is_none()), "nothing applied before a choice");

    // Searching again (back and forth in the picker) asks nothing new.
    let asked = f.web.asked().len();
    f.session.find_album_artwork(&songs, &artist, &album, true).unwrap();
    settle(&mut f.session);
    assert_eq!(f.web.asked().len(), asked);

    let picture = jpeg([30, 160, 90]);
    let image = format!("{ARCHIVE}/release/{RELEASE}/1-1200.jpg");
    f.web.route(&format!("{ARCHIVE}/release/{RELEASE}"), Ok(index(&image)));
    f.web.route(&image, Ok(picture.clone()));
    f.session.use_found_artwork(RELEASE).unwrap();
    settle(&mut f.session);
    for id in &songs {
        assert_eq!(cover_of(&f.session, *id), Some(picture.clone()));
    }
    assert!(f.session.use_found_artwork("not-found").is_err());
}

#[test]
fn a_release_without_a_cover_of_its_own_gets_the_albums() {
    let mut f = fixture();
    let songs = import_album(&mut f);
    let picture = jpeg([120, 0, 120]);
    let image = "https://archive.org/download/mbid-x/front-1200.jpg";
    f.web.route(&format!("{ARCHIVE}/release-group/{GROUP}"), Ok(index(image)));
    f.web.route(image, Ok(picture.clone()));
    let release = ReleaseRef { release: OTHER_RELEASE.into(), release_group: Some(GROUP.into()) };
    f.session.use_release_artwork(&songs, &release).unwrap();
    settle(&mut f.session);
    assert!(notices_text(&mut f.session).contains("uses the album's cover from another edition"));
    assert_eq!(cover_of(&f.session, songs[0]), Some(picture));

    // No cover anywhere: nothing changes, and the listener is told.
    let none = ReleaseRef { release: RELEASE.into(), release_group: None };
    f.web.route(&format!("{ARCHIVE}/release/{RELEASE}"), Err(WebError::NotFound));
    f.session.use_release_artwork(&songs, &none).unwrap();
    settle(&mut f.session);
    assert!(notices_text(&mut f.session).contains("has no cover for"));
    assert!(f.session.use_release_artwork(&songs, &ReleaseRef { release: "x".into(), release_group: None }).is_err());
}

#[test]
fn a_failed_search_says_why_and_nothing_is_applied() {
    let mut f = fixture();
    let songs = import_album(&mut f);
    f.web.route("https://musicbrainz.org/ws/2/release/", Err(WebError::Busy));
    f.session.find_album_artwork(&songs, "The Lanterns", "Tidewater", true).unwrap();
    settle(&mut f.session);
    let SearchState::Failed(message) = &f.session.artwork_search().unwrap().state else { panic!() };
    assert!(message.contains("Could not search MusicBrainz: the service is busy"), "{message}");
    assert!(f.session.find_album_artwork(&songs, "x", "  ", true).is_err(), "an album name is needed");
    assert!(f.session.use_found_artwork(RELEASE).is_err());
    f.session.close_artwork_search();
    assert!(f.session.artwork_search().is_none());
}
