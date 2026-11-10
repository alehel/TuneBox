//! Demo mode (`tunebox --demo <folder>`): the library made by `cargo xtask demo`, a virtual CD
//! in a virtual drive, a virtual player, and a pretend MusicBrainz and Cover Art Archive
//! (`demo_web`), so every part of TuneBox can be tried, and screenshotted, without hardware or
//! the network.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tunebox_engine::{DiscInfo, Session, VirtualDrive, Volume};

pub struct Demo {
    dir: PathBuf,
    disc_named: bool,
}

/// The made-up album on the virtual CD.
const DISC_ALBUM: &str = "Lighthouse Keeper";
const DISC_ARTIST: &str = "The Tidewater Band";
const DISC_TRACKS: [(&str, u32); 9] = [
    ("Fog Signal", 214),
    ("North Light", 187),
    ("Salt on the Glass", 243),
    ("The Long Watch", 301),
    ("Shipping Forecast Waltz", 176),
    ("Keeper's Log", 222),
    ("Spring Tide", 198),
    ("Lantern Room", 265),
    ("Morning Relief", 239),
];

impl Demo {
    /// Inserts the virtual disc, and answers web requests from the pretend services.
    pub fn start(dir: &Path, session: &mut Session) -> Self {
        let cover = |album: &str| {
            let track = session.library().tracks().find(|t| t.meta.album == album)?;
            session.cover_source(track.id)?.load().ok()?.map(|artwork| artwork.data)
        };
        let titles = DISC_TRACKS.iter().map(|(title, _)| (*title).to_owned()).collect();
        let web = crate::demo_web::DemoWeb::new(
            (DISC_ALBUM.into(), DISC_ARTIST.into(), titles),
            cover("Paper Boats").unwrap_or_default(),
            cover("Glass Garden").unwrap_or_default(),
            cover("Low Tide Sessions").unwrap_or_default(),
        );
        session.use_web_client(Arc::new(web));
        let lengths = DISC_TRACKS.iter().map(|(_, seconds)| *seconds).collect();
        session.read_disc(Arc::new(VirtualDrive::new("Virtual Drive", lengths)));
        Self { dir: dir.to_path_buf(), disc_named: false }
    }

    /// The virtual player, as a mounted volume.
    pub fn player_volume(&self) -> Volume {
        Volume {
            root: self.dir.join("player"),
            label: "POCKET".into(),
            total_bytes: 15_900_000_000,
            free_bytes: 15_210_000_000,
            removable: true,
            access: tunebox_engine::Access::Folder,
        }
    }

    /// Names the disc once it has been read, as a lookup would.
    pub fn name_disc(&mut self, session: &mut Session) {
        if self.disc_named {
            return;
        }
        let Some(cd) = session.cd() else { return };
        let mut info = DiscInfo::placeholder(&cd.toc);
        info.album = DISC_ALBUM.into();
        info.artist = DISC_ARTIST.into();
        info.genre = "Folk".into();
        info.year = Some(2011);
        for (slot, (title, _)) in info.tracks.iter_mut().zip(DISC_TRACKS) {
            slot.0 = title.into();
        }
        session.set_disc_info(info);
        self.disc_named = true;
    }
}
