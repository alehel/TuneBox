//! The book: its details, its discs in book order, their tracks, and the output chosen for it.

use super::chapters::BookEdits;
use super::seconds;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A track of the book: disc `disc` of the book (from 1), track `track` as numbered on that disc.
/// Written "2:14". Ordered as the book plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct TrackRef {
    pub disc: u16,
    pub track: u8,
}

impl TrackRef {
    pub const fn new(disc: u16, track: u8) -> Self {
        Self { disc, track }
    }
}

impl fmt::Display for TrackRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.disc, self.track)
    }
}

impl FromStr for TrackRef {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let bad = || format!("“{text}” is not a track: write the disc and the track, like 2:14");
        let (disc, track) = text.trim().split_once(':').ok_or_else(bad)?;
        let disc: u16 = disc.trim().parse().map_err(|_| bad())?;
        let track: u8 = track.trim().parse().map_err(|_| bad())?;
        if disc == 0 || track == 0 {
            return Err(bad());
        }
        Ok(TrackRef { disc, track })
    }
}

impl From<TrackRef> for String {
    fn from(track: TrackRef) -> String {
        track.to_string()
    }
}

impl TryFrom<String> for TrackRef {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        text.parse()
    }
}

/// What is cut off a track, in samples, from its start and from its end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trim {
    pub start: u64,
    pub end: u64,
}

impl Trim {
    pub fn is_none(&self) -> bool {
        self.start == 0 && self.end == 0
    }
}

/// A quiet stretch of a track, in samples from its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pause {
    pub start: u64,
    pub end: u64,
}

impl Pause {
    pub fn middle(&self) -> u64 {
        self.start + (self.end.saturating_sub(self.start)) / 2
    }

    pub fn length(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

/// One track of a disc in the book.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookTrack {
    /// The track's number on its disc.
    pub number: u8,
    /// Its name, from MusicBrainz or typed in; "Track 03" when unknown.
    pub title: String,
    /// Its length, from the disc's table of contents.
    pub samples: u64,
    /// Whether its audio has been ripped into the book's staging folder.
    pub staged: bool,
    /// Why the last attempt to rip it failed, if it did.
    pub failure: Option<String>,
    /// Its quiet stretches, measured while it was ripped.
    pub pauses: Vec<Pause>,
}

/// One disc of the book.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookDisc {
    /// The MusicBrainz disc id, which recognises the disc when it is inserted again.
    pub disc_id: String,
    /// Its audio tracks, in disc order.
    pub tracks: Vec<BookTrack>,
}

impl BookDisc {
    /// Whether every track has been ripped.
    pub fn is_ripped(&self) -> bool {
        self.tracks.iter().all(|t| t.staged)
    }
}

/// What the listener says the book is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookDetails {
    pub title: String,
    pub author: String,
    pub narrator: String,
    pub series: String,
    pub series_number: Option<u32>,
    pub year: Option<u32>,
    pub publisher: String,
    pub genre: String,
    pub description: String,
    /// The MusicBrainz release the details came from, when they were looked up.
    pub release_id: Option<String>,
    pub release_group_id: Option<String>,
}

impl Default for BookDetails {
    fn default() -> Self {
        Self {
            title: String::new(),
            author: String::new(),
            narrator: String::new(),
            series: String::new(),
            series_number: None,
            year: None,
            publisher: String::new(),
            genre: "Audiobook".into(),
            description: String::new(),
            release_id: None,
            release_group_id: None,
        }
    }
}

impl BookDetails {
    /// "The Harbour Trilogy, 2", "The Harbour Trilogy", or "".
    pub fn series_label(&self) -> String {
        match (self.series.trim(), self.series_number) {
            ("", _) => String::new(),
            (series, Some(n)) => format!("{series}, {n}"),
            (series, None) => series.to_owned(),
        }
    }
}

/// One medium of the MusicBrainz release the book came from: what is known about a disc
/// before it is inserted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReleaseMedium {
    /// Its position in the release, from 1.
    pub position: u32,
    /// The disc ids MusicBrainz knows for it.
    pub disc_ids: Vec<String>,
    /// Its track names, in order.
    pub track_titles: Vec<String>,
}

/// One file with chapters, or one file per chapter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputLayout {
    #[default]
    SingleFile,
    PerChapter,
}

/// The audio format the book is built in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFormat {
    #[default]
    Aac,
    AppleLossless,
    Flac,
}

impl OutputFormat {
    pub fn name(self) -> &'static str {
        match self {
            OutputFormat::Aac => "AAC",
            OutputFormat::AppleLossless => "Apple Lossless",
            OutputFormat::Flac => "FLAC",
        }
    }

    /// The extension of the files written: audiobooks in MP4 are `.m4b`.
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Aac | OutputFormat::AppleLossless => "m4b",
            OutputFormat::Flac => "flac",
        }
    }
}

/// How the book is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputSettings {
    pub layout: OutputLayout,
    pub format: OutputFormat,
    /// AAC only: the bit rate, in kbit/s.
    pub aac_bitrate_kbps: u32,
    /// AAC only: mix the two channels into one, as is usual for speech.
    pub mono: bool,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self { layout: OutputLayout::SingleFile, format: OutputFormat::Aac, aac_bitrate_kbps: 64, mono: true }
    }
}

impl OutputSettings {
    /// The AAC bit rates offered, in kbit/s.
    pub const AAC_BITRATES: [u32; 6] = [48, 64, 80, 96, 128, 160];

    /// Refuses settings that cannot be built.
    pub fn check(&self) -> Result<(), BookError> {
        if self.layout == OutputLayout::SingleFile && self.format == OutputFormat::Flac {
            return Err(BookError::Settings(
                "A FLAC file cannot hold chapters that players show. Build one file per chapter, or choose \
                 AAC or Apple Lossless."
                    .into(),
            ));
        }
        if self.format == OutputFormat::Aac && !Self::AAC_BITRATES.contains(&self.aac_bitrate_kbps) {
            let offered: Vec<String> = Self::AAC_BITRATES.iter().map(u32::to_string).collect();
            return Err(BookError::Settings(format!(
                "{} kbit/s is not an AAC bit rate TuneBox offers. Choose one of {} kbit/s.",
                self.aac_bitrate_kbps,
                offered.join(", ")
            )));
        }
        Ok(())
    }

    /// Whether the build mixes to mono.
    pub fn is_mono(&self) -> bool {
        self.mono && self.format == OutputFormat::Aac
    }

    /// "One .m4b, AAC 64 kbit/s mono".
    pub fn describe(&self) -> String {
        let layout = match self.layout {
            OutputLayout::SingleFile => format!("one .{}", self.format.extension()),
            OutputLayout::PerChapter => format!("a .{} per chapter", self.format.extension()),
        };
        let format = match self.format {
            OutputFormat::Aac => {
                format!("AAC {} kbit/s {}", self.aac_bitrate_kbps, if self.mono { "mono" } else { "stereo" })
            }
            other => other.name().to_owned(),
        };
        format!("{layout}, {format}")
    }
}

/// Why an edit of the book was refused. The messages are shown to the listener as they are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BookError {
    NoSuchTrack(TrackRef),
    NotRipped(TrackRef),
    TrimTooLong { track: TrackRef, length: u64 },
    NameTooLong { bytes: usize },
    Settings(String),
}

impl fmt::Display for BookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BookError::NoSuchTrack(track) => write!(f, "The book has no track {track}."),
            BookError::NotRipped(track) => {
                write!(f, "Track {track} is not ripped yet. Rip its disc first, then trim it.")
            }
            BookError::TrimTooLong { track, length } => write!(
                f,
                "That would cut all {:.1} seconds of track {track}. To leave the track out, remove it from the book \
                 instead.",
                seconds(*length)
            ),
            BookError::NameTooLong { bytes } => write!(
                f,
                "That chapter name is too long ({bytes} bytes); chapters in a .m4b hold at most {} bytes.",
                super::layout::MAX_CHAPTER_NAME_BYTES
            ),
            BookError::Settings(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for BookError {}

/// An audiobook being ripped and edited.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Book {
    pub details: BookDetails,
    /// The discs, in book order: disc 1 first.
    pub discs: Vec<BookDisc>,
    pub edits: BookEdits,
    pub output: OutputSettings,
    /// The media of the release the book came from, when it was looked up.
    pub release_media: Vec<ReleaseMedium>,
    /// The listener said the last disc was ripped.
    pub ripping_done: bool,
}

impl Book {
    pub fn new(details: BookDetails, release_media: Vec<ReleaseMedium>) -> Self {
        Self { details, release_media, ..Self::default() }
    }

    /// Adds the next disc. It starts as one chapter of its own. Returns its number in the book.
    pub fn add_disc(&mut self, disc: BookDisc) -> u16 {
        let number = self.next_disc_number();
        if let Some(first) = disc.tracks.first() {
            self.edits.chapter_starts.insert(TrackRef::new(number, first.number));
        }
        self.discs.push(disc);
        number
    }

    /// The number the next disc will have.
    pub fn next_disc_number(&self) -> u16 {
        u16::try_from(self.discs.len()).unwrap_or(u16::MAX).saturating_add(1)
    }

    pub fn disc(&self, number: u16) -> Option<&BookDisc> {
        self.discs.get(usize::from(number).checked_sub(1)?)
    }

    pub fn disc_mut(&mut self, number: u16) -> Option<&mut BookDisc> {
        self.discs.get_mut(usize::from(number).checked_sub(1)?)
    }

    pub fn track(&self, track: TrackRef) -> Option<&BookTrack> {
        self.disc(track.disc)?.tracks.iter().find(|t| t.number == track.track)
    }

    pub fn track_mut(&mut self, track: TrackRef) -> Option<&mut BookTrack> {
        self.disc_mut(track.disc)?.tracks.iter_mut().find(|t| t.number == track.track)
    }

    /// Every track, in book order.
    pub fn tracks(&self) -> impl Iterator<Item = (TrackRef, &BookTrack)> {
        self.discs.iter().zip(1u16..).flat_map(|(disc, number)| {
            disc.tracks.iter().map(move |track| (TrackRef::new(number, track.number), track))
        })
    }

    /// Every track's address, in book order.
    pub fn track_refs(&self) -> Vec<TrackRef> {
        self.tracks().map(|(r, _)| r).collect()
    }

    /// Whether `track` is in the book (not removed).
    pub fn is_kept(&self, track: TrackRef) -> bool {
        !self.edits.removed.contains(&track)
    }

    /// Kept tracks that are not ripped: the book cannot be built while there are any.
    pub fn unripped_kept(&self) -> Vec<TrackRef> {
        self.tracks().filter(|(r, t)| !t.staged && self.is_kept(*r)).map(|(r, _)| r).collect()
    }

    /// How long the book is, in samples: the kept tracks, less their trims.
    pub fn kept_samples(&self) -> u64 {
        self.tracks().filter(|(r, _)| self.is_kept(*r)).map(|(r, t)| self.trimmed_length(r, t)).sum()
    }

    /// How long `track` is once trimmed, in samples; 0 for a track not in the book's discs.
    pub fn trimmed_samples(&self, track: TrackRef) -> u64 {
        self.track(track).map_or(0, |data| self.trimmed_length(track, data))
    }

    /// The length of `track` once trimmed.
    pub(crate) fn trimmed_length(&self, track: TrackRef, data: &BookTrack) -> u64 {
        let trim = self.edits.trims.get(&track).copied().unwrap_or_default();
        data.samples.saturating_sub(trim.start).saturating_sub(trim.end)
    }

    /// The release medium MusicBrainz lists at `position`, if known.
    pub fn release_medium(&self, position: u32) -> Option<&ReleaseMedium> {
        self.release_media.iter().find(|m| m.position == position)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A book of discs with these track lengths, in seconds; every track ripped.
    pub(crate) fn book(discs: &[&[u64]]) -> Book {
        let mut book = Book::new(BookDetails { title: "The Long Road Home".into(), ..BookDetails::default() }, vec![]);
        for lengths in discs {
            let tracks = lengths
                .iter()
                .zip(1u8..)
                .map(|(seconds, number)| BookTrack {
                    number,
                    title: format!("Track {number:02}"),
                    samples: seconds * u64::from(super::super::SAMPLE_RATE),
                    staged: true,
                    ..BookTrack::default()
                })
                .collect();
            book.add_disc(BookDisc { disc_id: format!("disc-{}", book.discs.len() + 1), tracks });
        }
        book
    }

    #[test]
    fn track_refs_read_and_write_as_disc_colon_track() {
        let track: TrackRef = "2:14".parse().unwrap();
        assert_eq!(track, TrackRef::new(2, 14));
        assert_eq!(track.to_string(), "2:14");
        for bad in ["", "2", "0:1", "1:0", "a:b", "1:300", "-1:2"] {
            assert!(bad.parse::<TrackRef>().is_err(), "{bad}");
        }
        assert!(TrackRef::new(1, 14) < TrackRef::new(2, 1));
    }

    #[test]
    fn every_disc_starts_as_one_chapter() {
        let book = book(&[&[10, 20], &[30]]);
        assert_eq!(
            book.edits.chapter_starts.iter().copied().collect::<Vec<_>>(),
            vec![TrackRef::new(1, 1), TrackRef::new(2, 1)]
        );
        assert_eq!(book.next_disc_number(), 3);
        assert_eq!(book.track_refs().len(), 3);
    }

    #[test]
    fn the_book_survives_a_round_trip_through_json() {
        let mut book = book(&[&[10, 20], &[30]]);
        book.edits.names.insert(TrackRef::new(2, 1), "The Funeral".into());
        book.edits.removed.insert(TrackRef::new(1, 2));
        book.edits.trims.insert(TrackRef::new(1, 1), Trim { start: 5, end: 7 });
        let json = serde_json::to_string(&book).unwrap();
        assert!(json.contains("\"2:1\""), "{json}");
        let back: Book = serde_json::from_str(&json).unwrap();
        assert_eq!(back, book);
    }

    #[test]
    fn output_settings_refuse_what_cannot_be_built() {
        let flac = OutputSettings { format: OutputFormat::Flac, ..OutputSettings::default() };
        assert!(flac.check().is_err());
        assert!(OutputSettings { layout: OutputLayout::PerChapter, ..flac }.check().is_ok());
        assert!(OutputSettings { aac_bitrate_kbps: 63, ..OutputSettings::default() }.check().is_err());
        assert_eq!(OutputSettings::default().describe(), "one .m4b, AAC 64 kbit/s mono");
    }
}
