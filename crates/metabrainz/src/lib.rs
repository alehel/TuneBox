//! tunebox-metabrainz — every request TuneBox makes to the internet.
//!
//! TuneBox asks two MetaBrainz services for help, and only when the listener asks for it (or
//! has turned on an automatic setting that is off by default):
//!
//! - **MusicBrainz** (musicbrainz.org): a CD's names by its disc id (`tunebox-rip` builds that
//!   request), and releases matching an album's artist and title ([`search`]).
//! - **The Cover Art Archive** (coverartarchive.org, which hands the pictures out from
//!   archive.org): a release's front cover ([`cover_art`]).
//!
//! Every request goes through a [`WebClient`], so the engine can be tested with a fake one and
//! frontends can show what a request would do without the network. [`HttpClient`] is the real
//! one: it names TuneBox in its user agent, keeps to MusicBrainz's one request per second, gives
//! up after a timeout and refuses replies larger than the caller allows. [`CachingClient`]
//! remembers what was fetched for the session, so going back and forth does not ask again.
//! Nothing about the listener is sent: only the disc id, or the artist and album being looked
//! for, and the release ids MusicBrainz itself returned.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod cache;
mod client;
pub mod cover_art;
#[cfg(feature = "http")]
mod http;
mod mbid;
pub mod search;

pub use cache::CachingClient;
pub use client::{NoNetwork, WebClient, WebError};
pub use cover_art::{Cover, CoverOf, ReleaseRef};
#[cfg(feature = "http")]
pub use http::HttpClient;
pub use mbid::is_mbid;
pub use search::ReleaseCandidate;

/// The MusicBrainz web service.
pub const MUSICBRAINZ: &str = "https://musicbrainz.org/ws/2";

/// How TuneBox introduces itself, as MusicBrainz asks every application to.
pub const USER_AGENT: &str = concat!("TuneBox/", env!("CARGO_PKG_VERSION"), " ( https://github.com/alehel/tunebox )");

/// The client frontends use unless told otherwise: the real one when TuneBox is built with
/// network access, else one that says it has none.
pub fn default_client() -> std::sync::Arc<dyn WebClient> {
    #[cfg(feature = "http")]
    return std::sync::Arc::new(HttpClient::new());
    #[cfg(not(feature = "http"))]
    return std::sync::Arc::new(NoNetwork);
}
