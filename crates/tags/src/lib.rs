//! tunebox-tags — metadata in and out of audio files.
//!
//! [`read`] turns a file into a [`TrackMeta`] and [`FileInfo`]; [`write`] puts edited metadata
//! back. [`write_media_kind`] marks an MP4 file as an audiobook (`media_kind`). The cover picture is `artwork`'s ([`read_artwork`], [`write_artwork`],
//! [`remove_artwork`]), and `picture` turns an image the listener chose into one fit to be a
//! cover. The tag formats (ID3v2, MP4 atoms, Vorbis comments, RIFF INFO…) are lofty's business;
//! this crate maps them onto TuneBox's model and nothing else.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod artwork;
mod fields;
mod media_kind;
mod picture;

pub use artwork::{Artwork, read_artwork, remove_artwork, write_artwork};
pub use fields::can_store;
pub use media_kind::write_media_kind;
pub use picture::{MAX_COVER_SIDE, PictureError, looks_like_image, prepare_cover};

use lofty::config::WriteOptions;
use lofty::file::FileType;
use lofty::prelude::{Accessor, AudioFile, ItemKey, TagExt, TaggedFileExt};
use lofty::tag::Tag;
use std::path::Path;
use tunebox_model::{AudioFormat, FileInfo, MediaKind, NumberOf, TrackMeta};

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
    /// An audiobook when the file says so (MP4's media kind) or is named `.m4b`.
    pub kind: MediaKind,
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
    let kind = match media_kind::read(path, tagged.file_type()) {
        Some(kind) => kind,
        None => MediaKind::from_path(path),
    };
    Ok(ReadTags { meta, file, kind })
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
        extra: fields::read(tag),
    }
}

/// Writes `meta` into the file's main tag, creating the tag if the file has none. Fields left
/// empty in `meta` are removed from the file; additional fields absent from `meta.extra` are
/// left as they are. Tag items TuneBox does not know are kept.
///
/// The edit is made on a copy next to the file, which then replaces it: if anything fails, or
/// the computer stops half-way, the original file is untouched.
pub fn write(path: &Path, meta: &TrackMeta) -> Result<(), TagError> {
    rewrite_atomically(path, |copy| {
        let mut tag = load_primary_tag(copy)?;
        apply_meta(&mut tag, meta);
        tag.save_to_path(copy, WriteOptions::default())?;
        Ok(())
    })
}

/// Runs `edit` on a copy of `path` and moves the copy over the original only when it succeeds.
/// The copy is removed on failure, so nothing is left behind. An MP4 file's media kind is kept
/// unless the edit sets one: lofty's common tag has no place for it, so saving that tag would
/// drop an audiobook's mark.
pub(crate) fn rewrite_atomically(
    path: &Path,
    edit: impl FnOnce(&Path) -> Result<(), TagError>,
) -> Result<(), TagError> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // Hidden, and keeping the extension: lofty recognises the format by it.
    let copy = path.with_file_name(format!(".tunebox-edit-{name}"));
    let result = std::fs::copy(path, &copy)
        .map_err(TagError::from)
        .and_then(|_| {
            let kind = media_kind::read_atom(&copy)?;
            edit(&copy)?;
            if let Some(kind) = kind
                && media_kind::read_atom(&copy)?.is_none()
            {
                media_kind::write_atom(&copy, kind)?;
            }
            Ok(())
        })
        .and_then(|()| Ok(std::fs::File::open(&copy)?.sync_all()?))
        .and_then(|()| Ok(std::fs::rename(&copy, path)?));
    if result.is_err() {
        // The copy is ours and incomplete; failing to remove it changes nothing for the caller.
        let _ = std::fs::remove_file(&copy);
    }
    result
}

pub(crate) fn load_primary_tag(path: &Path) -> Result<Tag, TagError> {
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
    fields::apply(tag, &meta.extra);
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

#[cfg(test)]
mod tests {
    use super::*;
    use craft_audio::{AudioBuffer, EncodeFormat, EncodeOptions};
    use tunebox_model::{FieldKind, TagField};

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

    /// One second of silence in `format`, written by craft-audio.
    fn audio_file(dir: &Path, format: EncodeFormat, extension: &str) -> std::path::PathBuf {
        let buffer = AudioBuffer::new(44_100, 2, 44_100);
        let path = dir.join(format!("silence.{extension}"));
        std::fs::write(&path, craft_audio::encode(&buffer, &EncodeOptions::new(format)).unwrap()).unwrap();
        path
    }

    #[test]
    fn every_field_a_format_can_store_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let files = [
            (AudioFormat::Flac, audio_file(dir.path(), EncodeFormat::Flac, "flac")),
            (AudioFormat::Wav, audio_file(dir.path(), EncodeFormat::Wav, "wav")),
            (AudioFormat::Aiff, audio_file(dir.path(), EncodeFormat::Aiff, "aiff")),
        ];
        for (format, path) in files {
            let mut meta = TrackMeta { title: "Nocturne".into(), ..TrackMeta::default() };
            for field in TagField::ALL.into_iter().filter(|f| can_store(format, *f)) {
                let value = match field.kind() {
                    FieldKind::Lines => "Line one\nLine two".to_owned(),
                    FieldKind::Number => "3".to_owned(),
                    FieldKind::Date => "2011-04-02".to_owned(),
                    FieldKind::Text => format!("{} value", field.label()),
                };
                meta.extra.insert(field, value);
            }
            assert!(meta.extra.len() > 20, "{format:?} stores only {} fields", meta.extra.len());
            write(&path, &meta).unwrap();
            assert_eq!(read(&path).unwrap().meta, meta, "{format:?}");
        }
    }

    #[test]
    fn additional_fields_not_in_the_edit_are_kept_and_empty_ones_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let path = flac_file(dir.path());
        let mut first = TrackMeta { title: "A".into(), ..TrackMeta::default() };
        first.extra.insert(TagField::Conductor, "C. Baton".into());
        first.extra.insert(TagField::Label, "Harbour Records".into());
        write(&path, &first).unwrap();

        // An edit that knows nothing of the extra fields, as for a song added before TuneBox
        // read them, must not lose them.
        write(&path, &TrackMeta { title: "B".into(), ..TrackMeta::default() }).unwrap();
        let mut clear_label = TrackMeta { title: "B".into(), ..TrackMeta::default() };
        clear_label.extra.insert(TagField::Label, String::new());
        write(&path, &clear_label).unwrap();

        let back = read(&path).unwrap().meta;
        assert_eq!(back.extra(TagField::Conductor), "C. Baton");
        assert_eq!(back.extra(TagField::Label), "");
        assert_eq!(back.title, "B");
    }

    #[test]
    fn a_value_unfit_for_its_field_is_never_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = audio_file(dir.path(), EncodeFormat::Wav, "wav");
        let mut meta = TrackMeta { title: "A".into(), ..TrackMeta::default() };
        meta.extra.insert(TagField::ReleaseDate, "2011-04-02".into());
        write(&path, &meta).unwrap();
        meta.extra.insert(TagField::ReleaseDate, "Release date value".into());
        write(&path, &meta).unwrap();
        // Before the check, this left an ID3v2 date frame lofty could no longer parse.
        assert_eq!(read(&path).unwrap().meta.extra(TagField::ReleaseDate), "2011-04-02");
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
    fn a_failed_edit_leaves_the_file_and_folder_as_they_were() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.mp3");
        std::fs::write(&path, b"this is not audio at all").unwrap();
        assert!(write(&path, &TrackMeta { title: "x".into(), ..TrackMeta::default() }).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"this is not audio at all");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary copy is left behind");
    }

    #[test]
    fn garbage_is_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.mp3");
        std::fs::write(&path, b"this is not audio at all").unwrap();
        assert!(read(&path).is_err());
    }
}
