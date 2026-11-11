//! tunebox-engine — the façade every frontend talks to.
//!
//! A [`Session`] owns the library, the player, the devices, the CD in the drive and the
//! background jobs. Frontends (the desktop app, the command line) call its methods and read its
//! state; they never reach into the crates below. The session is single-threaded by design:
//! slow work (importing, ripping, syncing, looking up track names and covers) runs as a [`jobs`] job on a
//! worker thread and hands its result back to [`Session::tick`], so the library only ever has
//! one writer.
//!
//! [`commands`] lists every user-facing action with its label, menu and shortcut, so menus,
//! keyboard handling and the command line agree.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod album_artwork;
mod artwork;
mod availability;
mod cd;
pub mod commands;
mod devices;
mod disc_cover;
mod error;
mod file_check;
pub mod files;
pub mod jobs;
mod kept_artwork;
mod library_ops;
mod locate;
mod notices;
mod places;
mod playback;
mod prefs;
mod refresh;
mod rename;
mod session;
mod snapshot;
mod sources;
mod view;
mod watch;

pub use album_artwork::{ArtworkSearch, CoverCandidate, SearchState, Thumbnail};
pub use cd::CdState;
pub use devices::{DeviceEntry, find_mtp_players};
pub use disc_cover::DiscCover;
pub use error::EngineError;
pub use file_check::FileStatus;
pub use jobs::{JobId, JobKind, JobStatus};
pub use kept_artwork::{AlbumKey, CoverSource, album_key};
pub use notices::{Notice, NoticeLevel};
pub use places::Place;
pub use playback::NowPlaying;
pub use prefs::{LibraryFolder, Preferences};
pub use refresh::LibraryOnlyEdits;
pub use rename::{Conflict, Planned, RenameBase, RenameConflicts, RenameItem, RenamePlan, RenameRequest, SkipReason};
pub use session::{Session, SessionPaths};
pub use snapshot::LibrarySnapshot;
pub use sources::{Source, TrackQuery};
pub use view::LibraryView;

// The types frontends need from the layers below, so they depend on the engine alone.
pub use tunebox_audio::LosslessFormat;
pub use tunebox_library::{BrowseFilter, Browser, Library, Summary};
pub use tunebox_metabrainz::{CoverOf, ReleaseCandidate, ReleaseRef, WebClient, WebError, is_mbid};
pub use tunebox_model as model;
pub use tunebox_playback::{AudioTap, NullOutput, PlayState, Player, RepeatMode, analysis};
pub use tunebox_rip::{CdDrive, DiscInfo, DiscToc, DriveIdentity, VirtualDrive, check_read_offset};
pub use tunebox_sync::mtp::{MtpLink, mtp_links, probe as probe_mtp_link};
pub use tunebox_sync::{Access, Candidate, Device, MtpAddress, PlayerModel, SyncMode, SyncSettings, Volume};
pub use tunebox_tags::{Artwork, MAX_COVER_SIDE, can_store};
