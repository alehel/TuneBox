//! MP4's media kind (the `stik` atom), which tells players a file is an audiobook.
//!
//! lofty's common tag has no item for it, so it is read and written through the MP4 tag
//! itself, and [`crate::write`] carries it across edits made through the common tag. Other
//! formats have no standard tag for a media kind: there it lives in the library only.

use crate::{TagError, rewrite_atomically};
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, FileType};
use lofty::mp4::{Atom, AtomData, AtomIdent, DataType, Ilst, Mp4File};
use lofty::prelude::TagExt;
use std::fs::File;
use std::path::Path;
use tunebox_model::MediaKind;

const STIK: AtomIdent<'static> = AtomIdent::Fourcc(*b"stik");
/// The values players use: 1 is music ("normal"), 2 an audiobook.
const MUSIC: u8 = 1;
const AUDIOBOOK: u8 = 2;

/// Marks the file as `kind`. Only MP4 files can hold it; for others this does nothing.
pub fn write_media_kind(path: &Path, kind: MediaKind) -> Result<(), TagError> {
    if !is_mp4(path)? {
        return Ok(());
    }
    rewrite_atomically(path, |copy| write_atom(copy, kind))
}

/// The kind an MP4 file declares; `None` for other files, unreadable ones and MP4 files that
/// declare none.
pub(crate) fn read(path: &Path, file_type: FileType) -> Option<MediaKind> {
    if file_type != FileType::Mp4 {
        return None;
    }
    read_atom(path).ok().flatten()
}

/// The kind an MP4 file declares, `None` when the file is not MP4 or declares none.
pub(crate) fn read_atom(path: &Path) -> Result<Option<MediaKind>, TagError> {
    if !is_mp4(path)? {
        return Ok(None);
    }
    let file = Mp4File::read_from(&mut File::open(path)?, ParseOptions::new())?;
    let value =
        file.ilst().and_then(|ilst| ilst.get(&STIK)).and_then(|atom| atom.data().next()).and_then(|data| match data {
            AtomData::SignedInteger(value) => Some(*value),
            AtomData::UnsignedInteger(value) => i32::try_from(*value).ok(),
            AtomData::Unknown { data, .. } => data.last().map(|b| i32::from(*b)),
            _ => None,
        });
    Ok(value.map(|v| if v == i32::from(AUDIOBOOK) { MediaKind::Audiobook } else { MediaKind::Music }))
}

/// Writes the atom as Apple's software does: one byte, typed as a signed integer.
pub(crate) fn write_atom(path: &Path, kind: MediaKind) -> Result<(), TagError> {
    let file = Mp4File::read_from(&mut File::open(path)?, ParseOptions::new())?;
    let mut ilst = file.ilst().cloned().unwrap_or_else(Ilst::default);
    let value = match kind {
        MediaKind::Music => MUSIC,
        MediaKind::Audiobook => AUDIOBOOK,
    };
    ilst.replace_atom(Atom::new(STIK, AtomData::Unknown { code: DataType::BeSignedInteger, data: vec![value] }));
    ilst.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

fn is_mp4(path: &Path) -> Result<bool, TagError> {
    let probe = lofty::probe::Probe::open(path)?.guess_file_type()?;
    Ok(probe.file_type() == Some(FileType::Mp4))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunebox_audio::{Mp4AudioWriter, Mp4Chapter, Mp4Codec, Mp4Settings, read_mp4_chapters};
    use tunebox_model::TrackMeta;

    /// Two seconds of Apple Lossless with two chapters.
    fn mp4(path: &Path) {
        let settings = Mp4Settings {
            codec: Mp4Codec::AppleLossless,
            sample_rate: 44_100,
            channels: 2,
            frames: 88_200,
            chapters: vec![
                Mp4Chapter { start: 0, title: "One".into() },
                Mp4Chapter { start: 44_100, title: "Two".into() },
            ],
        };
        let mut writer = Mp4AudioWriter::create(path, &settings).unwrap();
        writer.write(&[vec![0.1; 88_200], vec![0.1; 88_200]]).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn m4b_files_are_audiobooks_and_the_mark_is_written_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let book = dir.path().join("book.m4b");
        mp4(&book);
        assert_eq!(crate::read(&book).unwrap().kind, MediaKind::Audiobook, "by its name");

        let song = dir.path().join("song.m4a");
        mp4(&song);
        assert_eq!(crate::read(&song).unwrap().kind, MediaKind::Music);
        write_media_kind(&song, MediaKind::Audiobook).unwrap();
        assert_eq!(crate::read(&song).unwrap().kind, MediaKind::Audiobook, "by its mark");

        // An ordinary tag edit keeps the mark and the chapters.
        let meta =
            TrackMeta { title: "The Long Road Home".into(), composer: "David Ashcombe".into(), ..TrackMeta::default() };
        crate::write(&song, &meta).unwrap();
        let read = crate::read(&song).unwrap();
        assert_eq!(read.kind, MediaKind::Audiobook);
        assert_eq!(read.meta.composer, "David Ashcombe");
        let titles: Vec<String> = read_mp4_chapters(&song).unwrap().into_iter().map(|(_, t)| t).collect();
        assert_eq!(titles, ["One", "Two"]);
        // So does adding a cover.
        let mut png = Vec::new();
        image::RgbImage::new(2, 2).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        crate::write_artwork(&song, &crate::Artwork { mime: "image/png".into(), data: png }).unwrap();
        assert_eq!(crate::read(&song).unwrap().kind, MediaKind::Audiobook);
        assert!(crate::read_artwork(&song).unwrap().is_some());

        write_media_kind(&song, MediaKind::Music).unwrap();
        assert_eq!(crate::read(&song).unwrap().kind, MediaKind::Music);
    }

    #[test]
    fn other_formats_have_no_mark_to_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.wav");
        let buffer = craft_audio::AudioBuffer::new(44_100, 2, 4410);
        let bytes =
            craft_audio::encode(&buffer, &craft_audio::EncodeOptions::new(craft_audio::EncodeFormat::Wav)).unwrap();
        std::fs::write(&path, bytes).unwrap();
        write_media_kind(&path, MediaKind::Audiobook).unwrap();
        assert_eq!(crate::read(&path).unwrap().kind, MediaKind::Music);
    }
}
