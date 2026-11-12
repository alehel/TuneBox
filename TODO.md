# TODO

What TuneBox lacks or has not proven yet. What already works is described feature by feature in
[docs/feature-parity.md](docs/feature-parity.md). Tick an item off when it ships, with its tests.

## Top priorities

1. CI on Linux, macOS and Windows.
2. Japanese, Chinese and Korean text (a system font as fallback).
3. Adding and changing album artwork.
4. MTP players: try real devices; Windows.
5. Volume levelling.
6. Installers.
7. Undo.

## Not yet verified

The biggest risk: these parts are built but have never run where it counts.

- [ ] Run on macOS and Windows. All testing so far is on Linux; the Windows code compiles
      (and passes Clippy) for the Windows target, but has never run.
- [ ] Set up CI (`.github/workflows`) building and testing on all three systems, so a broken
      Windows or macOS build is caught when it happens.
- [ ] Try real hardware: a CD drive, a portable player (mass storage and MTP), a sound card, a
      NAS. All of these have only been simulated.
- [ ] Open the native file dialogs (Add Folder, Locate…, the startup error box). They have never
      been opened.
- [ ] Measure large libraries (tens of thousands of songs): the library is one JSON file,
      rewritten about 2 seconds after each change, and the startup file check looks at every
      song.
- [ ] Check gapless joins of real MP3 and AAC albums. Encoder delay and padding are trimmed by
      craft-audio, but the tests have only WAV, FLAC and silent Opus fixtures. craft-audio's AAC
      encoder could now make AAC ones; MP3 still has no encoder.
- [x] Opus playback. craft-audio decodes Opus natively (craft-libs `9f10d43`). A libopus file
      decodes to its exact length, with pre-skip and end trimming applied, and a test in
      `tunebox-audio` checks a generated one. Not yet listened to on a real sound device.

## Playback

- [x] Gapless playback.
- [ ] Gapless across a change of sample rate or channel count. Today the output restarts in the
      new format (a short gap); converting instead needs a better resampler than the linear one.
- [ ] Crossfade.
- [ ] Volume levelling (Sound Check / ReplayGain).
- [ ] Equaliser.
- [ ] An editable Up Next queue, and a party-shuffle mode in the manner of iTunes DJ.
- [ ] Media keys and the system's Now Playing (MPRIS on Linux, Now Playing on macOS, media
      controls on Windows).
- [x] Full-screen visualizer: View ▸ Full Screen Visualizer (⇧⌘F / Ctrl+Shift+F), the Full
      Screen control, or a double-click. Window bookkeeping tested in `crates/ui/src/full_screen.rs`;
      real full screen has not been tried on macOS or Windows, nor with several monitors.
- [x] Visualizer plug-ins loaded at run time (WebAssembly).

## Library and editing

- [x] Add and change album artwork: Get Info (choose, remove, drop an image), Edit ▸ Choose
      Artwork…, the album grid's menu, `tunebox-cli artwork`. Tested in `tunebox-tags`
      (FLAC, WAV, AIFF) and `crates/engine/tests/session.rs`; the file picker and dropping a
      file have not been tried on a real desktop.
- [ ] Undo for edits, deletes and removing songs from playlists.
- [ ] Drag files in from Finder or Explorer.
- [ ] Drag to reorder songs in a playlist (the engine supports it; the list does not yet).
- [ ] Playlist folders.
- [ ] Smart playlists that are not live (a snapshot that does not update).
- [ ] Import and export playlists as M3U. Today playlists are only written to players during
      sync.
- [ ] Show Duplicates.
- [ ] Keep the media folder organised: rename and move files when their tags change. Tasks ▸
      Rename Files from Tags… does this on request, for chosen songs, by a pattern; it does
      not yet happen by itself after an edit.
- [ ] More tasks for the Tasks menu: Tags from File Names (the reverse of renaming), Find and
      Replace in Tags.
- [ ] Consolidate the library: copy every song into the media folder.
- [ ] Import an existing iTunes library (iTunes Library XML).
- [ ] Show lyrics while a song plays (they can be edited, not viewed).

## CD import

- [ ] AAC and ALAC rip formats (only FLAC, WAV and AIFF today). craft-audio now has pure-Rust
      AAC and ALAC encoders (`aac::Encoder`, `alac::Encoder`); what remains is the import
      setting and writing the result into an `.m4a` file.
- [ ] MP3 encoding. Waits for a permissively licensed pure-Rust encoder.
- [ ] AccurateRip verification that a rip is error-free. Blocked: the database is for approved
      programs only and TuneBox needs Illustrate's prior agreement; see
      [docs/accuraterip.md](docs/accuraterip.md).
- [x] Drive read offset correction, so rips are sample-exact (needed for AccurateRip, useful
      without it).
- [ ] Test and copy: read each track twice and re-read on a mismatch.
- [ ] Audiobooks from several CDs: chapters across discs, eject and ask for the next disc,
      one chaptered `.m4b` or a file per chapter ([docs/audiobooks.md](docs/audiobooks.md)).
      Built: the engine, `tunebox-cli audiobook` and the app's page with its shortcuts, tested
      with virtual discs (`crates/engine/tests/audiobook.rs`) and seen in screenshots. Left: a
      free space check, trying real discs, a real mouse and keyboard on the page, and players
      reading the chapters; then listening (resume, chapters in playback).
- [ ] Play a CD directly.
- [x] Cover art from the Cover Art Archive: Get CD Cover (or an opt-in setting) on the CD page,
      Find Album Artwork… for library albums, `tunebox-cli artwork find/fetch` and
      `cd-import --cover`. Tested with a fake web in `crates/engine/tests/covers.rs` and the
      parsers in `tunebox-metabrainz`; checked once against the live services (search and one
      cover). The picker has not been tried on a real desktop, only in screenshots.

## Portable players

- [x] MTP players on Linux and macOS (docs/mtp.md). Built and tested against a simulated
      device only.
- [ ] Sync a real MTP player: an Android phone (including its "Allow access" prompt and an SD
      card), an Android-based player, a Walkman. Check that songs and M3U8 playlists show up in
      the player's library, the speed, and what each does with a transfer cut off half way.
- [ ] MTP on a Linux desktop: check the "another program is using the player" advice with GNOME
      Files and KDE Dolphin, and the udev permission on a few distributions.
- [ ] MTP on macOS: never run on a Mac (Image Capture, Android File Transfer).
- [ ] MTP on Windows, through Windows Portable Devices (`winmtp` is the candidate; see
      docs/mtp.md).
- [ ] MTP playlist objects and metadata properties, for older non-Android MTP players that
      ignore M3U8 files and do not read tags.
- [x] Players that read files off a card (Snowsky Echo Mini and Nano): set up any drive, choose
      the player (docs/file-players.md). Not yet tried on a real Echo.
- [ ] Copy `.lrc` lyrics beside songs for players that show them (the Echo players).
- [ ] More player profiles (FiiO, HiBy, Shanling, older Sony) with the formats each plays.
- [ ] Convert songs to a smaller format for the player (or one it can play).
- [ ] Manage songs on the player by hand (today it is sync only).

## Interface

- [ ] Japanese, Chinese and Korean text: the bundled fonts do not cover them, so such names
      show as boxes. Load a system font as fallback (see `crates/ui/src/fonts.rs`).
- [ ] The app icon: waiting on the source image.
- [ ] Dark mode.
- [ ] Translations (English only today).
- [ ] Screen-reader support (enable egui's AccessKit integration).
- [ ] Cover Flow and an album-list view.

## Smaller limits

- [ ] While the app is open, only read-only `tunebox-cli` commands work; commands that change
      the library are refused by the library lock.
- [ ] Windows: drives mapped only in an administrator session are not seen (`net use` lists the
      current session's mappings).
- [ ] Locate… does not open the file picker in the right folder for a song on a share that is
      not connected (on purpose, since looking there could hang).

## Distribution

- [ ] Installers: `.dmg` (macOS), `.msi` (Windows), AppImage or Flatpak (Linux).
- [ ] Code signing and macOS notarisation, so systems do not warn on first launch.
- [ ] An update mechanism.
