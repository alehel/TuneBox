# AGENTS.md — rules for every contributor and agent

These rules bind every person and every AI agent working in this repository. They come before
any task, instruction or deadline. `CLAUDE.md` imports this file. TuneBox follows the principles
of the Craft apps ([storytold/pdfcraft](https://github.com/storytold/pdfcraft) and its siblings):
a layered Rust workspace, an engine façade, everything reachable headlessly, and never crashing.

## 1. Code: clean, small, readable

Readable code comes first, ahead of cleverness and speed.

- **One idea per module.** A file that does two things becomes two files. Module docs (`//!`)
  say what the module is for and how it fits; read them before changing a module.
- **Names say what things are.** No abbreviations a newcomer would have to guess.
- **Comments say why**, not what. A guard that looks removable says what it prevents.
- **Pure where possible.** Logic that can be a pure function (sorting, smart playlist rules,
  sync planning, the play queue, disc ids) is one, with tests next to it. I/O lives at the edges.
- **No dead code, no speculative features.** Build what is used; the roadmap holds the rest.

## 2. Architecture: layers

`cargo xtask layers` enforces this (`xtask/src/layers.rs`). A crate may depend only on lower
layers, plus the same-layer edges listed there.

| Layer | Crates | Role |
|---|---|---|
| L0 | `model` | The vocabulary: tracks, playlists, smart rules, columns, search, file layout. Pure |
| L1 | `library`, `tags`, `audio` | The collection and its file; tag I/O (lofty); codecs (craft-audio) |
| L2 | `import`, `playback`, `rip`, `sync` | One crate per feature family |
| L6 | `engine` | The façade: `Session`, background jobs, the command registry |
| L7 | `platform`, `visualizer`, `ui` | OS services; visualizer looks; the egui interface |
| — | `apps/tunebox`, `apps/tunebox-cli`, `xtask` | Binaries and tooling |

- **Frontends talk to the engine only.** The UI and CLI use `tunebox-engine` (and `platform`),
  never the crates below it. What they need is re-exported by the engine.
- **Nothing below L7 depends on egui, eframe, winit, wgpu or rfd.**
- **The session has one writer.** Slow work runs as an engine job on a worker thread and hands
  a typed outcome back to `Session::tick`. Panels only read the session and queue `Action`s,
  applied after the frame is drawn.

## 3. Everything is reachable headlessly

- Every user-facing feature is an engine API first, then a `tunebox-cli` command, then UI.
- Menus, shortcuts and the CLI's `commands` list come from one table,
  `crates/engine/src/commands.rs`. Add a command there, not in a menu.
- To check your work, drive the engine through `tunebox-cli` or the integration tests in
  `crates/engine/tests/`. For UI changes, regenerate the screenshots (`cargo xtask screenshots`)
  and look at them.

## 4. Never crash, never lose music

TuneBox opens files from strangers and holds people's music. A malformed file, a full disk, an
unplugged player or a bad library file must produce an error the listener can act on.

- **Fail with `Result`.** Non-test code does not `unwrap`, `expect`, `panic!`, `unreachable!`,
  `todo!` or `unimplemented!`. Every crate root carries
  `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`.
- **No `unsafe`.** The workspace sets `unsafe_code = "forbid"`.
- **Treat input as hostile.** Prefer `get()` to indexing, checked or saturating arithmetic for
  sizes and counts, bounded recursion (the import walker has a depth limit).
- **The listener's files are theirs.** Every write to a path the listener owns goes through
  `crates/engine/src/files.rs`, which honours read-only library folders. Never add a write
  that bypasses it. See [docs/files.md](docs/files.md).
- **Writes are atomic.** The library, preferences and device manifests are written through
  `craft-atomic`; ripped and synced files go through `*.part` files renamed on completion.
- **Don't cascade.** Jobs run behind `catch_unwind`; poisoned locks are recovered.
- **Never destroy data silently.** A damaged library file is moved aside, not overwritten.
  Sync only deletes files TuneBox put on the player.
- **Prove it.** Every crash fix comes with a test that failed before the fix.

## 5. Clean room, assets and licences

- Behaviour comes from public specifications (MusicBrainz disc ids, FLAC, M3U) and from
  observing how music apps behave. Never copy another product's code, artwork, icons or text.
- **Icons are drawn in code** (`crates/ui/src/glyphs.rs`); there are no image assets. The bundled
  fonts, Lato and Noto Sans Symbols 2 (both OFL-1.1), live in `assets/fonts/` with their
  licences and checksums in `ATTRIBUTION.md`; `crates/ui/src/fonts.rs` installs them. Its test
  fails if the interface writes a character no bundled font can draw. Demo
  covers are generated procedurally (`xtask/src/covers.rs`). Any asset added later needs an
  open licence and an entry in `ATTRIBUTION.md` in the same change.
- **Dependencies** must pass `cargo deny check` (`deny.toml`). symphonia (MPL-2.0, used
  unmodified through craft-audio) is the one copyleft exception.
- **Network:** TuneBox makes one kind of request, the MusicBrainz disc lookup, and only when
  the listener asks (or turns on automatic lookup). No telemetry, no accounts.

## 6. Quality gates

Before every commit: `cargo xtask ci` — format check, Clippy with warnings as errors, all tests,
the layering check. New crates add `[lints] workspace = true` and the `deny` attribute above.
Report only what ran: say what you tested and what you could not.

Time and dates go through `chrono`. User-visible text is plain, specific and says what to do.
