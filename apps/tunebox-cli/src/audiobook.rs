//! `tunebox-cli audiobook`: an audiobook from several CDs, one command per step.
//!
//! The book is saved between runs, so each disc is one run: `start` with disc 1 in the drive,
//! edit, `rip`; insert the next disc and `rip` again; `done` after the last; then `build`.
//! Undo is the app's: each run of the command line is a session of its own.

use crate::{read_cd, wait_for_jobs};
use clap::{Args, Subcommand};
use tunebox_engine::model::audiobook::{
    BookDetails, Grouping, OutputFormat, OutputLayout, OutputSettings, Suggestion, TrackRef, Trim, samples, seconds,
};
use tunebox_engine::model::format_clock;
use tunebox_engine::{Session, VirtualDrive};

#[derive(Subcommand)]
pub(crate) enum AudiobookAction {
    /// Start a book from the audio CD in the drive, which becomes disc 1. Details left out come
    /// from the disc's names (MusicBrainz's with --lookup).
    Start {
        #[command(flatten)]
        details: DetailArgs,
        #[command(flatten)]
        disc: DiscArgs,
    },
    /// Show the book: its discs, chapters, removed and trimmed tracks, and suggestions.
    Status,
    /// Rip the disc in the drive into the book, then eject it. A new disc joins the book as its
    /// next disc first.
    Rip {
        #[command(flatten)]
        disc: DiscArgs,
    },
    /// Answer the question about the disc in the drive: use it as the next disc, or eject it.
    Answer {
        #[arg(value_parser = ["use", "eject"])]
        answer: String,
    },
    /// That was the last disc.
    Done {
        /// Finish even though MusicBrainz lists more discs for the book.
        #[arg(long)]
        anyway: bool,
    },
    /// Change the book's details.
    Details {
        #[command(flatten)]
        details: DetailArgs,
    },
    /// Start a chapter at each of these tracks (written disc:track, like 2:4).
    Split { tracks: Vec<String> },
    /// Make one chapter of the chapters holding these tracks (ranges like 1:14-2:3 too). With
    /// one chapter's tracks, join it to the chapter before.
    Join { tracks: Vec<String> },
    /// Name the chapter holding a track, or chapter number N. An empty name goes back to
    /// "Chapter N".
    Rename { chapter: String, name: String },
    /// Leave these tracks out of the book ("Please insert disc 2").
    Remove { tracks: Vec<String> },
    /// Put removed tracks back.
    Restore { tracks: Vec<String> },
    /// Cut seconds off the start and end of a ripped track (0 for no cut).
    Trim {
        track: String,
        #[arg(long, default_value_t = 0.0)]
        start: f64,
        #[arg(long, default_value_t = 0.0)]
        end: f64,
    },
    /// Regroup the whole book: one chapter per disc (the start), per track, or from the track
    /// names ("Chapter 4 (1)", "Chapter 4 (2)" make one).
    Chapters {
        #[arg(value_parser = ["per-disc", "per-track", "from-names"])]
        grouping: String,
    },
    /// List the suggested edits; --accept-all makes them all.
    Suggestions {
        #[arg(long)]
        accept_all: bool,
    },
    /// List a ripped track's pauses, for precise trims.
    Pauses { track: String },
    /// Choose how the book is built.
    Output {
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Build the book into the media folder and the library, then delete the ripped discs.
    Build {
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Throw the book away: its ripped discs and edits. The library is not touched.
    Discard,
}

#[derive(Args)]
pub(crate) struct DetailArgs {
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    author: Option<String>,
    #[arg(long)]
    narrator: Option<String>,
    #[arg(long)]
    series: Option<String>,
    #[arg(long)]
    series_number: Option<u32>,
    #[arg(long)]
    year: Option<u32>,
    #[arg(long)]
    publisher: Option<String>,
    #[arg(long)]
    description: Option<String>,
}

impl DetailArgs {
    fn apply(self, details: &mut BookDetails) {
        let set = |field: &mut String, value: Option<String>| {
            if let Some(value) = value {
                *field = value;
            }
        };
        set(&mut details.title, self.title);
        set(&mut details.author, self.author);
        set(&mut details.narrator, self.narrator);
        set(&mut details.series, self.series);
        set(&mut details.publisher, self.publisher);
        set(&mut details.description, self.description);
        details.series_number = self.series_number.or(details.series_number);
        details.year = self.year.or(details.year);
    }
}

#[derive(Args)]
pub(crate) struct DiscArgs {
    /// Use a synthetic disc instead of a drive (for trying TuneBox out).
    #[arg(long)]
    virtual_disc: bool,
    /// Look the disc's names up on MusicBrainz.
    #[arg(long)]
    lookup: bool,
    /// Then get the release's cover from the Cover Art Archive (needs --lookup).
    #[arg(long, requires = "lookup")]
    cover: bool,
}

#[derive(Args)]
pub(crate) struct OutputArgs {
    /// One .m4b with chapters.
    #[arg(long, conflicts_with = "per_chapter")]
    single: bool,
    /// One file per chapter.
    #[arg(long)]
    per_chapter: bool,
    #[arg(long, value_parser = ["aac", "alac", "flac"])]
    format: Option<String>,
    /// AAC bit rate in kbit/s.
    #[arg(long)]
    bitrate: Option<u32>,
    #[arg(long, conflicts_with = "stereo")]
    mono: bool,
    #[arg(long)]
    stereo: bool,
}

impl OutputArgs {
    fn apply(&self, output: &mut OutputSettings) {
        if self.single {
            output.layout = OutputLayout::SingleFile;
        }
        if self.per_chapter {
            output.layout = OutputLayout::PerChapter;
        }
        match self.format.as_deref() {
            Some("aac") => output.format = OutputFormat::Aac,
            Some("alac") => output.format = OutputFormat::AppleLossless,
            Some("flac") => output.format = OutputFormat::Flac,
            _ => {}
        }
        if let Some(bitrate) = self.bitrate {
            output.aac_bitrate_kbps = bitrate;
        }
        output.mono = (output.mono || self.mono) && !self.stereo;
    }
}

pub(crate) fn run(session: &mut Session, action: AudiobookAction) -> Result<(), String> {
    let error = |e: tunebox_engine::EngineError| e.to_string();
    match action {
        AudiobookAction::Start { details: args, disc } => {
            read_disc(session, &disc, 1)?;
            let mut details = BookDetails::default();
            args.apply(&mut details);
            session.start_audiobook(details).map_err(error)?;
            status(session)?;
            println!("\nEdit the chapters if you like, then: tunebox-cli audiobook rip");
        }
        AudiobookAction::Status => status(session)?,
        AudiobookAction::Rip { disc } => rip(session, &disc)?,
        AudiobookAction::Answer { answer } => {
            session.answer_audiobook_question(answer == "use").map_err(error)?;
            if answer == "use" {
                println!("The disc is in the book. Rip it with: tunebox-cli audiobook rip");
            }
        }
        AudiobookAction::Done { anyway } => {
            session.finish_audiobook_ripping(anyway).map_err(|e| match e {
                tunebox_engine::EngineError::MoreDiscsListed { .. } => format!("{e} Add --anyway to finish."),
                other => other.to_string(),
            })?;
            println!("Ripping is finished. Build the book with: tunebox-cli audiobook build");
        }
        AudiobookAction::Details { details: args } => {
            let mut details = book(session)?.details.clone();
            args.apply(&mut details);
            session.set_audiobook_details(details).map_err(error)?;
        }
        AudiobookAction::Split { tracks } => {
            session.split_audiobook_chapter(&parse_tracks(session, &tracks)?).map_err(error)?
        }
        AudiobookAction::Join { tracks } => {
            session.join_audiobook_chapters(&parse_tracks(session, &tracks)?).map_err(error)?
        }
        AudiobookAction::Rename { chapter, name } => {
            let track = chapter_start(session, &chapter)?;
            session.rename_audiobook_chapter(track, &name).map_err(error)?;
        }
        AudiobookAction::Remove { tracks } => {
            session.set_audiobook_tracks_removed(&parse_tracks(session, &tracks)?, true).map_err(error)?
        }
        AudiobookAction::Restore { tracks } => {
            session.set_audiobook_tracks_removed(&parse_tracks(session, &tracks)?, false).map_err(error)?
        }
        AudiobookAction::Trim { track, start, end } => {
            let track = parse_track(&track)?;
            session.trim_audiobook_track(track, Trim { start: samples(start), end: samples(end) }).map_err(error)?;
        }
        AudiobookAction::Chapters { grouping } => {
            let grouping = match grouping.as_str() {
                "per-track" => Grouping::PerTrack,
                "from-names" => Grouping::FromNames,
                _ => Grouping::PerDisc,
            };
            session.regroup_audiobook(grouping).map_err(error)?;
            print_chapters(session)?;
        }
        AudiobookAction::Suggestions { accept_all } => {
            if accept_all {
                let count = session.accept_all_audiobook_suggestions().map_err(error)?;
                println!("{count} suggestions accepted.");
            } else {
                print_suggestions(session)?;
            }
        }
        AudiobookAction::Pauses { track } => {
            let track = parse_track(&track)?;
            let data = book(session)?.track(track).ok_or(format!("The book has no track {track}."))?;
            if !data.staged {
                return Err(format!("Track {track} is not ripped yet: its pauses are measured while it is ripped."));
            }
            for pause in &data.pauses {
                println!(
                    "{:>8.2} s – {:>8.2} s  ({:.2} s)",
                    seconds(pause.start),
                    seconds(pause.end),
                    seconds(pause.length())
                );
            }
        }
        AudiobookAction::Output { output } => {
            let mut settings = book(session)?.output;
            output.apply(&mut settings);
            session.set_audiobook_output(settings).map_err(error)?;
            println!("The book will be built as {}.", settings.describe());
        }
        AudiobookAction::Build { output } => {
            let mut settings = book(session)?.output;
            output.apply(&mut settings);
            session.set_audiobook_output(settings).map_err(error)?;
            session.build_audiobook().map_err(error)?;
            wait_for_jobs(session);
        }
        AudiobookAction::Discard => {
            session.discard_audiobook().map_err(error)?;
            println!("The audiobook was discarded.");
        }
    }
    Ok(())
}

fn book(session: &Session) -> Result<&tunebox_engine::model::audiobook::Book, String> {
    session.audiobook().ok_or_else(|| tunebox_engine::EngineError::NoAudiobook.to_string())
}

/// Reads the disc in the drive; `number` picks the synthetic disc with --virtual-disc, each
/// number a disc of its own.
fn read_disc(session: &mut Session, args: &DiscArgs, number: u16) -> Result<(), String> {
    let disc = args.virtual_disc.then(|| {
        let n = u32::from(number);
        VirtualDrive::new(format!("Virtual Disc {number}"), vec![2 + n % 3, 4, 3 + n, 2])
    });
    read_cd(session, disc, args.lookup, args.cover)
}

fn rip(session: &mut Session, args: &DiscArgs) -> Result<(), String> {
    let wanted =
        session.audiobook_waiting_for().or_else(|| book(session).ok().map(|b| b.next_disc_number().saturating_sub(1)));
    read_disc(session, args, wanted.unwrap_or(1))?;
    crate::print_notices(session);
    if let Some(question) = session.audiobook_question() {
        return Err(format!("{} Answer with: tunebox-cli audiobook answer use (or eject).", question.text()));
    }
    session.rip_audiobook_disc().map_err(|e| e.to_string())?;
    wait_for_jobs(session);
    if let Some(next) = session.audiobook_waiting_for() {
        println!(
            "Insert disc {next}, then: tunebox-cli audiobook rip. If that was the last disc: tunebox-cli audiobook done"
        );
    }
    Ok(())
}

fn status(session: &Session) -> Result<(), String> {
    let book = book(session)?;
    let details = &book.details;
    let narrator = if details.narrator.is_empty() { String::new() } else { format!(", read by {}", details.narrator) };
    println!("{} — {}{narrator}", details.title, details.author);
    let ripped = book.discs.iter().filter(|d| d.is_ripped()).count();
    let state = if book.ripping_done {
        "ripping finished".to_owned()
    } else if let Some(next) = session.audiobook_waiting_for() {
        format!("insert disc {next}")
    } else {
        format!("disc {} ready to rip", book.discs.len())
    };
    println!(
        "{ripped} of {} discs ripped · {state} · {} · builds {}",
        book.discs.len(),
        format_clock(std::time::Duration::from_secs_f64(seconds(book.kept_samples()))),
        book.output.describe()
    );
    println!();
    print_chapters(session)?;
    if !session.audiobook_suggestions().is_empty() {
        println!();
        print_suggestions(session)?;
    }
    Ok(())
}

fn print_chapters(session: &Session) -> Result<(), String> {
    let book = book(session)?;
    for chapter in book.chapters() {
        let number = chapter.number.map(|n| n.to_string()).unwrap_or_else(|| "–".into());
        let discs = match chapter.discs {
            (a, b) if a == b => format!("disc {a}"),
            (a, b) => format!("discs {a}–{b}"),
        };
        let length = format_clock(std::time::Duration::from_secs_f64(seconds(chapter.samples)));
        println!("{number:>4}  {:<40} {length:>8}  {discs}", chapter.name);
        for track in &chapter.tracks {
            let Some(data) = book.track(*track) else { continue };
            let length = format_clock(std::time::Duration::from_secs_f64(seconds(data.samples)));
            let mut notes = Vec::new();
            if !book.is_kept(*track) {
                notes.push("removed".to_owned());
            }
            let trim = book.trim(*track);
            if trim.start > 0 {
                notes.push(format!("−{:.1} s at the start", seconds(trim.start)));
            }
            if trim.end > 0 {
                notes.push(format!("−{:.1} s at the end", seconds(trim.end)));
            }
            if !data.staged {
                notes.push(data.failure.clone().map_or_else(|| "not ripped".to_owned(), |f| format!("failed: {f}")));
            }
            println!("        {:<6} {:<36.36} {length:>8}  {}", track.to_string(), data.title, notes.join(", "));
        }
    }
    Ok(())
}

fn print_suggestions(session: &Session) -> Result<(), String> {
    let book = book(session)?;
    let suggestions = session.audiobook_suggestions();
    if suggestions.is_empty() {
        println!("No suggestions.");
    }
    for suggestion in suggestions {
        let track = suggestion.track();
        let title = book.track(track).map(|t| t.title.clone()).unwrap_or_default();
        match suggestion {
            Suggestion::Remove(_) => println!("{track:<6} remove: “{title}” looks like an announcement"),
            Suggestion::Trim { trim, .. } if trim.end > 0 => {
                println!("{track:<6} cut {:.1} s off the end: a possible announcement", seconds(trim.end))
            }
            Suggestion::Trim { trim, .. } => {
                println!("{track:<6} cut {:.1} s off the start: a possible announcement", seconds(trim.start))
            }
        }
    }
    println!("Accept them all with: tunebox-cli audiobook suggestions --accept-all");
    Ok(())
}

fn parse_track(text: &str) -> Result<TrackRef, String> {
    text.parse()
}

/// Tracks and ranges ("1:14-2:3", every track between, in book order).
fn parse_tracks(session: &Session, texts: &[String]) -> Result<Vec<TrackRef>, String> {
    let all = book(session)?.track_refs();
    let mut tracks = Vec::new();
    for text in texts {
        match text.split_once('-') {
            Some((from, to)) => {
                let (from, to) = (parse_track(from)?, parse_track(to)?);
                tracks.extend(all.iter().copied().filter(|t| (from..=to).contains(t)));
            }
            None => tracks.push(parse_track(text)?),
        }
    }
    if tracks.is_empty() {
        return Err("Name at least one track, like 2:4.".into());
    }
    Ok(tracks)
}

/// A chapter given as a track in it ("2:1") or as its number ("7").
fn chapter_start(session: &Session, text: &str) -> Result<TrackRef, String> {
    if text.contains(':') {
        return parse_track(text);
    }
    let number: usize = text.trim().parse().map_err(|_| format!("“{text}” is not a chapter number or a track"))?;
    book(session)?
        .chapters()
        .into_iter()
        .find(|c| c.number == Some(number))
        .map(|c| c.start)
        .ok_or_else(|| format!("The book has no chapter {number}."))
}
