# Feature parity with the classic desktop jukebox

TuneBox takes the music half of iTunes 10 (and earlier) as its target: the library, playlists,
CD import and syncing a player. Video (music videos, TV, films), the store, podcasts' downloads,
radio and sharing are out of scope.

**Shipped** means it works end to end and has tests. **Partial** says what is missing.

## Library

| Feature | Status | Notes |
|---|---|---|
| Add files and folders to the library | Shipped | Played in place by default; optionally copied into the media folder, organised by artist and album |
| Library folders, each read-only or writable | Shipped | Nested folders follow their own setting; scan for new songs ([files.md](files.md)) |
| Watch folders for changes automatically | Planned | Scanning is on request |
| Track list with sortable columns | Shipped | 20 columns; choose them in Preferences ▸ Columns |
| Column browser (Genres, Artists, Albums) | Shipped | ⌘B / Ctrl+B |
| Album grid view | Shipped | Embedded artwork; double-click plays the album |
| Search | Shipped | Every word must match; ⌘F / Ctrl+F |
| Star ratings, checkboxes | Shipped | Click the stars in place; unchecked songs are skipped in playback and sync |
| Play count, skip count, last played | Shipped | Counted as you listen |
| Get Info and editing tags | Shipped | One song or many at once; written into the files |
| Artwork pane | Shipped | Selected item or now playing |
| Delete from library (and disk) | Shipped | Asks first |
| Keep media folder organised after edits | Planned | Files are not yet renamed when tags change |
| Consolidate library, find missing files | Planned | |
| Cover Flow, album list view | Planned | Grid and list exist |
| Import an iTunes Library XML | Planned | |

## Playlists

| Feature | Status | Notes |
|---|---|---|
| Playlists, drag songs onto them | Shipped | Duplicates allowed, as in the classic app |
| Smart playlists | Shipped | Text, number and date rules; match all/any; limit by items, time or size, selected by 13 orders; only checked |
| Default smart playlists | Shipped | 90's Music, Classical Music, My Top Rated, Recently Added, Recently Played, Top 25 Most Played |
| Reorder songs in a playlist | Partial | Engine and tests exist; drag-to-reorder in the list is not wired up yet |
| Playlist folders | Planned | |
| Non-live smart playlists | Planned | Smart playlists are always live |

## Playback

| Feature | Status | Notes |
|---|---|---|
| Play, pause, next, previous, seek, volume | Shipped | Space, ⌘← ⌘→, ⌘↑ ⌘↓ |
| Shuffle, repeat all, repeat one | Shipped | |
| Status display (LCD) | Shipped | Title, artist/album, elapsed and remaining time, click to seek; shows import and sync progress |
| Formats | Shipped | MP3, AAC, ALAC, FLAC, WAV, AIFF, Ogg Vorbis (through craft-audio and symphonia) |
| Visualizer | Shipped | Five looks, track-info card, ⌘T / Ctrl+T; new looks are one file or a compile-time plug-in |
| Full-screen visualizer, run-time (WebAssembly) visualizer plug-ins | Planned | |
| Gapless playback, crossfade, Sound Check, equaliser | Planned | |
| Up Next / play queue editing, iTunes DJ | Planned | |

## CDs

| Feature | Status | Notes |
|---|---|---|
| Show the disc, its tracks and lengths | Shipped | cd-da-reader on macOS, Windows and Linux |
| Get Track Names | Shipped | MusicBrainz, by disc id; only when asked or when turned on |
| Edit names before importing | Shipped | |
| Import CD (choose tracks) | Shipped | FLAC, WAV or AIFF; tagged; atomic per track |
| Lossy encoders (AAC, MP3) | Planned | No permissively licensed pure-Rust encoders yet |
| Error correction / secure ripping | Partial | cd-da-reader retries failed reads; no AccurateRip verification yet |
| Play a CD directly | Planned | |
| Cover art from the Cover Art Archive | Planned | |

## Devices

| Feature | Status | Notes |
|---|---|---|
| Detect players (mass storage) | Shipped | Rockbox and players with a Music folder; managed ones by their manifest |
| Device page: capacity bar, sync settings | Shipped | Entire library or selected playlists; only checked songs; playlists as M3U8 |
| Sync | Shipped | Incremental; deletes only what TuneBox put there; checks free space first |
| Eject | Shipped | macOS (diskutil), Linux (gio); Windows asks you to use Safely Remove Hardware |
| Transcode for the player | Planned | |
| Manually manage music on the device | Planned | |
| MTP players | Planned | Mass-storage players only |
