# TuneBox architecture

How TuneBox is put together, and where a new feature goes. The rules that keep it this way are
in [AGENTS.md](../AGENTS.md).

## Shape

One Cargo workspace: twelve library crates in `crates/`, two binaries in `apps/`, tooling in
`xtask/`. Rust only, no `unsafe`, no panics outside tests.

```text
            apps/tunebox ─────────┐            apps/tunebox-cli
                 │                │                   │
   L7      tunebox-ui ──▶ tunebox-platform ◀──────────┤
                 └──────▶ tunebox-visualizer
                 │                │                   │
   L6            └──────▶ tunebox-engine ◀────────────┘
                                  │
   L2   import ◀── rip      playback       sync
          │         │          │             │
   L1   library    tags      audio ──▶ craft-audio (symphonia, FLAC/WAV/AIFF)
          │         │          │
   L0     └──── tunebox-model ─┘
```

| Crate | What it does |
|---|---|
| `tunebox-model` | Tracks, metadata, ratings, playlists, smart playlist rules and evaluation, track-list columns and sorting, search, organised file names. Pure and deterministic |
| `tunebox-library` | The collection: unique ids, one track per file, playlists that only reference real tracks; the column browser; summaries; the versioned JSON library file |
| `tunebox-tags` | Reads and writes metadata and cover art (lofty) in MP3, MP4/M4A, FLAC, Ogg, Opus, WAV, AIFF |
| `tunebox-audio` | Streaming decode by path, seekable; lossless block-by-block encoding through `*.part` files |
| `tunebox-import` | Finds audio files, reads tags, copies into `Artist/Album/NN Title.ext` |
| `tunebox-playback` | The play queue (shuffle, repeat), and a player thread feeding a lock-free ring to a cpal callback |
| `tunebox-rip` | CD drives (cd-da-reader, or a virtual drive), tables of contents, MusicBrainz disc ids and lookup, ripping |
| `tunebox-sync` | Recognises players, plans a sync as a diff against the device manifest, executes it, writes M3U8 |
| `tunebox-engine` | The `Session` façade, background jobs, preferences, notices, the command registry |
| `tunebox-platform` | App folders, mounted volumes (sysinfo), reveal in file manager, eject |
| `tunebox-visualizer` | The `Visualizer` trait, the meter that turns the audio tap into smoothed bands, and the built-in looks ([visualizers.md](visualizers.md)) |
| `tunebox-ui` | The egui desktop interface |

## The session and its threads

```text
 UI thread                         worker threads (one per job)
 ─────────                         ────────────────────────────
 frame:                            import  ─┐
   session.tick() ◀── outcomes ───  rip    ─┤ JobContext: progress, cancel flag
   panels read session             sync    ─┤ catch_unwind → error outcome
   panels queue Actions            lookup  ─┘
   app applies Actions ──▶ session (the only writer of the library)

                                   player thread
   session ──commands──▶ Player ── Decoder → Converter ──ring──▶ cpal callback
           ◀─status/events──
```

- `Session` owns the `Library`, the `Player`, the device list, the disc in the drive and the job
  table. It is not shared between threads.
- A job gets copies of what it needs (paths, a snapshot of tracks, the device manifest) and
  returns a typed `Outcome`. `Session::tick` applies outcomes, follows the player (counting
  plays and skips, advancing the queue) and autosaves the library two seconds after the last
  change.
- `Session::revision` changes whenever anything a view shows changes, so the UI caches sorted
  rows and the browser until it does.

## Playback

The player thread opens a `Decoder`, asks the output for the file's own sample rate and channel
count (falling back to the device default with conversion), and decodes about half a second
ahead into an `rtrb` ring. The output callback never locks or allocates: it applies the volume
and counts played frames, from which the position is derived. Pause keeps the ring; seek and
stop flush it. `NullOutput` drains the ring without a sound device, for tests, headless use and
demo mode.

The callback also copies what it plays, mixed to mono and before the volume, into a second
ring: the visualizer tap. `Player::recent_audio` reads it; `tunebox_playback::analysis` turns it
into spectrum bands; `tunebox-visualizer`'s `Meter` smooths them into the `Frame` a look paints.

## Files TuneBox writes

What may touch the listener's own files, and the read-only music folder, are described in
[files.md](files.md); `crates/engine/src/files.rs` is the single checkpoint.


| File | Where | Format |
|---|---|---|
| Library | `<app data>/TuneBox/TuneBox Library.json` | `{format: "tunebox-library", version, next_id, tracks, playlists}`; unknown newer versions are refused, damaged files moved aside |
| Preferences | `<app data>/TuneBox/Preferences.json` | Preferences; unreadable files fall back to defaults |
| Media | `~/Music/TuneBox Media/Artist/Album/NN Title.ext` | Copies and CD imports only; created when first needed |
| Device manifest | `<player>/.tunebox/device.json` | Name, sync settings, every file TuneBox copied (with source size and time), playlists written |
| Playlists on players | `<player>/Playlists/*.m3u8` | Extended M3U, UTF-8, paths relative to the playlist |

## Adding a feature

1. Model the data in `tunebox-model` if it is new vocabulary (pure, with tests).
2. Put the logic in the feature crate it belongs to, or a new crate in the right layer (add it
   to `xtask/src/layers.rs`).
3. Expose it on `Session`; slow work becomes a job with an `Outcome` variant.
4. Add a command to `crates/engine/src/commands.rs` if it is a user action, and a
   `tunebox-cli` subcommand.
5. Draw it in `tunebox-ui`: a panel reads the session and queues an `Action`; `app.rs` applies it.
6. Add a test at each level, an entry in [feature-parity.md](feature-parity.md), and run
   `cargo xtask ci`.
