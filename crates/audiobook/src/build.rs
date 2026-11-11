//! Building the book: its chapters, read from the staged tracks, written in the chosen format.
//!
//! A build is all or nothing: if anything fails, or it is cancelled, the files it wrote are
//! removed and the book stays as it was, ready to build again. Nothing it writes replaces an
//! existing file: a name already taken gets a number, as imports do.

use crate::output::Output;
use crate::source::{SourceError, read_part};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tunebox_audio::{Mp4Chapter, mono_mix};
use tunebox_library::NewTrack;
use tunebox_model::audiobook::layout::{book_meta, chapter_file_path, chapter_meta, single_file_path};
use tunebox_model::audiobook::{Book, BookError, MAX_CHAPTERS_PER_FILE, OutputChapter, OutputLayout, TrackRef};
use tunebox_model::{MediaKind, TrackMeta};
use tunebox_tags::Artwork;

/// What to build, and from what.
#[derive(Clone, Debug)]
pub struct BuildRequest {
    pub book: Book,
    /// The staged file of every ripped track.
    pub staged: BTreeMap<TrackRef, PathBuf>,
    pub media_folder: PathBuf,
    /// The cover to put in every file.
    pub cover: Option<Artwork>,
}

/// Where a build has got to.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildProgress {
    /// The chapter being written, from 1, and how many there are.
    pub chapter: usize,
    pub chapters: usize,
    /// Through the whole book, 0–1.
    pub fraction: f32,
}

/// Why a book could not be built. The messages are shown to the listener as they are.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("The book has no tracks to build. Restore some tracks, or rip a disc first.")]
    Empty,
    #[error("{} not ripped yet: {}. Rip {} first, or remove {} from the book.", tracks_word(.0), list(.0), discs_word(.0), them(.0))]
    NotRipped(Vec<TrackRef>),
    #[error(
        "This book has {0} chapters; one file holds at most {MAX_CHAPTERS_PER_FILE}. Join some chapters, or build one file per chapter."
    )]
    TooManyChapters(usize),
    #[error("{0}")]
    Settings(BookError),
    #[error("{0}")]
    Source(#[from] SourceError),
    #[error("Could not write {}: {reason}", .path.display())]
    Write { path: PathBuf, reason: String },
    #[error("The build was stopped. Nothing was added.")]
    Cancelled,
}

fn tracks_word(tracks: &[TrackRef]) -> &'static str {
    if tracks.len() == 1 { "A track is" } else { "Some tracks are" }
}

fn discs_word(tracks: &[TrackRef]) -> String {
    let mut discs: Vec<u16> = tracks.iter().map(|t| t.disc).collect();
    discs.dedup();
    match discs.as_slice() {
        [disc] => format!("disc {disc}"),
        _ => "those discs".to_owned(),
    }
}

fn them(tracks: &[TrackRef]) -> &'static str {
    if tracks.len() == 1 { "it" } else { "them" }
}

fn list(tracks: &[TrackRef]) -> String {
    let shown: Vec<String> = tracks.iter().take(8).map(TrackRef::to_string).collect();
    let more = tracks.len().saturating_sub(shown.len());
    if more > 0 { format!("{} and {more} more", shown.join(", ")) } else { shown.join(", ") }
}

/// Whether `book` can be built as it is.
pub fn check(book: &Book) -> Result<(), BuildError> {
    book.output.check().map_err(BuildError::Settings)?;
    let unripped = book.unripped_kept();
    if !unripped.is_empty() {
        return Err(BuildError::NotRipped(unripped));
    }
    let chapters = book.output_chapters();
    if chapters.is_empty() || book.kept_samples() == 0 {
        return Err(BuildError::Empty);
    }
    if book.output.layout == OutputLayout::SingleFile && chapters.len() > MAX_CHAPTERS_PER_FILE {
        return Err(BuildError::TooManyChapters(chapters.len()));
    }
    Ok(())
}

/// Builds the book. Returns the files written, tagged, as tracks for the library.
pub fn build(
    request: &BuildRequest,
    now: DateTime<Utc>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(BuildProgress),
) -> Result<Vec<NewTrack>, BuildError> {
    check(&request.book)?;
    let mut written: Vec<PathBuf> = Vec::new();
    let result = build_files(request, now, cancel, progress, &mut written);
    if result.is_err() {
        for path in &written {
            // Ours, and incomplete as a book: removing it leaves the media folder as it was.
            if let Err(e) = std::fs::remove_file(path) {
                log::warn!("could not remove {} after a failed build: {e}", path.display());
            }
        }
    }
    result
}

fn build_files(
    request: &BuildRequest,
    now: DateTime<Utc>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(BuildProgress),
    written: &mut Vec<PathBuf>,
) -> Result<Vec<NewTrack>, BuildError> {
    let book = &request.book;
    let chapters = book.output_chapters();
    let total = book.kept_samples().max(1);
    let extension = book.output.format.extension();
    let mut done: u64 = 0;
    let mut report = |chapter: usize, done: u64| {
        progress(BuildProgress { chapter, chapters: chapters.len(), fraction: done as f32 / total as f32 })
    };
    let mut tracks = Vec::new();
    match book.output.layout {
        OutputLayout::SingleFile => {
            let path = target(&request.media_folder, &single_file_path(&book.details, extension))?;
            let marks = chapters.iter().map(|c| Mp4Chapter { start: c.start, title: c.name.clone() }).collect();
            let mut output =
                Output::create(&path, &book.output, book.kept_samples(), marks).map_err(|e| write_error(&path, e))?;
            for chapter in &chapters {
                if let Err(e) =
                    write_chapter(request, chapter, &mut output, cancel, &mut |n| report(chapter.number, done + n))
                {
                    output.abandon();
                    return Err(e);
                }
                done += chapter.samples;
            }
            let path = output.finish().map_err(|e| write_error(&path, e))?;
            written.push(path.clone());
            tracks.push(finish_file(&path, &book_meta(&book.details), request.cover.as_ref(), now)?);
        }
        OutputLayout::PerChapter => {
            let count = chapters.len();
            for chapter in &chapters {
                let relative = chapter_file_path(&book.details, chapter.number, count, &chapter.name, extension);
                let path = target(&request.media_folder, &relative)?;
                let mut output = Output::create(&path, &book.output, chapter.samples, Vec::new())
                    .map_err(|e| write_error(&path, e))?;
                if let Err(e) =
                    write_chapter(request, chapter, &mut output, cancel, &mut |n| report(chapter.number, done + n))
                {
                    output.abandon();
                    return Err(e);
                }
                done += chapter.samples;
                let path = output.finish().map_err(|e| write_error(&path, e))?;
                written.push(path.clone());
                let meta = chapter_meta(&book.details, chapter.number, count, &chapter.name);
                tracks.push(finish_file(&path, &meta, request.cover.as_ref(), now)?);
            }
        }
    }
    report(chapters.len(), total);
    Ok(tracks)
}

/// Writes one chapter's parts to `output`, reporting the samples written so far.
fn write_chapter(
    request: &BuildRequest,
    chapter: &OutputChapter,
    output: &mut Output,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> Result<(), BuildError> {
    let mono = request.book.output.is_mono();
    let mut written: u64 = 0;
    for part in &chapter.parts {
        let length = request.book.track(part.track).map_or(0, |t| t.samples);
        let path = request.staged.get(&part.track).ok_or(BuildError::NotRipped(vec![part.track]))?;
        read_part(path, part.track, length, part.from, part.to, &mut |block| {
            if cancel.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let result = if mono { output.write(&[mono_mix(block)]) } else { output.write(block) };
            result.map_err(|e| e.to_string())?;
            written += block.first().map_or(0, Vec::len) as u64;
            progress(written);
            Ok(true)
        })?;
        if cancel.load(Ordering::Relaxed) {
            return Err(BuildError::Cancelled);
        }
    }
    Ok(())
}

/// The full path for `relative` in the media folder, with its folder made, never an existing file.
fn target(media_folder: &Path, relative: &Path) -> Result<PathBuf, BuildError> {
    let wanted = media_folder.join(relative);
    if let Some(parent) = wanted.parent() {
        std::fs::create_dir_all(parent).map_err(|e| write_error(parent, e))?;
    }
    Ok(tunebox_import::unique_path(&wanted))
}

/// Tags a finished file as an audiobook and describes it for the library.
fn finish_file(
    path: &Path,
    meta: &TrackMeta,
    cover: Option<&Artwork>,
    now: DateTime<Utc>,
) -> Result<NewTrack, BuildError> {
    let tag_error = |e: tunebox_tags::TagError| write_error(path, e);
    tunebox_tags::write(path, meta).map_err(tag_error)?;
    tunebox_tags::write_media_kind(path, MediaKind::Audiobook).map_err(tag_error)?;
    if let Some(cover) = cover {
        tunebox_tags::write_artwork(path, cover).map_err(tag_error)?;
    }
    let file = tunebox_tags::read(path).map_err(tag_error)?.file;
    Ok(NewTrack { location: path.to_path_buf(), meta: meta.clone(), file, date_added: now, kind: MediaKind::Audiobook })
}

fn write_error(path: &Path, error: impl std::fmt::Display) -> BuildError {
    BuildError::Write { path: path.to_path_buf(), reason: error.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunebox_audio::{Decoder, LosslessFormat, PcmWriter, read_mp4_chapters};
    use tunebox_model::audiobook::{BookDetails, BookDisc, BookTrack, OutputFormat, OutputSettings, SAMPLE_RATE, Trim};

    /// A distinct, recognisable sample for every position of every track.
    fn sample(track: TrackRef, frame: u64) -> f32 {
        let base = f32::from(track.disc) * 0.1 + f32::from(track.track) * 0.01;
        base + (frame % 100) as f32 / 10_000.0
    }

    /// A book of discs with these track lengths in frames, every track staged.
    fn staged_book(dir: &Path, discs: &[&[u64]]) -> BuildRequest {
        let mut book = Book::new(
            BookDetails {
                title: "The Long Road Home".into(),
                author: "Margaret Ellison".into(),
                narrator: "David Ashcombe".into(),
                ..BookDetails::default()
            },
            vec![],
        );
        let mut staged = BTreeMap::new();
        for (index, lengths) in discs.iter().enumerate() {
            let disc = index as u16 + 1;
            let mut tracks = Vec::new();
            for (position, frames) in lengths.iter().enumerate() {
                let number = position as u8 + 1;
                let track = TrackRef::new(disc, number);
                let path = dir.join(format!("disc-{disc:02}-track-{number:02}.flac"));
                let audio: Vec<f32> = (0..*frames).map(|f| sample(track, f)).collect();
                let mut writer = PcmWriter::create(&path, LosslessFormat::Flac, SAMPLE_RATE, 2).unwrap();
                writer.write(&[audio.clone(), audio]).unwrap();
                writer.finish().unwrap();
                staged.insert(track, path);
                tracks.push(BookTrack {
                    number,
                    title: format!("Track {number}"),
                    samples: *frames,
                    staged: true,
                    ..BookTrack::default()
                });
            }
            book.add_disc(BookDisc { disc_id: format!("d{disc}"), tracks });
        }
        BuildRequest { book, staged, media_folder: dir.join("media"), cover: None }
    }

    fn decode_left(path: &Path) -> Vec<f32> {
        let mut decoder = Decoder::open(path).unwrap();
        let channels = usize::from(decoder.channels());
        let mut left = Vec::new();
        while let Some(block) = decoder.next_block().unwrap() {
            left.extend(block.samples.chunks_exact(channels).map(|f| f[0]));
        }
        left
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0 / 16_000.0
    }

    #[test]
    fn per_chapter_flac_is_the_kept_trimmed_audio_joined_across_discs() {
        let dir = tempfile::tempdir().unwrap();
        let mut request = staged_book(dir.path(), &[&[3_000, 500], &[700, 4_000]]);
        let t = TrackRef::new;
        let book = &mut request.book;
        book.output = OutputSettings {
            layout: OutputLayout::PerChapter,
            format: OutputFormat::Flac,
            ..OutputSettings::default()
        };
        // "End of disc one" and "Disc two" out; the chapter runs on across the discs.
        book.set_removed(&[t(1, 2), t(2, 1)], true).unwrap();
        book.join(&[t(1, 1), t(2, 2)]).unwrap();
        book.set_trim(t(1, 1), Trim { start: 100, end: 0 }).unwrap();
        book.set_trim(t(2, 2), Trim { start: 0, end: 1_000 }).unwrap();
        book.rename(t(1, 1), "The Funeral").unwrap();

        let tracks = build(&request, Utc::now(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(tracks.len(), 1);
        let file = &tracks[0];
        assert_eq!(file.location, dir.path().join("media/Margaret Ellison/The Long Road Home/01 The Funeral.flac"));
        assert_eq!(file.kind, MediaKind::Audiobook);
        let left = decode_left(&file.location);
        let expected: Vec<f32> =
            (100..3_000).map(|f| sample(t(1, 1), f)).chain((0..3_000).map(|f| sample(t(2, 2), f))).collect();
        assert_eq!(left.len(), expected.len());
        assert!(left.iter().zip(&expected).all(|(a, b)| close(*a, *b)), "sample for sample");
        let tags = tunebox_tags::read(&file.location).unwrap();
        assert_eq!((tags.meta.title.as_str(), tags.meta.composer.as_str()), ("The Funeral", "David Ashcombe"));
    }

    #[test]
    fn one_apple_lossless_file_carries_every_chapter_at_its_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut request = staged_book(dir.path(), &[&[44_100, 22_050], &[44_100]]);
        request.book.output = OutputSettings { format: OutputFormat::AppleLossless, ..OutputSettings::default() };
        let tracks = build(&request, Utc::now(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(tracks.len(), 1);
        let path = &tracks[0].location;
        assert_eq!(path, &dir.path().join("media/Margaret Ellison/The Long Road Home.m4b"));
        let chapters = read_mp4_chapters(path).unwrap();
        let starts: Vec<f64> = chapters.iter().map(|(d, _)| d.as_secs_f64()).collect();
        assert_eq!(starts, [0.0, 1.5]);
        assert_eq!(chapters[1].1, "Chapter 2");
        assert_eq!(decode_left(path).len(), 44_100 + 22_050 + 44_100);
        assert_eq!(tunebox_tags::read(path).unwrap().kind, MediaKind::Audiobook);
    }

    #[test]
    fn aac_mono_is_the_default_and_comes_out_at_the_books_length() {
        let dir = tempfile::tempdir().unwrap();
        let request = staged_book(dir.path(), &[&[30_000], &[20_000]]);
        let mut progress = Vec::new();
        let tracks = build(&request, Utc::now(), &AtomicBool::new(false), &mut |p| progress.push(p.fraction)).unwrap();
        let mut decoder = Decoder::open(&tracks[0].location).unwrap();
        assert_eq!(decoder.channels(), 1);
        let mut frames = 0;
        while let Some(block) = decoder.next_block().unwrap() {
            frames += block.frames();
        }
        assert_eq!(frames, 50_000);
        assert_eq!(progress.last().copied(), Some(1.0));
    }

    #[test]
    fn a_failed_or_cancelled_build_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut request = staged_book(dir.path(), &[&[5_000], &[5_000]]);
        request.book.output = OutputSettings {
            layout: OutputLayout::PerChapter,
            format: OutputFormat::Flac,
            ..OutputSettings::default()
        };
        // The second disc's file is damaged after ripping: the first chapter's file goes too.
        std::fs::write(&request.staged[&TrackRef::new(2, 1)], b"not audio").unwrap();
        let error = build(&request, Utc::now(), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert!(error.to_string().contains("Rip disc 2 again"), "{error}");
        let leftovers: Vec<_> = walk(&dir.path().join("media"));
        assert!(leftovers.is_empty(), "{leftovers:?}");

        let request = staged_book(dir.path(), &[&[50_000]]);
        let cancel = AtomicBool::new(false);
        let result = build(&request, Utc::now(), &cancel, &mut |p| {
            if p.fraction > 0.2 {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert!(matches!(result, Err(BuildError::Cancelled)));
        assert!(walk(&dir.path().join("media")).is_empty());
    }

    #[test]
    fn books_that_cannot_be_built_are_refused_up_front() {
        let dir = tempfile::tempdir().unwrap();
        let mut request = staged_book(dir.path(), &[&[100, 100]]);
        request.book.discs[0].tracks[1].staged = false;
        let error = check(&request.book).unwrap_err();
        assert_eq!(error.to_string(), "A track is not ripped yet: 1:2. Rip disc 1 first, or remove it from the book.");
        request.book.set_removed(&[TrackRef::new(1, 2)], true).unwrap();
        assert!(check(&request.book).is_ok());

        let mut many = staged_book(dir.path(), &[&[1; 128], &[1; 128]]);
        many.book.regroup(tunebox_model::audiobook::Grouping::PerTrack);
        assert!(matches!(check(&many.book), Err(BuildError::TooManyChapters(256))));
        many.book.output.layout = OutputLayout::PerChapter;
        assert!(check(&many.book).is_ok());

        request.book.set_removed(&[TrackRef::new(1, 1)], true).unwrap();
        assert!(matches!(check(&request.book), Err(BuildError::Empty)));
    }

    #[test]
    fn an_existing_file_is_never_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let request = staged_book(dir.path(), &[&[1_000]]);
        let first = build(&request, Utc::now(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        let second = build(&request, Utc::now(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_ne!(first[0].location, second[0].location);
        assert!(first[0].location.exists() && second[0].location.exists());
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        entries.flatten().flat_map(|e| if e.path().is_dir() { walk(&e.path()) } else { vec![e.path()] }).collect()
    }
}
