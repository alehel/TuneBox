# AccurateRip verification: investigation

Status: **investigated, not built.** The AccurateRip database may only be used by programs its
owner has approved, and TuneBox has not been approved (see [Terms of use](#terms-of-use)). This
document records what verification would involve, so the work can start quickly if approval is
given, and what TuneBox can do in the meantime.

## What AccurateRip is

AccurateRip is a database of checksums of ripped CD tracks, run by Illustrate (the makers of
dBpoweramp). Rippers that take part send a checksum for every track they rip. A later rip of the
same disc is then compared with these checksums: if another person's rip on another drive came
out bit-for-bit the same, both are very probably free of read errors. The number of matching
submissions is the *confidence*. The site describes it as verifying "ripped tracks against an
Internet database, making sure they are error free" and reports 5.3 million unique discs and
536 million accurate rips ([accuraterip.com][home], read November 2026).

Drives do not report every read error themselves (even drives with C2 error reporting), so
AccurateRip catches errors that secure-ripping retries alone miss ([accuraterip.com][home]).

## How it works

The descriptions below are the ones repeated across public descriptions of the format
([Hydrogenaudio wiki][ha-wiki], [Hydrogenaudio forum][ha-79082], [dBpoweramp forum][dbp-dev]).
**In this session those pages could not be fetched** (Hydrogenaudio and Wikipedia sit behind a
bot check, and the dBpoweramp forum refused automated requests), so the details here come from
search-result summaries of those pages and from general knowledge of the published format. Every
detail marked *(confirm)* must be checked against the sources, and against real database replies,
before code relies on it. No source code of any ripper was read.

### Disc identifiers

A database entry is found by three numbers computed from the table of contents (TOC). Offsets
are in sectors, counted from the start of track 1's addressable area, i.e. *without* the
150-sector lead-in that the MusicBrainz and FreeDB ids add (`DiscToc::start_lba` in TuneBox is
counted the same way).

| Id | Computed as |
|---|---|
| AccurateRip id 1 | The sum of every track's start offset, plus the lead-out offset (32-bit, wrapping) |
| AccurateRip id 2 | The sum of `max(offset, 1) × track number` over the tracks, plus `lead-out × (track count + 1)` (32-bit, wrapping) — "the sum of all the indexes times their track number" ([Hydrogenaudio forum][ha-79082]) |
| FreeDB/CDDB id | The classic FreeDB id; TuneBox already has `tunebox_rip::freedb_id` |

How a trailing data track (enhanced CD) enters the ids and the track count is described
inconsistently *(confirm)*. Several pressings may share an id, and different discs may collide;
this is harmless because the reply can hold several entries side by side
([Hydrogenaudio forum][ha-79082]).

### Request and reply

The lookup is a plain HTTP GET of a static file:

```
http://www.accuraterip.com/accuraterip/a/b/c/dBAR-ttt-iiiiiiii-jjjjjjjj-kkkkkkkk.bin
```

where `a`, `b`, `c` are the lowest, second-lowest and third-lowest hex digits of id 1, `ttt` the
track count in three decimal digits, and `iiiiiiii`, `jjjjjjjj`, `kkkkkkkk` ids 1, 2 and the
FreeDB id in eight lowercase hex digits *(confirm)*. A disc that is not in the database gives
HTTP 404.

The reply is zero or more back-to-back entries, one per pressing or checksum version, each:

| Bytes | Field |
|---|---|
| 1 | Track count |
| 4 | AccurateRip id 1 (little-endian) |
| 4 | AccurateRip id 2 |
| 4 | FreeDB id |
| 9 per track | Confidence (1 byte), track checksum (4 bytes), offset-finding checksum (4 bytes) |

(13-byte header and 9 bytes per track, as summarised from public descriptions; byte order
*(confirm)*.) An entry's header must be checked against the ids asked for, and an entry with the
wrong track count must be rejected rather than read past: the reply is untrusted input.

### Track checksums (v1 and v2)

Each stereo frame (one left and one right 16-bit sample) is read as one 32-bit little-endian word,
left in the low half. Frames are numbered from 1 within the track, and each word is multiplied by
its number:

- **v1:** the sum of the products, keeping the low 32 bits of each.
- **v2:** each product is computed in 64 bits, and its high and low 32-bit halves are both added.
  v2 was added later because v1 ignores the high bits, so some sample errors in late frames
  cancel out. The database holds both kinds; a track matches if either does.

The first frames of the first track and the last frames of the last track are left out, because
a drive with a read offset cannot read the lead-in or lead-out and pads with silence there: the
public summary says the checksum "ignores the first and last 1/15 of a second of audio on the
disc" ([Hydrogenaudio forum][ha-53150]), i.e. 5 sectors (5 × 588 = 2,940 frames). The exact
boundary (2,939 or 2,940 frames at the start) is the classic off-by-one of AccurateRip
implementations *(confirm)*. Without a real database entry to test against, this cannot be
settled here.

The *offset-finding checksum* is a checksum of one sector a fixed way into each track, stored so a
ripper can discover a drive's offset by trying shifts against a known disc *(confirm which
sector)*.

### Confidence and pressings

Confidence is the number of submitted rips that agree with that checksum. A disc may have
several entries (pressings, or v1 and v2), each with its own confidence. A sensible display is
the highest confidence among entries that match, and the total among all entries for context
("matches 12 of 14 submissions").

## Drive read offsets

Every drive reads audio a fixed number of samples early or late. AccurateRip says: "Each CD drive
reads audio discs slightly out (a number of samples), if your CD drive supports 'Accurate Stream'
it will be a constant value, this value tends to be the same for each particular make and model
of CD Drive" ([drive offsets][offsets]). Since the checksum multiplies each frame by its position,
a rip shifted by even one sample does not match. **Without offset correction, rips from most
drives will not match.**

Where an offset can come from:

1. **AccurateRip's drive offset list** ([drive offsets][offsets]), about 4,900 drives, e.g.
   "+91" or "+738". Fetching it is a second request to the same site, and it is Illustrate's
   data: copying it into TuneBox would need the same agreement. Some drives are listed as
   *Purged* because their offset is not constant.
2. **A manual setting** in Preferences, default 0, with a note to look the drive up in that list.
   This needs no request.
3. **Detection with a known disc:** rip a disc that is in the database and try a range of
   offsets against the offset-finding checksums. This also uses the database (the site says
   drives without a listed offset need "3 matching key disks").

The correction must be applied to the **ripped audio itself**, not only to the checksum: a
"verified" result must describe the file that was written. Shifting by *n* samples means
reading *n* samples of the neighbouring track (and padding with silence at the disc's start or
end).

### What TuneBox does (offset correction exists)

TuneBox has option 2: a manual setting, default 0, applied to the rip itself.

- **Sign.** An offset of +*n* means the drive delivers audio *n* samples early: asked for sample
  *k*, it returns the sample that really sits at *k* − *n*. TuneBox therefore asks for the track
  *n* samples later. A negative offset shifts the other way. One sample is one stereo frame
  (4 bytes); a sector holds 588.
- **Reading.** `CdDrive::read_sectors` reads any sector range (cd-da-reader's
  `read_sector_range`, two seconds at a time). The pure function `shifted_read`
  (`crates/rip/src/offset.rs`) plans each track: which sectors to read, how many samples to drop
  at each end of the read, and how much silence to write where the shift reaches before sector 0
  (the lead-in) or past the end of the disc's audio (the lead-out, or the end of the audio
  session on an enhanced CD), which drives cannot read. Reading by range rather than carrying
  one track's tail into the next works for any selection of tracks in any order.
- **Which drive.** The offset is saved per drive in preferences (`read_offsets`). Offsets belong
  to a make and model, so on Linux the drive is known by the vendor and model the kernel
  publishes in sysfs (`/sys/class/block/sr0/device/vendor` and `model`, the SCSI inquiry
  strings), e.g. "PLEXTOR DVDR PX-716A". cd-da-reader does not report the model, so on macOS
  and Windows (and on Linux if sysfs has nothing) the drive is known by its device path, and the
  setting says so.
- **Where it is set.** Preferences ▸ Importing ▸ Drive read offset (for the drive holding a
  disc), or `tunebox-cli cd-offset --set=+667`. Values beyond ±10,000 samples are refused with
  an explanation. The CD page shows the offset the import will use. The help text links to
  AccurateRip's drive list; TuneBox does not fetch or copy it.
- **Tests.** `VirtualDrive` can simulate a read offset (`with_read_offset`) over a disc whose true
  audio is fixed; rips with the matching correction equal the true audio sample for sample,
  apart from the silence padded exactly where the shift reaches the lead-in or lead-out
  (`crates/rip/src/rip.rs`, `crates/engine/tests/session.rs`).
- **Not yet confirmed on a real drive:** the sign convention against a drive with a known
  offset, reading of the lead-in and lead-out areas (some drives can; TuneBox always pads), and
  the model strings on real hardware.

## Terms of use

Illustrate states two things publicly:

- "Access to AccurateRip is free for non-commercial usage, commercial usage is restricted to
  prior agreement" ([accuraterip.com][home]).
- "Access to the AccurateRip database, using 3rd party applications (such as adopting open
  source access routines) requires prior agreement, our reasons are: To ensure the server is not
  swamped and can handle requests, Checking compatibility of access, To preserve the good name
  and quality behind AccurateRip. AccurateRip's database in it's final state, is copyrighted
  Illustrate, access is restricted to approved programs." ([3rd party access][3rdparty])

TuneBox is a third-party application, free and open source (MIT/Apache-2.0). Being
non-commercial does not lift the second condition: **third-party use needs Illustrate's prior
agreement, and TuneBox has none.** Building the lookup now would mean shipping a client that
uses a copyrighted database its owner restricts to approved programs. That is why this work
stops here.

Also relevant: TuneBox's licence lets anyone reuse its code. An approval given to TuneBox may
not extend to forks, so an approved lookup should identify itself as TuneBox and be easy for a
fork to switch off.

**To unblock:** the maintainer writes to Illustrate (contact form linked from the
[3rd party access page][3rdparty]) asking for approval of read-only lookups by TuneBox, made only
when the listener asks, with no submissions. Only with a written yes should phase 2 start.

## How it would fit TuneBox

If approval is given, the design follows TuneBox's layers and its MusicBrainz lookup:

- **Pure code in `crates/rip`:** an `accuraterip` module with the three disc ids (from `DiscToc`,
  beside `discid.rs`), a v1/v2 checksum accumulator fed frame by frame, and a parser for the reply
  that never indexes past the data it was given. Unit tests on hand-computed small cases and a
  hand-built reply; `VirtualDrive` discs for self-consistency.
- **During the rip (`rip.rs`):** the accumulator sees the same PCM chunks that go to the encoder,
  before encoding, so the checksum describes exactly what was written. It costs two multiplies
  and adds per frame: negligible next to FLAC encoding. Checksums go into `RipReport`, per track.
- **The lookup:** `crates/rip/src/accuraterip.rs` next to `musicbrainz.rs`, using `ureq`, behind
  an `accuraterip` cargo feature mirroring `musicbrainz` in `crates/rip` and `crates/engine`. It
  sends nothing but the URL with the three ids. Note that the server is plain HTTP, so those ids
  are visible on the network.
- **Engine:** a `Session::verify_rip` job (and a `Prefs` setting, off by default, to verify after
  every import), a command in `crates/engine/src/commands.rs`, and per-track results held in
  `CdState`.
- **CLI:** `tunebox-cli cd-import --verify`.
- **UI (`crates/ui/src/panels/cd_view.rs`) and notices:** a per-track column: "Accurately ripped
  (confidence 12)", "Not in the AccurateRip database", "Does not match: re-rip, or check the
  drive offset in Preferences".
- **Policy:** `AGENTS.md` §5 (Network), the README and `docs/files.md` would name it as the
  second kind of request TuneBox makes, only when asked.

### What it could not verify

- **Real drives.** No CD drive is available here; offsets and real read errors were never seen.
- **Data tracks** are never checksummed; enhanced CDs need the id rules confirmed.
- **Discs not in the database** (rare or new pressings, home-burned CDs) get no verdict, only
  "not in the database".
- **Pre-emphasis, HTOA** (audio hidden before track 1) are outside what AccurateRip checks.

## What TuneBox can do without the database

- **Offset correction** (above, built): makes rips sample-exact regardless of AccurateRip.
- **Test and copy:** read each track twice and compare (TuneBox's own checksums, e.g. a CRC of
  the PCM), re-reading on a mismatch. This catches inconsistent reads entirely offline. It
  cannot catch an error the drive makes the same way twice, which is AccurateRip's strength.
- Other rip databases (for example the CUETools database) were not investigated; their terms
  would need the same check.

## Reachability from this environment

`https://www.accuraterip.com/` (the home, 3rd-party access and drive offset pages) answered with
HTTP 200 from this environment. **No database file was requested**: without Illustrate's
agreement even one test lookup would be the third-party access the terms restrict. Nothing has
been verified against the live database.

## Sources

Read in this investigation (November 2026):

- `home`: AccurateRip home page — <https://www.accuraterip.com/>
- `3rdparty`: "3rd Party Access of AccurateRip's Database" — <https://www.accuraterip.com/3rdparty-access.htm>
- `offsets`: CD drive offsets — <https://www.accuraterip.com/driveoffsets.htm>

Public descriptions of the format, found by search but blocked to automated fetching here
(the format details above rely on their search summaries; confirm against them before building):

- `ha-wiki`: Hydrogenaudio Knowledgebase, "AccurateRip" — <https://wiki.hydrogenaud.io/index.php?title=AccurateRip>
- `ha-53150`: Hydrogenaudio forum, topic 53150 — <https://hydrogenaud.io/index.php/topic,53150.0.html>
- `ha-79082`: Hydrogenaudio forum, topic 79082 — <https://hydrogenaud.io/index.php/topic,79082.0.html>
- `dbp-dev`: dBpoweramp forum, "AccurateRip developments" — <https://forum.dbpoweramp.com/forum/dbpoweramp/cd-ripper/14983-accuraterip-developments>

[home]: https://www.accuraterip.com/
[3rdparty]: https://www.accuraterip.com/3rdparty-access.htm
[offsets]: https://www.accuraterip.com/driveoffsets.htm
[ha-wiki]: https://wiki.hydrogenaud.io/index.php?title=AccurateRip
[ha-53150]: https://hydrogenaud.io/index.php/topic,53150.0.html
[ha-79082]: https://hydrogenaud.io/index.php/topic,79082.0.html
[dbp-dev]: https://forum.dbpoweramp.com/forum/dbpoweramp/cd-ripper/14983-accuraterip-developments
