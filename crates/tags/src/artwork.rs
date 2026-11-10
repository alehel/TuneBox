//! The cover picture inside an audio file: reading it, setting it and removing it. Changes are
//! made through `rewrite_atomically`, like every other tag edit, so a failure leaves the file as
//! it was.

use crate::{TagError, load_primary_tag, rewrite_atomically};
use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::{TagExt, TaggedFileExt};
use std::path::Path;

/// A cover picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artwork {
    /// "image/jpeg" or "image/png".
    pub mime: String,
    pub data: Vec<u8>,
}

impl Artwork {
    /// The usual file extension for the picture: "jpg" or "png".
    pub fn extension(&self) -> &'static str {
        if self.mime == "image/png" { "png" } else { "jpg" }
    }
}

/// The file's front cover, or its first picture of any kind.
pub fn read_artwork(path: &Path) -> Result<Option<Artwork>, TagError> {
    let tagged = lofty::read_from_path(path)?;
    let pictures = tagged.tags().iter().flat_map(|t| t.pictures());
    let mut best: Option<&Picture> = None;
    for picture in pictures {
        if picture.pic_type() == PictureType::CoverFront {
            best = Some(picture);
            break;
        }
        best = best.or(Some(picture));
    }
    Ok(best.map(|p| Artwork {
        mime: p.mime_type().map(|m| m.as_str().to_owned()).unwrap_or_else(|| sniff_mime(p.data()).to_owned()),
        data: p.data().to_vec(),
    }))
}

/// Sets the file's front cover, replacing any existing one.
pub fn write_artwork(path: &Path, artwork: &Artwork) -> Result<(), TagError> {
    rewrite_atomically(path, |copy| {
        let mut tag = load_primary_tag(copy)?;
        tag.remove_picture_type(PictureType::CoverFront);
        let picture = Picture::unchecked(artwork.data.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::from_str(&artwork.mime))
            .build();
        tag.push_picture(picture);
        tag.save_to_path(copy, WriteOptions::default())?;
        Ok(())
    })
}

/// Removes every picture from the file, in every tag it has. Every kind goes, not only the
/// front cover: [`read_artwork`] shows any picture when there is no front cover, so leaving a
/// back cover behind would leave the song with a cover after "Remove". A file without pictures
/// is not rewritten.
pub fn remove_artwork(path: &Path) -> Result<(), TagError> {
    let has_pictures = lofty::read_from_path(path)?.tags().iter().any(|t| t.picture_count() > 0);
    if !has_pictures {
        return Ok(());
    }
    rewrite_atomically(path, |copy| {
        let tagged = lofty::read_from_path(copy)?;
        for tag in tagged.tags().iter().filter(|t| t.picture_count() > 0) {
            let mut tag = tag.clone();
            while tag.picture_count() > 0 {
                tag.remove_picture(0);
            }
            tag.save_to_path(copy, WriteOptions::default())?;
        }
        Ok(())
    })
}

/// The image type from its first bytes, for pictures stored without one.
fn sniff_mime(data: &[u8]) -> &'static str {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_audio::{AudioBuffer, EncodeFormat, EncodeOptions};

    /// One second of silence in each format whose tags carry pictures differently: FLAC
    /// (picture blocks), WAV (ID3v2 in a RIFF chunk) and AIFF (ID3v2 in an IFF chunk).
    fn songs(dir: &Path) -> Vec<std::path::PathBuf> {
        let buffer = AudioBuffer::new(44_100, 2, 44_100);
        [(EncodeFormat::Flac, "flac"), (EncodeFormat::Wav, "wav"), (EncodeFormat::Aiff, "aiff")]
            .into_iter()
            .map(|(format, extension)| {
                let path = dir.join(format!("silence.{extension}"));
                std::fs::write(&path, craft_audio::encode(&buffer, &EncodeOptions::new(format)).unwrap()).unwrap();
                path
            })
            .collect()
    }

    fn png(marker: u8) -> Artwork {
        Artwork { mime: "image/png".into(), data: vec![0x89, b'P', b'N', b'G', 1, 2, marker] }
    }

    #[test]
    fn artwork_is_set_replaced_and_removed_in_every_format() {
        let dir = tempfile::tempdir().unwrap();
        for path in songs(dir.path()) {
            assert_eq!(read_artwork(&path).unwrap(), None, "{path:?}");
            write_artwork(&path, &png(1)).unwrap();
            assert_eq!(read_artwork(&path).unwrap(), Some(png(1)), "{path:?}");
            write_artwork(&path, &png(2)).unwrap();
            assert_eq!(read_artwork(&path).unwrap(), Some(png(2)), "replaced, not added: {path:?}");
            remove_artwork(&path).unwrap();
            assert_eq!(read_artwork(&path).unwrap(), None, "{path:?}");
            assert!(crate::read(&path).is_ok(), "still a song: {path:?}");
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3, "no temporary copies are left behind");
    }

    #[test]
    fn removing_takes_every_kind_of_picture() {
        let dir = tempfile::tempdir().unwrap();
        let path = songs(dir.path()).remove(0);
        rewrite_atomically(&path, |copy| {
            let mut tag = load_primary_tag(copy)?;
            tag.push_picture(Picture::unchecked(png(3).data).pic_type(PictureType::CoverBack).build());
            tag.save_to_path(copy, WriteOptions::default())?;
            Ok(())
        })
        .unwrap();
        assert!(read_artwork(&path).unwrap().is_some(), "a back cover is shown when there is no front one");
        remove_artwork(&path).unwrap();
        assert_eq!(read_artwork(&path).unwrap(), None);
    }

    #[test]
    fn removing_from_a_file_without_artwork_leaves_it_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = songs(dir.path()).remove(1);
        let before = std::fs::read(&path).unwrap();
        remove_artwork(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_damaged_file_is_an_error_and_stays_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.flac");
        std::fs::write(&path, b"not a song").unwrap();
        assert!(write_artwork(&path, &png(1)).is_err());
        assert!(remove_artwork(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"not a song");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
