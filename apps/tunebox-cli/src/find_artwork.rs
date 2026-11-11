//! `artwork find` and `artwork fetch`: album covers from MusicBrainz and the Cover Art Archive.
//!
//! `find` lists the releases (editions) that match the album of the songs found by a search;
//! `fetch` puts the front cover of the one chosen, by its id, into those songs. Nothing is
//! applied by `find`, and `fetch` applies only the release named.

use crate::{print_notices, search_ids, wait_for_jobs};
use tunebox_engine::model::TrackId;
use tunebox_engine::{ReleaseRef, SearchState, Session, Source};

/// What to search for: the album of the songs matching `search`, or the words given.
pub(crate) struct Terms {
    pub search: String,
    pub artist: Option<String>,
    pub album: Option<String>,
}

/// Lists the releases matching the album, best first, with what tells them apart.
pub(crate) fn find(session: &mut Session, terms: &Terms) -> Result<(), String> {
    let ids = songs(session, &terms.search)?;
    search(session, &ids, terms)?;
    let Some(SearchState::Found(found)) = session.artwork_search().map(|s| s.state.clone()) else {
        return Ok(());
    };
    for candidate in &found {
        let release = &candidate.release;
        println!("{}  {} — {}", release.id, release.artist, release.title);
        let details = release.details();
        if !details.is_empty() {
            println!("{:38}{details}", "");
        }
    }
    println!(
        "Put a cover into the songs with `tunebox-cli artwork fetch \"{}\" --release <id>`.",
        terms.search.replace('"', "\\\"")
    );
    Ok(())
}

/// Fetches the front cover of `release` and puts it into the songs, waiting for both.
pub(crate) fn fetch(session: &mut Session, terms: &Terms, release: &str) -> Result<(), String> {
    let ids = songs(session, &terms.search)?;
    // The search says which release group the release belongs to, for when it has no cover of
    // its own. A release it did not find, or a search that failed, is still tried on its own.
    if let Err(message) = search(session, &ids, terms) {
        eprintln!("{message}. Trying the release's own cover only.");
    }
    let group = match session.artwork_search().map(|s| &s.state) {
        Some(SearchState::Found(found)) => {
            found.iter().find(|c| c.release.id == release).and_then(|c| c.release.release_group_id.clone())
        }
        _ => None,
    };
    session.close_artwork_search();
    session.set_network_mounts(tunebox_platform::network_mounts());
    session.check_files_now();
    let release = ReleaseRef { release: release.to_owned(), release_group: group };
    session.use_release_artwork(&ids, &release).map_err(|e| e.to_string())?;
    wait_for_jobs(session);
    Ok(())
}

fn songs(session: &Session, search: &str) -> Result<Vec<TrackId>, String> {
    let ids = search_ids(session.view(), Source::Library, Some(search));
    if ids.is_empty() {
        return Err(format!("no song matches “{search}”"));
    }
    Ok(ids)
}

/// Searches MusicBrainz and waits. Fails when the search does.
fn search(session: &mut Session, ids: &[TrackId], terms: &Terms) -> Result<(), String> {
    let (artist, album) = session.album_search_terms(ids);
    let artist = terms.artist.clone().unwrap_or(artist);
    let album = terms.album.clone().unwrap_or(album);
    eprintln!(
        "Searching MusicBrainz for “{album}”{}…",
        if artist.is_empty() { String::new() } else { format!(" by {artist}") }
    );
    session.find_album_artwork(ids, &artist, &album, false).map_err(|e| e.to_string())?;
    wait_for_jobs(session);
    print_notices(session);
    match session.artwork_search().map(|s| &s.state) {
        Some(SearchState::Failed(message)) => Err(message.trim_end_matches('.').to_owned()),
        Some(SearchState::Found(found)) if found.is_empty() => {
            Err(format!("MusicBrainz knows no release called “{album}”; try --album and --artist"))
        }
        _ => Ok(()),
    }
}
