# Audiobooks from CDs: plan

Status: **phase 1 built** (the engine and `tunebox-cli audiobook`); the app's page is phase 2.
craft-libs provides the AAC encoder, the Apple Lossless (ALAC) encoder and MP4 chapter lists
(see [What craft-libs provides](#what-craft-libs-provides)). This document says what the
audiobook builder does, how it fits TuneBox's layers, and what is left; where phase 1 differs
from the plan, [What phase 1 built](#what-phase-1-built) says so.

## What the listener does

1. Inserts the first disc of the book and chooses **Import as Audiobook…**: a fourth button in
   the CD page header's row, after Import CD, Get Track Names and Get CD Cover; also in the
   File menu, or `tunebox-cli audiobook start`.
2. Types the book's details: title, author, narrator, and optionally series, number in the
   series, year, publisher, description and a cover. When the disc is on MusicBrainz, the
   details and the track names of *every* disc in the set are filled in from the release.
   TuneBox does not ask how many discs there are, nor how to group the tracks.
3. The disc's tracks start as one chapter. The listener may split them into chapters and leave
   out tracks that are not part of the story, before or while it rips (see
   [Editing the book](#editing-the-book)). **Rip Disc** starts the rip; nothing is ripped
   until then.
4. When the disc is done, TuneBox ejects it and says which disc is next: "Disc 2 is ripped.
   Insert disc 3." The listener either inserts it or says **That Was the Last Disc**; there is
   no third way out of that dialog. The new disc's tracks appear as one more chapter and wait
   for **Rip Disc** again.
5. Reviews the whole book as one list: moves chapter boundaries (a chapter that ran from one
   disc into the next), removes tracks ("Please insert disc 2"), and trims the start or end
   of a track.
6. Chooses the output and builds the book:
   - **One file with chapters**: a single `.m4b`.
   - **One file per chapter**: numbered files, tagged as parts of the same book.

The book is added to the library. Only then is the staged rip deleted.

### Why ripping and building are separate

The user's sketch builds the output as the discs go in. This plan rips every disc to lossless
files first and builds the book from them at the end. Reasons:

- **Never lose music.** A single `.m4b` spanning twelve discs would need one encoder held open
  across an hour or more of disc swapping. A crash, a quit, a full disk or a scratched disc on
  disc 11 would lose everything. Staged files survive all of these; the book resumes where it
  stopped.
- **The book can be edited until the end.** Chapters, removed tracks and trims are notes in
  the project, applied only when the book is built. A chapter that runs on into the next disc,
  or a mistake noticed on disc 7, is fixed by moving a boundary, with no re-ripping, and every
  edit can be undone until the build.
- **The output can change until the end.** Single file or files per chapter, bit rate, mono or
  stereo: all decided at build time, from the same rip.
- **Seamless chapters.** A chapter made of several CD tracks is fed to one encoder as one
  stream, so there is no gap or click where the CD tracks joined. Track files ripped with read
  offset correction join sample-exactly.

The cost is disk space while the book is in progress: a full CD of speech is roughly 300–400 MB
as FLAC, so a book of a dozen discs needs a few GB until it is built. TuneBox checks free space before each
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
| Discs | Per disc, in book order: its disc id, table of contents, track names, which tracks are staged |
| Removed tracks | The tracks left out of the book, as (disc, track) |
| Trims | Per track: how many samples to cut from its start and from its end |
| Chapters | The tracks where a chapter starts, as (disc, track), and each chapter's name |
| Output | Single file or per chapter; format, bit rate, mono or stereo |

Chapters are stored as **boundaries** in the book's track list, not as lists of tracks. Joining,
splitting and moving a chapter are then edits of one set, a chapter can cross a disc without
special cases, and every track always belongs to exactly one chapter.

A damaged `Project.json` is moved aside, like a damaged library file, and the staged audio is
kept so nothing has to be ripped again.

## Editing the book

Chapters and removed tracks can be edited before a disc is ripped, **while it rips**, and
after the last disc, over the whole book as one list; trims once a track is ripped. While a
disc rips the page stays fully editable, and its tracks show their progress. Between discs,
while TuneBox asks for the next one, nothing is edited (see [After each disc](#after-each-disc)).
Nothing touches the staged audio: the edits are applied when the book is built, so each one
can be undone until then.

### Chapters

Each disc starts as one chapter. From there the listener splits and joins: **Split Chapter
Here** starts a chapter at the selected track (at every selected track, when several are
selected), and **Join into One Chapter** makes one chapter of the selected tracks' chapters.
After the last disc, the list shows the book across disc boundaries, so the last tracks of
disc 1 and the first of disc 2 can be joined into the chapter they belong to.

Chapters are named "Chapter 1", "Chapter 2"… in order, and renumbered after every split and
join, until the listener types a name; a typed name is kept.

Two commands regroup the whole book at once, for books whose discs fit them: **One Chapter
per Track**, and **Chapters from Track Names** (MusicBrainz names such as "Chapter 4 (1)",
"Chapter 4 (2)" become one chapter). **One Chapter per Disc** goes back to the start. Like
every edit, each can be undone.

### Removing tracks

**Remove from Book** leaves a track out: "Please insert disc 2", "This is the end of disc 1",
the publisher's music. It stays in the list, greyed, and **Restore to Book** brings it back.
Every track is still ripped and staged, so a removal can be undone after its disc has been
put away.

When a removed track was the first of a chapter, the chapter starts at its next kept track. A
chapter with no kept tracks is left out of the output; its name is kept in case a track is
restored.

### Trimming

Often the announcement is not a track of its own but the first or last seconds of one ("End
of disc one. Please insert disc two."). **Trim…** cuts a stretch off the start or the end of a
staged track:

- A waveform of the start and the end of the track (craft-audio's peaks), with a handle for
  each cut, and a time field for precise values.
- **Play from Cut** plays the few seconds after the start cut, **Play to Cut** the few seconds
  before the end cut, so the listener hears exactly what the book will contain.
- The cuts are stored as sample counts and applied sample-exactly when the book is built. A
  trim that would leave nothing of the track is refused with the advice to remove the track
  instead.

Trimming needs the track's audio, so it is offered once the track is ripped.

### Suggestions

TuneBox points out likely edits but never makes them on its own: a suggestion is shown, and
the listener accepts or ignores it.

**Likely announcements.** A track is marked "Looks like an announcement" when it is short
(under 15 seconds) and is the first or last track of its disc, or when its MusicBrainz name
says so ("Please insert disc 2", "End of disc 1"; English phrases only to begin with). The mark
offers **Remove from Book** (**A**); **Accept All Suggestions** (⇧**A**) removes every marked
track and makes every proposed trim at once, each undoable as usual.

**Quiet moments.** While a track is ripped, TuneBox measures its loudness every 10 ms and keeps
the pauses: stretches of at least 0.2 s more than 30 dB quieter than the track's typical level
(its median), so a quietly recorded book is judged against itself. Only the pauses are saved
in the project, a few hundred per track. They are measured from the audio as it is written, so
finding them costs no extra pass over the disc. They are used in two ways:

- **Snapping.** A trim handle dragged near a pause snaps to its middle, so a cut never clips a
  word. Holding Alt (Option on a Mac) while dragging turns snapping off.
- **Proposed trims.** When the end of a disc's last track (or the start of the next disc's
  first track) has a pause within its last (or first) 20 seconds followed (or preceded) only by
  a short stretch of speech, the track is marked in the list ("Possible announcement in the
  last 6.5 s") and the trim editor opens with a cut at that pause, labelled "Possible
  announcement after this point". **E** plays the end, **A** accepts the cut from the list,
  **T** opens the editor to move it.

Both detectors are pure functions (track lengths, positions and names in; marks out; a loudness
envelope in, pauses out), tested on their own with synthetic speech-and-silence signals. The
thresholds are starting values, to be tuned on real audiobook discs.

### Chapter times

Chapter start times are a pure function of the tables of contents, the removed tracks and the
trims: every kept track contributes its sector count × 588 samples (which offset correction
does not change), minus its trims. The same function names the files of a book built one file
per chapter.

## Shortcuts

A book of 30 discs has 400 or more tracks, so every edit has a key and the editing is done
without the mouse. The keys below work on the book page while no name is being edited;
elsewhere the single letters do nothing. They come from the command table, like every
shortcut, and are listed in the Audiobook menu, which appears while the book page is shown.
**?** shows them all on one sheet.

| Key | Does |
|---|---|
| ↑ ↓, ⇧↑ ⇧↓ | Previous, next track; extend the selection |
| ⌥↑ ⌥↓ | Previous, next chapter start |
| ← → | Fold, unfold the selected track's chapter |
| ⌥← ⌥→ | Fold, unfold every chapter (folded, the book is a list of chapters) |
| **S** | Split: start a chapter at the selected track, or at each selected track |
| **J** | Join: the selected tracks' chapters become one; with one track selected, its chapter joins the one before |
| Return | Rename the selected track's chapter. While renaming: Tab saves and moves to the next chapter's name, ⇧Tab to the one before, Esc cancels |
| Delete | Remove the selected tracks from the book, or restore them if all are removed |
| **T** | Trim the selected track |
| **A** | Accept the suggestion on the selected tracks (remove the announcement, or make the proposed trim) |
| ⇧**A** | Accept every suggestion in the book (asks first, with the count) |
| Space | Play or pause, from the selected track |
| **P** | Play across the selected track's start: the last 4 s before it and the first 4 s of it, to check a chapter boundary |
| **E** | Play the last 8 s of the selected track, where announcements are |
| ⌘Z, ⇧⌘Z | Undo, redo any edit of the book |
| ⌘R | Rip Disc |
| ⌘D | That Was the Last Disc, in the "Insert disc N" dialog |
| ⇧⌘B | Build Audiobook… |
| ⌥⌘1, ⌥⌘2, ⌥⌘3 | Regroup: one chapter per disc, one per track, from track names |

In the trim editor:

| Key | Does |
|---|---|
| Tab | Switch between the start cut and the end cut |
| ← → | Move the cut 0.1 s; with ⇧, 1 s |
| ⌥← ⌥→ | Move the cut to the previous, next pause |
| **P** | Play across the cut (from it at the start, up to it at the end) |
| **X** | Play what is cut away |
| 0 | No cut |
| Return, Esc | Trim, cancel |

A fast pass over a large book: fold every chapter (⌥←), walk the disc chapters with ⌥↓,
unfold one, press **E** on its last track and **P** on candidates for a chapter start, **S**
to split, **Delete** for announcements, then Return and Tab through the chapter names.

Undo covers every edit of the book in this session, from a history of the project's states
(each is small: boundaries, removals, trims, names). It is not saved: after a restart the book
is as it was last edited. TuneBox has no undo elsewhere yet; this history is the book's own.

The command table needs two additions for these keys: an Option modifier (`Shortcut` has
Command and Shift today) and the keys Return, Tab, Escape, Home, End, 0 and ?. A test checks
that no two commands in the same place share a key.

## Inserting the next disc

The engine already looks for discs every few seconds (`Session::scan_drives`). While a project
is waiting for a disc, a newly read disc is checked before it is ripped:

| The disc is | TuneBox says |
|---|---|
| The next one (MusicBrainz medium position matches, or unknown) | Shows it as the next disc, one more chapter, ready to rip |
| One already ripped (same disc id) | "This is disc 2, which is already imported. Insert disc 3." Ejects it |
| Another disc of the same release (MusicBrainz says disc 5, disc 3 expected) | Asks: rip it as disc 5, or as disc 3 anyway |
| An earlier disc with tracks that failed | Offers to rip only the missing tracks (or to remove them from the book) |
| Unrelated (unknown disc, different release) | Asks whether it belongs to this book, since a disc without MusicBrainz data cannot be checked |

The checks are a pure function of the project, the new disc's id and the lookup result, tested
on their own.

### After each disc

When a disc is ripped and ejected, a dialog says so and names the next disc:

> Disc 2 is ripped and ejected.
> **Insert disc 3**
> Its tracks appear as soon as it is in.
>
> [That Was the Last Disc]

The dialog has two ways out and no others: inserting disc 3, which closes it by itself and
shows the new disc's tracks ready to rip, or **That Was the Last Disc**, which ends the
ripping. It cannot be dismissed to edit in between: editing happens while a disc rips, and
after the last one. When MusicBrainz lists more discs for the release than were ripped, That
Was the Last Disc asks first ("MusicBrainz lists 12 discs for this release; 11 are ripped.
Finish anyway?").

The book is saved after every disc, so quitting TuneBox at this point loses nothing: the
dialog is shown again, for the same disc, when the book is next opened. A disc that is not the
next one (see [Inserting the next disc](#inserting-the-next-disc)) is ejected or asked about
from within the dialog, which then goes on asking for disc 3.

### Ejecting

Eject used to be `tunebox_platform::eject_disc()`, which always opened `/dev/cdrom` on Linux and
did nothing on Windows. Now:

- `CdDrive::eject(&self) -> Result<(), DriveError>`. `PhysicalDrive` runs the system's eject
  tool for its own device (`eject <device>` on Linux, `drutil tray eject` on macOS); on Windows
  it says it cannot yet. Sending the drive the SCSI START STOP UNIT command itself needs a small
  addition to cd-da-reader (TuneBox forbids `unsafe`), which would also cover Windows.
  `VirtualChanger` takes its disc out, for tests.
- The rip job ejects the disc as its last step, so the CLI and the UI behave the same way. A
  drive that cannot eject (some laptop drives, or a failure) is not an error: the listener is
  asked to take the disc out.
- The existing Eject command goes through the same call (`Session::eject_disc`).

## Building the book

Building is an engine job. It decodes the staged FLAC files of the kept tracks in book order,
drops the trimmed samples, feeds each chapter (or the whole book) to the encoder, and writes the result through a `*.part` file renamed on
completion.

Chapter start times come from [Chapter times](#chapter-times). The build refuses to start
while a kept track has not been ripped, naming the disc and tracks.

### What craft-libs provides

craft-libs `9f10d43`, which TuneBox uses since `aa75839` (it pinned `0e4943c` before, which had
none of the encoders or chapters below).

| Piece | Where | What it does |
|---|---|---|
| AAC encoding | craft-audio `aac::Encoder` (feature `aac-encode`, part of `encode`) | AAC-LC, constant (`EncoderConfig::cbr`) or variable (`vbr`) bit rate, 1–8 channels, 44.1 kHz among the 13 AAC rates. A packet encoder: it returns access units and the `AudioSpecificConfig`, and the app muxes them. `priming_samples()` (1024) goes into the edit list, so files are gapless. No HE-AAC encoding, which matters only below about 48 kbit/s. Optional `rayon` feature encodes in parallel |
| Apple Lossless | craft-audio `alac::Encoder` (`alac-encode`) | Lossless packets and the magic cookie, muxed into MP4 the same way. Ported from Apple's Apache-2.0 reference, bit-exact with it and ffmpeg |
| MP4 container | craft-media `isobmff::Mp4Writer` (re-exported as `craft_audio::craft_media`, same commit) | 64-bit durations when needed (a 30-hour book overflows 32 bits at 44.1 kHz), edit lists (`TrackConfig::media_start` for the priming), `moov` in front ("fast start"), `mp4a` and `alac` sample entries |
| Chapters | craft-media `WriterOptions::chapters` (`Chapter { start, title }`) | Both forms players read: a disabled QuickTime chapter text track referenced by `tref/chap` (Apple's players) and a Nero `udta/chpl` list (VLC, ffprobe; checked against ffprobe in craft-media's tests). `read_chapters` reads them back, for tests and for listening later. **At most 255 chapters per file and 255 bytes per title** (`MAX_CHAPTERS`, `MAX_CHAPTER_TITLE_BYTES`) |
| Waveforms | craft-audio `peaks` | The trim editor's waveform |
| Opus, AC-3, MP1/MP2, ALAC decoding | craft-audio, native | Not needed here; Opus files now play (`aa75839`) |

The move to this commit is done (`aa75839`): TuneBox compiled unchanged, and `PcmWriter::write`
now refuses a block whose channels differ in length, as `FileEncoder::write` no longer pads
them. The audiobook build must hand the encoders equal channels, which the mono mixdown and
the trims (applied to both channels alike) do; a test covers it.

### One file with chapters (`.m4b`)

| Piece | How |
|---|---|
| Audio | AAC from `aac::Encoder`, or Apple Lossless from `alac::Encoder`, fed one continuous stream for the whole book |
| Container and chapters | `Mp4Writer` with `media_start` = the encoder's priming and `WriterOptions::chapters` from [Chapter times](#chapter-times), finished with fast start |
| Tags and cover | `tunebox-tags` (lofty), after muxing. lofty updates the `stco`/`co64` offsets of every track when it grows `moov`, so the chapter track stays valid; a test writes tags to a chaptered file and reads the chapters back with `read_chapters` |
| Media kind "Audiobook" | The `stik` atom set to 2, through lofty's generic atoms |

**The 255-chapter limit.** Both chapter forms are capped at 255 per file (the `chpl` count is one
byte). A single-file book with more chapters cannot be built as it is: Build Audiobook… says so
("This book has 312 chapters; one file holds at most 255. Join some chapters, or build one
file per chapter.") instead of dropping chapters silently. The chapter count is shown in the
header's details line, so the limit is visible while editing. One file per chapter has no
limit. Chapter names longer than 255 bytes are refused where they are typed.

Tags: title and album = book title, artist and album artist = author, **narrator in composer**
(decided), genre "Audiobook", description in Comments (`©cmt`; `desc`/`ldes` would need lofty's
MP4 tag directly, as the media kind does), publisher in Label. Series in grouping or the sort
album is a convention seen in audiobook tools; *confirm* against real players. Track and disc numbers are left out of a single-file book.

Encoding options:

| Format | Settings | A 14-hour book |
|---|---|---|
| AAC (default) | 64 kbit/s mono (the default, decided); 96 or 128 kbit/s stereo for dramatised productions | about 400 MB mono |
| Apple Lossless | Stereo, as on the disc | several GB (about half the CD audio, less for quiet speech) |

Mono mixes the two channels down before encoding. AAC uses constant bit rate, so the size shown
before building is exact to within the container's overhead.

### One file per chapter

Each chapter becomes one file: `Author/Book Title/NN Chapter Name.ext`, album = book title,
track number = chapter number of the total, no disc number (chapters run across discs). Format:

- AAC or Apple Lossless in `.m4b`, tagged as an audiobook.
- FLAC, for listeners who keep lossless files outside Apple's world.

## How it fits the layers

| Layer | Crate | New |
|---|---|---|
| L0 | `model` | `MediaKind` on tracks, and a Media Kind smart playlist rule. `audiobook`: book details, the track list, chapter boundaries (join, split, rename, per track, per disc, from names), removed tracks, trims and their limits, likely announcements, chapter start times, output file names. Pure, tested next to it |
| L1 | `library` | The media kind in the library file, and books recognised by `.m4b` when loading an older file |
| L1 | `tags` | Audiobook tags: media kind (read on import, written on build and from Get Info), narrator, description, chapters read back for tests |
| L1 | `audio` | An MP4 audio writer over craft-audio's AAC and ALAC encoders and craft-media's muxer, with chapters (writes through a `*.part` file, like `PcmWriter`); mono mixdown. Without chapters, the same writer gives the CD import its AAC and ALAC formats (an `.m4a` per track), a TODO item of its own: build it once for both |
| L2 | `rip` | Rip into a given staging folder without tagging or the library, handing each track's samples to a loudness meter as they are written (today's `rip` both writes and tags into the media folder; the reading part is shared). `CdDrive::eject`. The MusicBrainz parser keeps every medium of the release, not only the inserted one, for the names of discs not yet ripped. A virtual drive that can swap discs, for tests |
| L2 | `audiobook` (new) | Finding pauses in a loudness envelope; builds the book from the staged files: chapter streams, encoding, the `.m4b` with chapters, files per chapter |
| L6 | `engine` | `audiobook.rs`: the project in the session, loading and saving it, the next-disc checks, the rip and build jobs, playing a stretch of a staged track for trimming, `Source::Audiobooks`, commands. Re-exports what the frontends need |
| L7 | `ui` | The audiobook page and its sidebar item |
| — | `tunebox-cli` | `audiobook` commands |

Jobs: `RipBookDisc` (rips one disc into the project, then ejects) and `BuildBook`. Both run on
worker threads and hand outcomes to `Session::tick`; both can be cancelled, and a cancelled job
leaves the project as it was before the job.

### Commands

Added to `crates/engine/src/commands.rs`, so menus, shortcuts and the CLI agree, with the keys
listed under [Shortcuts](#shortcuts): Import as Audiobook…, Rip Disc, That Was the Last Disc,
Split Chapter Here, Join into One Chapter, Rename Chapter, Remove from Book / Restore to Book,
Trim…, Accept Suggestion, Accept All Suggestions, Play Across Start, Play End, Undo, Redo, One
Chapter per Disc, One Chapter per Track, Chapters from Track Names, Build Audiobook…, Discard
Audiobook. They live in a new **Audiobook** menu.

### CLI

```text
tunebox-cli audiobook start --title "…" --author "…" [--narrator "…"] [--series …] [--lookup [--cover]]
tunebox-cli audiobook rip                       # rips the disc in the drive as the next disc, ejects
tunebox-cli audiobook answer use|eject          # when a disc may not belong to the book
tunebox-cli audiobook done [--anyway]           # that was the last disc
tunebox-cli audiobook status
tunebox-cli audiobook details --narrator "…"    # change the book's details
tunebox-cli audiobook chapters [--per-disc | --per-track | --from-names]   # regroup (per disc is the start)
tunebox-cli audiobook join 1:14-2:3             # disc:track ranges
tunebox-cli audiobook split 2:4
tunebox-cli audiobook rename 7 "The Long Road"    # chapter 7, or the chapter holding a track: 2:1
tunebox-cli audiobook remove 2:1                # leave a track out of the book
tunebox-cli audiobook restore 2:1
tunebox-cli audiobook trim 2:14 --end 6.5       # seconds cut off the end (--start for the start)
tunebox-cli audiobook suggestions [--accept-all] # likely announcements and proposed trims
tunebox-cli audiobook pauses 2:14               # where the quiet moments are, for precise trims
tunebox-cli audiobook output --per-chapter --format flac   # or --single, --format aac|alac, --bitrate, --mono|--stereo
tunebox-cli audiobook build [same options as output]
tunebox-cli audiobook discard
tunebox-cli list --kind audiobook
```

`--virtual-disc` on `start` and `rip` uses a synthetic disc, a different one for each disc of the
book, for trying the loop without a drive. Undo is not offered on the command line: its history
lives in a session, and each run is a session of its own.

### The page

A book in progress appears in the sidebar under **Audiobooks** (see below), named after the
book, with "ripping" or "ready to build" beside it, also after a restart.

Its page follows the CD page (`cd_header.rs`, `cd_view.rs`), so ripping a book looks and
works like ripping an album: a header laid out as an album page, over a table of tracks.

**The header**, with the CD header's layout and its fields that show a frame only while hovered
or focused:

| CD header | Book header |
|---|---|
| The disc, or its cover | The book's cover, or the disc |
| Album, large and bold | Title |
| Artist | Author |
| Genre · Year · Compilation | Narrator · Series and number · Year |
| Details: "9 songs, 34 minutes · Imports with the FLAC Encoder · Drive read offset" | "3 discs ripped · disc 4 ready · 4 hours 20 minutes so far · Builds one .m4b, AAC 64 kbit/s mono" |
| Import CD · Get Track Names · Get CD Cover | **Rip Disc** · Build Audiobook… · Get Track Names · Get Cover |

Rip Disc is the primary button while a disc is ready; once the last disc is ripped, Build
Audiobook… takes its place. Build Audiobook… asks for the output (single file or per chapter,
format), shows the space needed and starts the build. Publisher, description and the
other details too long for the header are edited in Get Info for the book.

**The table**, as the CD page's, with these changes:

- The check box means "in the book": unticking a track is **Remove from Book**. Removed tracks
  are greyed.
- The tracks are grouped under chapter rows, each with the chapter's name (edited in place,
  like a track name), its length and the discs it spans; chapters fold to their row.
- A Disc column, since the list spans discs.
- The last column shows a track's state: ripping (with progress), ripped, failed, a trim
  ("−6.5 s at the end"), or the "Looks like an announcement" mark with its one-click removal.
  Trim… opens the waveform editor described under [Trimming](#trimming).
- While a disc rips, each of its tracks shows its progress, and every edit keeps working.

**Sharing with the CD header.** The pieces both headers need, the quiet field, the art (cover
or disc), the details line and the button row, move out of `cd_header.rs` into a shared module,
so the book header lays out its own fields without copying them.

**The CD page while a book is in progress.** When the disc in the drive has been taken as the
book's next disc, the CD header's details line says so ("Disc 3 of “The Long Road”") and
Import as Audiobook… becomes **Show Audiobook**. Import CD stays, for a listener who wants the
disc as music after all.

## Audiobooks in the library

Built books do not go into Music. The sidebar's LIBRARY heading gets a second category under
Music:

```text
LIBRARY
  ♪ Music
  ▤ Audiobooks
      The Long Road (ripping)      a book in progress, while there is one
```

**Audiobooks** is shown once the library holds a book or a book is in progress. It lists the
books, in the list or the album grid like Music. A single-file book is one row; a book built
one file per chapter is grouped by book (album) in the grid. In this view the Composer column
is headed **Narrator** and Artist is headed **Author**.

### Media kind

Each track gets a media kind, **Music** or **Audiobook** (`MediaKind` in `model`, a field of
`Track`):

- A book TuneBox builds is an Audiobook.
- An imported file is an Audiobook when its extension is `.m4b` or its MP4 media kind (`stik`)
  says so; everything else is Music.
- Get Info can change it, for books imported as MP3s or FLAC (several songs at once, as other
  fields). It is written to MP4 files (`stik`). Other formats have no standard tag for it, so
  there it lives in the library only: removing the file from the library and adding it again
  makes it Music.
- The library file gets the field with Music as its default. Its version stays 1 (older
  builds ignore unknown fields), and on loading a library without it, tracks whose file ends
  in `.m4b` become Audiobooks.

What changes for Music:

- `Source::Music` shows only Music, and a new `Source::Audiobooks` only books.
- Shuffle, and Up Next after a song, never step into an audiobook from Music.
- Smart playlists get a **Media Kind** rule ("is Music" / "is Audiobook"). Existing smart
  playlists keep matching what they match today, books included, since their rules do not
  mention a kind; the listener can add the rule.
- Commands that find songs by search (the CLI's `list`, `edit`, `play` and the like) search the
  whole library, Music and Audiobooks, so nothing becomes unreachable. `tunebox-cli list
  --kind audiobook` narrows to books.
- Syncing a player carries books as it carries music today.

## Not in this plan: listening

A built book plays today as one long song. Listening comfortably needs more, planned separately:

- Remember the playback position of each book.
- Chapters in playback: read them from `.m4b` files (craft-media's `read_chapters`, already
  there), show them, skip between them.
- Speed (1.25×, 1.5×), which needs time stretching.

## Phases

0. **craft-libs.** Done on main (`aa75839`): craft-libs `9f10d43`, Opus playback.
1. **The loop and the build.** Built (see below). Model, project file, staged ripping, `CdDrive::eject`, next-disc
   checks, chapters, removing and trimming, suggestions, the media kind and
   `Source::Audiobooks`, CLI, and every output: the chaptered `.m4b` (AAC or Apple Lossless)
   and files per chapter. Fully testable with the virtual drive.
2. **The page.** Audiobooks under Music in the sidebar, the shared header pieces moved out of
   `cd_header.rs`, the book page, the trim editor, and new scenes in `cargo xtask screenshots`
   (the book page while ripping, the "Insert disc 3" dialog, the trim editor, the sidebar).
3. **Listening**, above.

## What phase 1 built

| Layer | What |
|---|---|
| `model` | `audiobook`: the book, chapters as start points (split, join, rename, regroup), removals, trims, output chapters and file names, suggestions, next-disc checks, undo history. `MediaKind` on tracks; a Media Kind smart-playlist rule |
| `library` | The media kind in the library file; an older file's `.m4b` tracks become audiobooks once |
| `tags` | MP4's media kind (`stik`) read, written, and kept through every other tag edit |
| `audio` | `Mp4AudioWriter` (AAC or Apple Lossless with chapters, gapless), `read_mp4_chapters`, `LoudnessMeter` and pause finding, `mono_mix` |
| `rip` | `stage`, `CdDrive::eject`, `VirtualChanger`, every medium of a MusicBrainz release |
| `audiobook` (new) | `build` and `check` |
| `engine` | The book in the session, saved in `Audiobooks/<id>/`; edits with undo; the rip and build jobs; disc checks and questions; `Source::Audiobooks`, `Source::Library`; `set_media_kind`; eject through the drive |
| `tunebox-cli` | `audiobook …`; `list --kind`; searches cover the whole library |

Where it differs from the plan, or is not done yet:

- **Free space** is not checked before a disc or a build. A full disk fails the rip or the build
  with the system's message, and nothing half-written is kept.
- **Commands and shortcuts** (the Audiobook menu, the single-letter keys) are not in
  `commands.rs` yet: they come with the page in phase 2, with the Option modifier and the new
  keys.
- **Eject** uses the system's eject tool, not a command sent by cd-da-reader (see
  [Ejecting](#ejecting)); Windows cannot eject yet.
- **The description** goes in Comments, not `desc`/`ldes`.
- **Undo** is not on the command line (see [CLI](#cli)).
- **Tested** with virtual discs and fixtures only (`crates/engine/tests/audiobook.rs`, the
  crates' unit tests); never with a real drive, real audiobook discs, or a player reading the
  chapters.

## Tests to write

- Model: one chapter per disc to start with; splitting at one and at several tracks, joining,
  across disc boundaries; automatic names renumbered, typed names kept; regrouping; undo and
  redo; removing and
  restoring tracks, including the first track of a chapter and every track of one; trims,
  including ones that would leave nothing; likely announcements by length, position and name;
  pauses found in synthetic speech-and-silence envelopes, at different recording levels;
  snapping to the nearest pause; chapter start times from tables of contents,
  removals and trims; output names; next-disc checks for each row of the table above.
- Rip: three virtual discs swapped in turn, each waiting for Rip Disc, ejected after each,
  with "Insert disc N" naming the right disc; edits made while a disc rips apply to it, and a
  removed track is still ripped; the dialog comes back after a restart between discs; That Was the Last Disc, also with MusicBrainz
  listing more discs; a disc changed mid-rip; a
  cancelled disc leaves no staged `*.part` files and the project as before.
- Resume: a project saved after disc 2, reloaded, continues with disc 3; a damaged
  `Project.json` is moved aside with the staged audio kept.
- Build: a chapter of several tracks, with a removed track and trims, decodes to exactly the
  kept, trimmed disc audio joined (FLAC and Apple Lossless sample-exact; AAC within a
  tolerance, with the priming trimmed); a `.m4b` reads back its chapters with `read_chapters` at
  the computed times; 256 chapters in one file are refused with the advice, and accepted per
  chapter; a
  build with an unripped kept track is refused; the `.m4b` chapters read back at the computed
  times, before and after tags are written.
- Library: Music without books and Audiobooks with only books; `.m4b` and `stik` imports
  become books; an older library file loads with its `.m4b` tracks as books; shuffle from
  Music never plays a book; a smart playlist with and without the Media Kind rule; CLI search
  finds books.
- Shortcuts: no two commands on the book page, or in the trim editor, share a key.
- Space: a full disk during staging or building fails with an error the listener can act on.

## Decided

- Ripping waits for **Rip Disc**, so the listener can group the new disc's tracks first.
- TuneBox does not ask how many discs there are. After each disc it names the next ("Insert
  disc 3"); the only ways on are inserting it or That Was the Last Disc.
- Editing happens before and while a disc rips, and after the last disc; not between discs.
- TuneBox does not ask how to group: each disc starts as one chapter, edited from there.
- Editing is keyboard first, with a key for every edit ([Shortcuts](#shortcuts)).
- Default output: AAC at 64 kbit/s mono.
- The staged FLAC is deleted once the book is in the library.
- The narrator goes in the composer tag.
- After the last disc the whole book stays editable: chapters across discs, removed tracks,
  trims of a track's start and end.
- Books get their own **Audiobooks** category under Music in the sidebar, and leave Music.
- TuneBox suggests likely announcement tracks and trims, and snaps trim handles to quiet
  moments ([Suggestions](#suggestions)). It never applies a suggestion by itself.
