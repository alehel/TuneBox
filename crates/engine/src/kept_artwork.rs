//! Covers TuneBox keeps itself, for songs whose files it may not change.
//!
//! A song in a read-only library folder cannot have a cover written into it. When the listener
//! gives one anyway, TuneBox keeps the picture in its own folder beside the library file
//! ("TuneBox Artwork") and shows it as the song's cover, just as edits to such songs are kept in
//! the library (`Track::library_only_edits`). The listener's files are never touched.
//!
//! Covers are kept per album ([`AlbumKey`]: filing artist and album name), the way the interface
//! shows them, so one kept picture serves every read-only song of the album. A kept cover is
//! shown only for songs TuneBox may not write: a song it may write carries its cover in its
//! own file. Each picture is one file named by a stable hash of its album, written atomically.

use crate::files;
use crate::prefs::Preferences;
use std::path::{Path, PathBuf};
use tunebox_model::Track;
use tunebox_tags::Artwork;

/// An album, as covers are grouped: its filing artist and its name.
pub type AlbumKey = (String, String);

/// The album `track` belongs to, for covers.
pub fn album_key(track: &Track) -> AlbumKey {
    (track.meta.filing_artist().to_owned(), track.meta.album.clone())
}

/// The folder of kept covers.
#[derive(Clone, Debug)]
pub(crate) struct KeptArtwork {
    folder: PathBuf,
}

impl KeptArtwork {
    /// The kept covers of the library at `library_file`, in a folder beside it.
    pub fn beside(library_file: &Path) -> Self {
        let parent = library_file.parent().unwrap_or(Path::new(""));
        KeptArtwork { folder: parent.join("TuneBox Artwork") }
    }

    /// Keeps `artwork` as the cover of `album`, replacing any kept before.
    pub fn put(&self, album: &AlbumKey, artwork: &Artwork) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.folder)?;
        let base = self.base(album);
        craft_atomic::atomic_write(&base.with_extension(artwork.extension()), &artwork.data)?;
        // A JPEG replacing a PNG (or the reverse) must not leave the old one to be found.
        let other = if artwork.extension() == "png" { "jpg" } else { "png" };
        remove_if_there(&base.with_extension(other))
    }

    /// Forgets the kept cover of `album`, if there is one.
    pub fn remove(&self, album: &AlbumKey) -> std::io::Result<()> {
        let base = self.base(album);
        remove_if_there(&base.with_extension("jpg"))?;
        remove_if_there(&base.with_extension("png"))
    }

    /// Where `album`'s cover is kept, without its extension.
    fn base(&self, album: &AlbumKey) -> PathBuf {
        self.folder.join(format!("{:016x}", album_hash(album)))
    }

    /// Where the cover of `track` comes from: TuneBox's kept cover for a song it may not write,
    /// else the song's own file.
    pub fn source(&self, prefs: &Preferences, track: &Track) -> CoverSource {
        let kept = (!files::may_write(prefs, &track.location)).then(|| self.base(&album_key(track)));
        CoverSource { kept, file: track.location.clone() }
    }
}

/// Where to read a song's cover from. It can be sent to another thread, so a frontend can read
/// covers in the background.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverSource {
    /// A kept cover to look for first, without its extension.
    kept: Option<PathBuf>,
    file: PathBuf,
}

impl CoverSource {
    /// Reads the cover. The song's file is read only when TuneBox keeps no cover for it.
    pub fn load(&self) -> Result<Option<Artwork>, String> {
        if let Some(base) = &self.kept {
            for (extension, mime) in [("jpg", "image/jpeg"), ("png", "image/png")] {
                if let Ok(data) = std::fs::read(base.with_extension(extension)) {
                    return Ok(Some(Artwork { mime: mime.to_owned(), data }));
                }
            }
        }
        tunebox_tags::read_artwork(&self.file).map_err(|e| e.to_string())
    }
}

fn remove_if_there(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// FNV-1a over the artist and album. Kept covers are found by this name in later versions too,
/// so it must never change (Rust's own hasher may).
fn album_hash((artist, album): &AlbumKey) -> u64 {
    let bytes = artist.bytes().chain([0xFF]).chain(album.bytes());
    bytes.fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(artist: &str, album: &str) -> AlbumKey {
        (artist.to_owned(), album.to_owned())
    }

    #[test]
    fn the_name_of_a_kept_cover_never_changes() {
        // The value a library's kept covers were saved under; changing the hash loses them.
        assert_eq!(album_hash(&key("The Harbour Lights", "Coastline")), 0x8c1a_bb3a_af1e_c663);
        assert_ne!(album_hash(&key("ab", "c")), album_hash(&key("a", "bc")));
    }

    #[test]
    fn kept_covers_replace_each_other_and_can_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let kept = KeptArtwork::beside(&dir.path().join("Library.json"));
        let album = key("Artist", "Album");
        let source = CoverSource { kept: Some(kept.base(&album)), file: dir.path().join("none.flac") };
        let png = Artwork { mime: "image/png".into(), data: vec![1] };
        let jpeg = Artwork { mime: "image/jpeg".into(), data: vec![2] };
        kept.put(&album, &png).unwrap();
        assert_eq!(source.load(), Ok(Some(png.clone())));
        kept.put(&album, &jpeg).unwrap();
        kept.put(&album, &png).unwrap();
        assert_eq!(source.load(), Ok(Some(png)), "the newest, whatever its type");
        kept.remove(&album).unwrap();
        kept.remove(&album).unwrap();
        assert!(source.load().is_err(), "falls back to the file, which is not there");
    }
}
