//! tunebox-tags — metadata in and out of audio files.
//!
//! [`read`] turns a file into a [`TrackMeta`] and [`FileInfo`]; [`write`] puts edited metadata
//! back, and [`read_artwork`] / [`write_artwork`] handle the cover picture. The tag formats
//! (ID3v2, MP4 atoms, Vorbis comments, RIFF INFO…) are lofty's business; this crate maps them
//! onto TuneBox's model and nothing else.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use lofty::config::WriteOptions;
use lofty::file::FileType;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::{Accessor, AudioFile, ItemKey, TagExt, TaggedFileExt};
use lofty::tag::Tag;
use std::path::Path;
use tunebox_model::{AudioFormat, FileInfo, NumberOf, TrackMeta};

/// Why a file's tags could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum TagError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not a supported audio file: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Read(#[from] lofty::error::FileParseError),
    #[error("{0}")]
    Write(#[from] lofty::error::FileEncodingError),
}

/// What [`read`] learns about a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadTags {
    pub meta: TrackMeta,
    pub file: FileInfo,
}

/// A cover picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artwork {
    /// "image/jpeg" or "image/png".
    pub mime: String,
    pub data: Vec<u8>,
}

/// Reads a file's metadata and audio properties.
pub fn read(path: &Path) -> Result<ReadTags, TagError> {
    let tagged = lofty::read_from_path(path)?;
    let properties = tagged.properties();
    let format = audio_format(tagged.file_type(), properties.bit_depth())
        .ok_or_else(|| TagError::Unsupported(format!("{:?}", tagged.file_type())))?;
    let file = FileInfo {
        format,
        duration: properties.duration(),
        size_bytes: std::fs::metadata(path)?.len(),
        bit_rate_kbps: properties.audio_bitrate().or(properties.overall_bitrate()),
        sample_rate: properties.sample_rate(),
        channels: properties.channels(),
    };
    let meta = tagged.primary_tag().or_else(|| tagged.first_tag()).map(meta_from_tag).unwrap_or_default();
    Ok(ReadTags { meta, file })
}

/// Maps lofty's file type to ours. MP4 files with a bit depth hold Apple Lossless; AAC has none.
fn audio_format(file_type: FileType, bit_depth: Option<u8>) -> Option<AudioFormat> {
    Some(match file_type {
        FileType::Mpeg => AudioFormat::Mp3,
        FileType::Aac => AudioFormat::Aac,
        FileType::Mp4 if bit_depth.is_some() => AudioFormat::Alac,
        FileType::Mp4 => AudioFormat::Aac,
        FileType::Flac => AudioFormat::Flac,
        FileType::Wav => AudioFormat::Wav,
        FileType::Aiff => AudioFormat::Aiff,
        FileType::Vorbis => AudioFormat::Vorbis,
        FileType::Opus => AudioFormat::Opus,
        _ => return None,
    })
}

fn meta_from_tag(tag: &Tag) -> TrackMeta {
    let text = |key: ItemKey| tag.get_string(key).unwrap_or_default().trim().to_owned();
    let number = |key: ItemKey| tag.get_string(key).and_then(|s| s.trim().parse::<u32>().ok());
    TrackMeta {
        title: tag.title().unwrap_or_default().trim().to_owned(),
        artist: tag.artist().unwrap_or_default().trim().to_owned(),
        album_artist: text(ItemKey::AlbumArtist),
        album: tag.album().unwrap_or_default().trim().to_owned(),
        genre: tag.genre().unwrap_or_default().trim().to_owned(),
        composer: text(ItemKey::Composer),
        grouping: text(ItemKey::ContentGroup),
        comments: tag.comment().unwrap_or_default().trim().to_owned(),
        year: tag.date().map(|d| u32::from(d.year)).filter(|y| *y > 0),
        track: NumberOf { number: tag.track(), total: tag.track_total() },
        disc: NumberOf { number: tag.disk(), total: tag.disk_total() },
        bpm: number(ItemKey::IntegerBpm).or_else(|| number(ItemKey::Bpm)),
        compilation: matches!(text(ItemKey::FlagCompilation).as_str(), "1" | "true" | "True"),
    }
}

/// Writes `meta` into the file's main tag, creating the tag if the file has none. Fields left
/// empty in `meta` are removed from the file. Tag items TuneBox does not know are kept.
pub fn write(path: &Path, meta: &TrackMeta) -> Result<(), TagError> {
    let mut tag = load_primary_tag(path)?;
    apply_meta(&mut tag, meta);
    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

fn load_primary_tag(path: &Path) -> Result<Tag, TagError> {
    let tagged = lofty::read_from_path(path)?;
    let tag_type = tagged.primary_tag_type();
    Ok(tagged.primary_tag().cloned().unwrap_or_else(|| Tag::new(tag_type)))
}

fn apply_meta(tag: &mut Tag, meta: &TrackMeta) {
    set_text(tag, ItemKey::TrackTitle, &meta.title);
    set_text(tag, ItemKey::TrackArtist, &meta.artist);
    set_text(tag, ItemKey::AlbumArtist, &meta.album_artist);
    set_text(tag, ItemKey::AlbumTitle, &meta.album);
    set_text(tag, ItemKey::Genre, &meta.genre);
    set_text(tag, ItemKey::Composer, &meta.composer);
    set_text(tag, ItemKey::ContentGroup, &meta.grouping);
    set_text(tag, ItemKey::Comment, &meta.comments);
    set_number(tag, ItemKey::Year, meta.year);
    set_number(tag, ItemKey::TrackNumber, meta.track.number);
    set_number(tag, ItemKey::TrackTotal, meta.track.total);
    set_number(tag, ItemKey::DiscNumber, meta.disc.number);
    set_number(tag, ItemKey::DiscTotal, meta.disc.total);
    set_bpm(tag, meta.bpm);
    set_text(tag, ItemKey::FlagCompilation, if meta.compilation { "1" } else { "" });
}

fn set_text(tag: &mut Tag, key: ItemKey, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        tag.remove_key(key);
    } else {
        tag.insert_text(key, value.to_owned());
    }
}

fn set_number(tag: &mut Tag, key: ItemKey, value: Option<u32>) {
    set_text(tag, key, &value.map(|v| v.to_string()).unwrap_or_default());
}

/// ID3v2 and MP4 have an integer BPM item, Vorbis comments only a text one: write whichever the
/// tag format supports.
fn set_bpm(tag: &mut Tag, bpm: Option<u32>) {
    tag.remove_key(ItemKey::IntegerBpm);
    tag.remove_key(ItemKey::Bpm);
    if let Some(bpm) = bpm
        && !tag.insert_text(ItemKey::IntegerBpm, bpm.to_string())
    {
        tag.insert_text(ItemKey::Bpm, bpm.to_string());
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
    let mut tag = load_primary_tag(path)?;
    tag.remove_picture_type(PictureType::CoverFront);
    let picture = Picture::unchecked(artwork.data.clone())
        .pic_type(PictureType::CoverFront)
        .mime_type(MimeType::from_str(&artwork.mime))
        .build();
    tag.push_picture(picture);
    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

/// The image type from its first bytes, for pictures stored without one.
fn sniff_mime(data: &[u8]) -> &'static str {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_audio::{AudioBuffer, EncodeFormat, EncodeOptions};

    /// One second of silence as a FLAC file.
    fn flac_file(dir: &Path) -> std::path::PathBuf {
        let buffer = AudioBuffer::new(44_100, 2, 44_100);
        let bytes = craft_audio::encode(&buffer, &EncodeOptions::new(EncodeFormat::Flac)).unwrap();
        let path = dir.join("silence.flac");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn writes_and_reads_back_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = flac_file(dir.path());
        let meta = TrackMeta {
            title: "Lighthouse".into(),
            artist: "The Harbour Lights".into(),
            album_artist: "The Harbour Lights".into(),
            album: "Coastline".into(),
            genre: "Folk".into(),
            composer: "M. Keeper".into(),
            year: Some(2011),
            track: NumberOf::new(4, 11),
            disc: NumberOf::new(1, 1),
            bpm: Some(96),
            compilation: true,
            ..TrackMeta::default()
        };
        write(&path, &meta).unwrap();
        let back = read(&path).unwrap();
        assert_eq!(back.meta, meta);
        assert_eq!(back.file.format, AudioFormat::Flac);
        assert_eq!(back.file.sample_rate, Some(44_100));
        assert_eq!(back.file.duration.as_secs(), 1);
    }

    #[test]
    fn clearing_a_field_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = flac_file(dir.path());
        write(&path, &TrackMeta { title: "A".into(), genre: "Rock".into(), ..TrackMeta::default() }).unwrap();
        write(&path, &TrackMeta { title: "A".into(), ..TrackMeta::default() }).unwrap();
        assert_eq!(read(&path).unwrap().meta.genre, "");
    }

    #[test]
    fn artwork_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = flac_file(dir.path());
        assert_eq!(read_artwork(&path).unwrap(), None);
        let art = Artwork { mime: "image/png".into(), data: vec![0x89, b'P', b'N', b'G', 1, 2, 3] };
        write_artwork(&path, &art).unwrap();
        assert_eq!(read_artwork(&path).unwrap(), Some(art));
    }

    #[test]
    fn garbage_is_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.mp3");
        std::fs::write(&path, b"this is not audio at all").unwrap();
        assert!(read(&path).is_err());
    }
}
