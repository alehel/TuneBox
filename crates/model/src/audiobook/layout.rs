//! What building the book writes: its chapters as runs of trimmed track audio, where the files
//! go, and how they are tagged.

use super::book::{Book, BookDetails, TrackRef};
use crate::organize::component;
use crate::{NumberOf, TagField, TrackMeta};
use std::path::PathBuf;

/// The most chapters one MP4 file holds: the Nero chapter list counts them in one byte, and
/// craft-media writes both lists the same.
pub const MAX_CHAPTERS_PER_FILE: usize = 255;

/// The longest chapter name, in bytes, for the same reason.
pub const MAX_CHAPTER_NAME_BYTES: usize = 255;

/// A run of one track's audio: samples `from..to` of `track`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
    pub track: TrackRef,
    pub from: u64,
    pub to: u64,
}

impl Part {
    pub fn samples(&self) -> u64 {
        self.to.saturating_sub(self.from)
    }
}

/// A chapter of the built book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputChapter {
    /// From 1.
    pub number: usize,
    pub name: String,
    /// Where it starts in the whole book, in samples.
    pub start: u64,
    pub samples: u64,
    /// Its audio, in order: the kept tracks, trimmed.
    pub parts: Vec<Part>,
}

impl Book {
    /// The chapters the build writes, with their place in the book. Chapters with no kept
    /// track are left out.
    pub fn output_chapters(&self) -> Vec<OutputChapter> {
        let mut start = 0;
        let mut out = Vec::new();
        for chapter in self.chapters() {
            let Some(number) = chapter.number else { continue };
            let parts: Vec<Part> = chapter
                .tracks
                .iter()
                .filter(|t| self.is_kept(**t))
                .filter_map(|t| {
                    let data = self.track(*t)?;
                    let trim = self.trim(*t);
                    let to = data.samples.saturating_sub(trim.end);
                    Some(Part { track: *t, from: trim.start.min(to), to })
                })
                .collect();
            let samples = parts.iter().map(Part::samples).sum();
            out.push(OutputChapter { number, name: chapter.name, start, samples, parts });
            start += samples;
        }
        out
    }
}

/// Where a single-file book goes, relative to the media folder: `Author/Title.m4b`.
pub fn single_file_path(details: &BookDetails, extension: &str) -> PathBuf {
    let mut path = PathBuf::from(component(&details.author, "Unknown Author"));
    path.push(format!("{}.{extension}", component(&details.title, "Untitled Audiobook")));
    path
}

/// Where chapter `number` of `total` goes: `Author/Title/03 Name.ext`, numbered with as many
/// digits as the last chapter needs (at least two).
pub fn chapter_file_path(details: &BookDetails, number: usize, total: usize, name: &str, extension: &str) -> PathBuf {
    let width = total.to_string().len().max(2);
    let mut path = PathBuf::from(component(&details.author, "Unknown Author"));
    path.push(component(&details.title, "Untitled Audiobook"));
    path.push(format!("{number:0width$} {}.{extension}", component(name, "Chapter")));
    path
}

/// The tags of a single-file book. The narrator goes in Composer, as audiobook players show
/// it; the series in Grouping; the description in Comments.
pub fn book_meta(details: &BookDetails) -> TrackMeta {
    let mut meta = TrackMeta {
        title: details.title.clone(),
        artist: details.author.clone(),
        album_artist: details.author.clone(),
        album: details.title.clone(),
        genre: details.genre.clone(),
        composer: details.narrator.clone(),
        grouping: details.series_label(),
        comments: details.description.clone(),
        year: details.year,
        ..TrackMeta::default()
    };
    if !details.publisher.trim().is_empty() {
        meta.extra.insert(TagField::Label, details.publisher.clone());
    }
    meta
}

/// The tags of chapter `number` of `total`, built one file per chapter: the book is the album,
/// the chapter the title and track number. No disc number: chapters run across discs.
pub fn chapter_meta(details: &BookDetails, number: usize, total: usize, name: &str) -> TrackMeta {
    let as_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    TrackMeta { title: name.to_owned(), track: NumberOf::new(as_u32(number), as_u32(total)), ..book_meta(details) }
}

#[cfg(test)]
mod tests {
    use super::super::book::Trim;
    use super::super::book::tests::book;
    use super::*;
    use std::path::Path;

    #[test]
    fn chapters_are_runs_of_kept_trimmed_audio() {
        let mut book = book(&[&[10, 1], &[2, 20]]);
        let t = TrackRef::new;
        book.set_removed(&[t(1, 2), t(2, 1)], true).unwrap();
        book.set_trim(t(1, 1), Trim { start: 0, end: 44_100 }).unwrap();
        book.join(&[t(1, 1), t(2, 2)]).unwrap();
        book.split(&[t(2, 2)]).unwrap();
        book.set_trim(t(2, 2), Trim { start: 22_050, end: 0 }).unwrap();
        let chapters = book.output_chapters();
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].parts, vec![Part { track: t(1, 1), from: 0, to: 9 * 44_100 }]);
        assert_eq!(chapters[1].start, 9 * 44_100);
        assert_eq!(chapters[1].parts, vec![Part { track: t(2, 2), from: 22_050, to: 20 * 44_100 }]);
        assert_eq!(chapters[1].samples, 20 * 44_100 - 22_050);
        assert_eq!(book.kept_samples(), chapters.iter().map(|c| c.samples).sum::<u64>());
    }

    #[test]
    fn chapters_with_no_kept_track_are_not_written() {
        let mut book = book(&[&[10], &[1], &[10]]);
        book.set_removed(&[TrackRef::new(2, 1)], true).unwrap();
        let chapters = book.output_chapters();
        assert_eq!(chapters.iter().map(|c| c.number).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(chapters[1].name, "Chapter 2");
    }

    #[test]
    fn files_go_under_author_and_title() {
        let details = BookDetails {
            title: "The Long Road: Home".into(),
            author: "Margaret Ellison".into(),
            ..BookDetails::default()
        };
        assert_eq!(single_file_path(&details, "m4b"), Path::new("Margaret Ellison/The Long Road_ Home.m4b"));
        assert_eq!(
            chapter_file_path(&details, 7, 31, "The Funeral", "flac"),
            Path::new("Margaret Ellison/The Long Road_ Home/07 The Funeral.flac")
        );
        assert_eq!(chapter_file_path(&details, 7, 312, "X", "m4b").file_name().unwrap(), "007 X.m4b");
        assert_eq!(
            single_file_path(&BookDetails::default(), "m4b"),
            Path::new("Unknown Author/Untitled Audiobook.m4b")
        );
    }

    #[test]
    fn tags_put_the_narrator_in_composer() {
        let details = BookDetails {
            title: "The Long Road Home".into(),
            author: "Margaret Ellison".into(),
            narrator: "David Ashcombe".into(),
            series: "The Harbour Trilogy".into(),
            series_number: Some(2),
            publisher: "Harbourside Audio".into(),
            ..BookDetails::default()
        };
        let meta = chapter_meta(&details, 3, 31, "Low Water");
        assert_eq!((meta.title.as_str(), meta.album.as_str()), ("Low Water", "The Long Road Home"));
        assert_eq!(meta.composer, "David Ashcombe");
        assert_eq!(meta.grouping, "The Harbour Trilogy, 2");
        assert_eq!(meta.genre, "Audiobook");
        assert_eq!(meta.track, NumberOf::new(3, 31));
        assert_eq!(meta.extra(TagField::Label), "Harbourside Audio");
        assert_eq!(book_meta(&details).track, NumberOf::default());
    }
}
