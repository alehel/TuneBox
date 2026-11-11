# Audiobooks from CDs: plan

Status: **planned, not built.** The single-file `.m4b` output waits for the AAC encoder that
craft-audio is getting. Most of the rest can be built before it arrives (see [Phases](#phases)).
This document says what the audiobook builder does, how it fits TuneBox's layers, and what
still has to be decided.

## What the listener does

1. Inserts the first disc of the book and chooses **Import as Audiobook…** on the CD page (or
   `tunebox-cli audiobook start`).
2. Types the book's details: title, author, narrator, and optionally series, number in the
   series, year, publisher, description, how many discs the book has, and a cover. When the
   disc is on MusicBrainz, the details and the track names of *every* disc in the set are
   filled in from the release.
3. Rips the disc. When the disc is done, TuneBox ejects it and asks for the next one:
   "Insert disc 3 of 12." Putting it in starts the next rip. This repeats until the last disc,
   or until the listener says "That was the last disc".
4. Groups tracks into chapters, before, during or after ripping: select tracks and **Join into
   One Chapter**, or **Split Chapter Here**, and name each chapter. A chapter may run from the
   end of one disc into the next. Quick starts: one chapter per track, one per disc, or the
   chapters suggested by MusicBrainz track names.
5. Chooses the output and builds the book:
   - **One file with chapters**: a single `.m4b`.
   - **One file per chapter**: numbered files, tagged as parts of the same book.

The book is added to the library, and the staged rip is deleted.

### Why ripping and building are separate

The user's sketch builds the output as the discs go in. This plan rips every disc to lossless
files first and builds the book from them at the end. Reasons:

- **Never lose music.** A single `.m4b` spanning twelve discs would need one encoder held open
  across an hour or more of disc swapping. A crash, a quit, a full disk or a scratched disc on
  disc 11 would lose everything. Staged files survive all of these; the book resumes where it
  stopped.
- **Chapters can change until the end.** Grouping happens on the staged tracks, so a chapter
  that runs on into the next disc, or a mistake noticed on disc 7, is fixed by moving a
  boundary, with no re-ripping.
- **The output can change until the end.** Single file or files per chapter, bit rate, mono or
  stereo: all decided at build time, from the same rip.
- **Seamless chapters.** A chapter made of several CD tracks is fed to one encoder as one
  stream, so there is no gap or click where the CD tracks joined. Track files ripped with read
  offset correction join sample-exactly.

The cost is disk space while the book is in progress: a full CD of speech is roughly 300–400 MB
as FLAC, so a 12-disc book needs a few GB until it is built. TuneBox checks free space before each
disc and says how much is needed.

## The book project

An audiobook in progress is a **project**, saved to disk so it survives quitting TuneBox and so
the CLI can work on it across several runs (one per disc).

```text
<app folder>/Audiobooks/<project id>/
    Project.json              written through craft-atomic
    disc-01/track-01.flac     the staged rip, lossless, through *.part files
    disc-01/track-02.flac
    disc-02/…
```

The staging folder belongs to TuneBox (like the library file), not to the listener, so it is
not written through `files.rs`. The finished book is: it goes into the media folder through
`files.rs`, which refuses a read-only media folder before any disc is ripped.

`Project.json` holds:

| Part | Contents |
|---|---|
| Book | Title, author, narrator, series and number, year, publisher, genre (default "Audiobook"), description, cover, MusicBrainz release id |
| Discs | Per disc, in book order: its disc id, table of contents, track names, which tracks are staged and which are left out of the book (a publisher's trailer, say) |
| Chapters | The tracks where a chapter starts, as (disc, track), and each chapter's name |
| Output | Single file or per chapter; format, bit rate, mono or stereo |
| Expected discs | How many discs the book has, if known |

Chapters are stored as **boundaries** in the book's track list, not as lists of tracks. Joining,
splitting and moving a chapter are then edits of one set, a chapter can cross a disc without
special cases, and every track always belongs to exactly one chapter.

A damaged `Project.json` is moved aside, like a damaged library file, and the staged audio is
kept so nothing has to be ripped again.

## Inserting the next disc

The engine already looks for discs every few seconds (`Session::scan_drives`). While a project
is waiting for a disc, a newly read disc is checked before it is ripped:

| The disc is | TuneBox says |
|---|---|
| The next one expected (MusicBrainz medium position matches, or unknown) | Rips it as the next disc |
| One already ripped (same disc id) | "This is disc 2, which is already imported. Insert disc 3." Ejects it |
| Another disc of the same release (MusicBrainz says disc 5, disc 3 expected) | Asks: rip it as disc 5, or as disc 3 anyway |
| An earlier disc with tracks that failed | Offers to rip only the missing tracks |
| Unrelated (unknown disc, different release) | Asks whether it belongs to this book, since a disc without MusicBrainz data cannot be checked |

The checks are a pure function of the project, the new disc's id and the lookup result, tested
on their own.

### Ejecting

Eject today is `tunebox_platform::eject_disc()`, called by the UI. It always opens `/dev/cdrom`
on Linux and does nothing on Windows. For the audiobook loop it should eject the drive that was
ripped, on every system:

- Add `CdDrive::eject(&self) -> Result<(), DriveError>`. `PhysicalDrive` sends the drive the
  SCSI START STOP UNIT command with the eject bit, the same way cd-da-reader already sends its
  read commands. That needs a small addition to cd-da-reader (TuneBox forbids `unsafe`, so it
  cannot issue the ioctl itself). `VirtualDrive` records that it was ejected, for tests.
- The rip job ejects the disc as its last step, so the CLI and the UI behave the same way. A
  drive that cannot eject (some laptop drives, or a failure) is not an error: the listener is
  asked to take the disc out.
- The existing Eject command moves to the same call, which fixes the Linux and Windows cases.

## Building the book

Building is an engine job. It decodes the staged FLAC files in book order, feeds each chapter
(or the whole book) to the encoder, and writes the result through a `*.part` file renamed on
completion.

Chapter start times come from the tables of contents alone: every track's length is its sector
count × 588 samples, which offset correction does not change. They are a pure function, tested
without audio.

### One file with chapters (`.m4b`)

| Piece | Who provides it | State |
|---|---|---|
| AAC encoding | craft-audio | Coming |
| MP4 container | craft-media `isobmff::Mp4Writer` | Exists: writes 64-bit durations when needed (a 30-hour book overflows 32 bits at 44.1 kHz), edit lists for AAC priming, `moov` in front ("fast start") |
| Chapters | craft-media | **To add**: a disabled text track with one sample per chapter, referenced from the audio track by `tref/chap` (what Apple's players read), plus a Nero `chpl` box in `moov/udta` (what many other players read). Both written, since players differ |
| Tags and cover | `tunebox-tags` (lofty) | lofty updates the `stco`/`co64` offsets of every track when it grows `moov`, so writing tags after muxing keeps the chapter track valid. To be proven by a test that writes tags to a chaptered file and reads the chapters back |
| Media kind "Audiobook" | `tunebox-tags` | The `stik` atom set to 2, through lofty's generic atoms |

Tag conventions to follow (observed in how audiobook players and tools behave; *confirm*
against real players before relying on them): title and album = book title, artist and album
artist = author, narrator in composer, genre "Audiobook", description in `desc`/`ldes`, series
in grouping or the sort album. Track and disc numbers are left out of a single-file book.

Encoding options: AAC at 64 kbit/s mono (the default proposed for speech), 96 or 128 kbit/s
stereo. Mono mixes the two channels down before encoding.

### One file per chapter

Each chapter becomes one file: `Author/Book Title/NN Chapter Name.ext`, album = book title,
track number = chapter number of the total, no disc number (chapters run across discs). Format:

- AAC in `.m4b` or `.m4a`, tagged as an audiobook, once the encoder exists.
- **FLAC, possible today.** This lets the whole multi-disc loop ship before the AAC encoder.

## How it fits the layers

| Layer | Crate | New |
|---|---|---|
| L0 | `model` | `audiobook`: book details, the track list, chapter boundaries (join, split, rename, per track, per disc, from names), chapter start times, output file names. Pure, tested next to it |
| L1 | `tags` | Audiobook tags: media kind, narrator, description, chapters read back for tests |
| L1 | `audio` | The AAC writer (and mono mixdown) over craft-audio once it lands |
| L2 | `rip` | Rip into a given staging folder without tagging or the library (today's `rip` both writes and tags into the media folder; the reading part is shared). `CdDrive::eject`. The MusicBrainz parser keeps every medium of the release, not only the inserted one, for the names of discs not yet ripped. A virtual drive that can swap discs, for tests |
| L2 | `audiobook` (new) | Builds the book from the staged files: chapter streams, encoding, the `.m4b` with chapters, files per chapter |
| L6 | `engine` | `audiobook.rs`: the project in the session, loading and saving it, the next-disc checks, the rip and build jobs, commands. Re-exports what the frontends need |
| L7 | `ui` | The audiobook page and its sidebar item |
| — | `tunebox-cli` | `audiobook` commands |

Jobs: `RipBookDisc` (rips one disc into the project, then ejects) and `BuildBook`. Both run on
worker threads and hand outcomes to `Session::tick`; both can be cancelled, and a cancelled job
leaves the project as it was before the job.

### Commands

Added to `crates/engine/src/commands.rs`, so menus, shortcuts and the CLI agree: Import as
Audiobook…, Join into One Chapter, Split Chapter Here, That Was the Last Disc, Build Audiobook,
Discard Audiobook.

### CLI

```text
tunebox-cli audiobook start --title "…" --author "…" [--narrator "…"] [--discs 12] [--lookup]
tunebox-cli audiobook rip                       # rips the disc in the drive as the next disc, ejects
tunebox-cli audiobook status
tunebox-cli audiobook chapters [--per-track | --per-disc | --from-names]
tunebox-cli audiobook join 1:14-2:3             # disc:track ranges
tunebox-cli audiobook split 2:4
tunebox-cli audiobook rename 7 "The Long Road"
tunebox-cli audiobook build --single | --per-chapter [--format aac|flac] [--bitrate 64] [--mono]
tunebox-cli audiobook discard
```

### The page

An "Audiobook" item appears in the sidebar under the CD while a project exists (also after a
restart). Its page shows:

- **The book**: cover, title, author, narrator and the other details, editable in place.
- **The discs**: a strip of discs, each done, being ripped (with progress), waiting, or with
  failed tracks. While waiting: "Insert disc 3 of 12" in large type.
- **The chapters**: the book's tracks grouped under chapter headings, with each chapter's name
  and length; select rows to join, split at a row, rename a heading.
- **The output**: single file or per chapter, the format options, the space needed, Build.

## Not in this plan: listening

A built book plays today as one long song. Listening comfortably needs more, planned separately:

- A media kind in the library (Music, Audiobook), an Audiobooks source in the sidebar, books
  left out of shuffle and smart playlists of music.
- Remember the playback position of each book.
- Chapters in playback: read them from `.m4b` files (craft-media's demuxer), show them, skip
  between them.
- Speed (1.25×, 1.5×), which needs time stretching.

## Phases

1. **The loop, lossless.** Model, project file, staged ripping, `CdDrive::eject`, next-disc
   checks, CLI, and output as FLAC files per chapter. Fully testable with the virtual drive.
   Does not wait for anything.
2. **The page.** The audiobook page in the UI and screenshots in `cargo xtask screenshots`.
3. **AAC.** When craft-audio has its encoder and craft-media writes chapters: the single `.m4b`
   and AAC files per chapter.
4. **Listening**, above.

## Tests to write

- Model: joining, splitting and renaming chapters, across disc boundaries; chapter start times
  from tables of contents; output names; next-disc checks for each row of the table above.
- Rip: three virtual discs swapped in turn, ejected after each; a disc changed mid-rip; a
  cancelled disc leaves no staged `*.part` files and the project as before.
- Resume: a project saved after disc 2, reloaded, continues with disc 3; a damaged
  `Project.json` is moved aside with the staged audio kept.
- Build: a chapter of several tracks decodes to exactly the concatenated disc audio (FLAC per
  chapter now; AAC within a tolerance later); the `.m4b` chapters read back at the computed
  times, before and after tags are written.
- Space: a full disk during staging or building fails with an error the listener can act on.

## Open questions

1. Rip on insertion, or wait for a click? This plan rips the next disc as soon as it is
   recognised, since the listener is in the middle of a book. A Preferences switch could turn
   that off.
2. Default output: 64 kbit/s mono AAC is the usual choice for speech, but some listeners want
   stereo (dramatised productions). Ask on the first build, or default and let them change it?
3. Keep the staged FLAC after building (as an archive copy), or always delete it? This plan
   deletes it once the book is in the library.
4. Narrator in composer is a convention, not a standard. Check what the players we care about
   (Apple Books, the Echo players, Android audiobook apps) actually show.
