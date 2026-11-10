//! The additional tag fields ([`TagField`]) and where each lives in a tag. lofty translates
//! its generic keys into each format's own: ID3v2 frames, MP4 atoms, Vorbis comment names.

use lofty::prelude::ItemKey;
use lofty::tag::{Tag, TagType};
use std::collections::BTreeMap;
use tunebox_model::{AudioFormat, TagField};

/// Whether files of `format` have a place of its own for `field`. Not every tag format has
/// every field (MP4 has no arranger, Vorbis comments no composer sort name), and some keep two
/// fields in one frame (ID3v2 lists arranger, producer and engineer together). An edit the file
/// cannot hold, or that would overwrite another field, must not look as if it was saved.
pub fn can_store(format: AudioFormat, field: TagField) -> bool {
    tag_types(format).iter().all(|tag_type| has_own_place(*tag_type, field))
}

/// The name of the frame, atom or comment that holds `field`, if the tag has one.
fn frame_name(tag_type: TagType, field: TagField) -> Option<&'static str> {
    item_key(field, tag_type).map_key(tag_type)
}

fn has_own_place(tag_type: TagType, field: TagField) -> bool {
    frame_name(tag_type, field).is_some_and(|name| {
        TagField::ALL.into_iter().filter(|other| frame_name(tag_type, *other) == Some(name)).count() == 1
    })
}

/// The key lofty files the field under when it reads a file. Usually the key it was written
/// with, but where two keys share a frame (ID3v2's TPUB is both label and publisher), lofty
/// reads it back as the first.
fn read_key(tag_type: TagType, field: TagField) -> ItemKey {
    frame_name(tag_type, field)
        .and_then(|name| ItemKey::from_key(tag_type, name))
        .unwrap_or_else(|| item_key(field, tag_type))
}

/// The tag TuneBox writes for each format: the one lofty treats as the format's main tag.
fn tag_types(format: AudioFormat) -> &'static [TagType] {
    match format {
        AudioFormat::Mp3 | AudioFormat::Wav | AudioFormat::Aiff => &[TagType::Id3v2],
        // "AAC" is either a bare AAC stream (ID3v2) or AAC in MP4: allow what both can hold.
        AudioFormat::Aac => &[TagType::Id3v2, TagType::Mp4Ilst],
        AudioFormat::Alac => &[TagType::Mp4Ilst],
        AudioFormat::Flac | AudioFormat::Vorbis | AudioFormat::Opus => &[TagType::VorbisComments],
    }
}

/// lofty's key for `field` in a tag of `tag_type`.
fn item_key(field: TagField, tag_type: TagType) -> ItemKey {
    match field {
        // ID3v2 keeps lyrics in a USLT frame, which lofty reaches only through this key.
        TagField::Lyrics if tag_type == TagType::Id3v2 => ItemKey::UnsyncLyrics,
        TagField::Subtitle => ItemKey::TrackSubtitle,
        TagField::Lyricist => ItemKey::Lyricist,
        TagField::Conductor => ItemKey::Conductor,
        TagField::Arranger => ItemKey::Arranger,
        TagField::Remixer => ItemKey::Remixer,
        TagField::Producer => ItemKey::Producer,
        TagField::Engineer => ItemKey::Engineer,
        TagField::Performer => ItemKey::Performer,
        TagField::Work => ItemKey::Work,
        TagField::Movement => ItemKey::Movement,
        TagField::MovementNumber => ItemKey::MovementNumber,
        TagField::MovementTotal => ItemKey::MovementTotal,
        TagField::Label => ItemKey::Label,
        TagField::CatalogNumber => ItemKey::CatalogNumber,
        TagField::Barcode => ItemKey::Barcode,
        TagField::ReleaseDate => ItemKey::ReleaseDate,
        TagField::OriginalReleaseDate => ItemKey::OriginalReleaseDate,
        TagField::MediaType => ItemKey::OriginalMediaType,
        TagField::Copyright => ItemKey::CopyrightMessage,
        TagField::Isrc => ItemKey::Isrc,
        TagField::SortName => ItemKey::TrackTitleSortOrder,
        TagField::SortArtist => ItemKey::TrackArtistSortOrder,
        TagField::SortAlbum => ItemKey::AlbumTitleSortOrder,
        TagField::SortAlbumArtist => ItemKey::AlbumArtistSortOrder,
        TagField::SortComposer => ItemKey::ComposerSortOrder,
        TagField::Mood => ItemKey::Mood,
        TagField::Key => ItemKey::InitialKey,
        TagField::Language => ItemKey::Language,
        TagField::Lyrics => ItemKey::Lyrics,
        TagField::EncodedBy => ItemKey::EncodedBy,
        TagField::EncoderSettings => ItemKey::EncoderSettings,
    }
}

/// The additional fields the tag has a value for.
pub(crate) fn read(tag: &Tag) -> BTreeMap<TagField, String> {
    TagField::ALL
        .into_iter()
        .filter_map(|field| {
            let value = tag.get_string(read_key(tag.tag_type(), field))?.trim();
            (!value.is_empty()).then(|| (field, value.to_owned()))
        })
        .collect()
}

/// Writes the fields in `extra` that the tag has a place for; an empty value removes the field.
/// Fields not in `extra` are left as they are in the file, and so is a field whose new value
/// does not suit it: a malformed date frame can make the whole file unreadable.
pub(crate) fn apply(tag: &mut Tag, extra: &BTreeMap<TagField, String>) {
    for (field, value) in extra {
        if field.accepts(value) && has_own_place(tag.tag_type(), *field) {
            crate::set_text(tag, item_key(*field, tag.tag_type()), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_format_offers_the_fields_it_has_a_place_of_its_own_for() {
        // ID3v2 lists arranger, producer and engineer in one frame, so none can be edited alone.
        assert!(!can_store(AudioFormat::Mp3, TagField::Arranger));
        assert!(!can_store(AudioFormat::Mp3, TagField::Producer));
        assert!(can_store(AudioFormat::Mp3, TagField::Lyrics));
        assert!(can_store(AudioFormat::Mp3, TagField::Label));
        assert!(!can_store(AudioFormat::Alac, TagField::Arranger));
        assert!(can_store(AudioFormat::Alac, TagField::Work));
        assert!(can_store(AudioFormat::Flac, TagField::Arranger));
        assert!(!can_store(AudioFormat::Flac, TagField::SortComposer));
        // Bare AAC and AAC in MP4 differ; only what both hold is offered.
        assert!(!can_store(AudioFormat::Aac, TagField::EncoderSettings));
        for format in AudioFormat::ALL {
            let stored = TagField::ALL.into_iter().filter(|f| can_store(format, *f)).count();
            assert!(stored >= 20, "{format:?} offers only {stored} fields");
        }
    }
}
