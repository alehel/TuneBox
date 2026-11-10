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
| Track list with sortable columns | Shipped | 20 columns; choose them in Preferences ▸ Columns |
| Column browser (Genres, Artists, Albums) | Shipped | ⌘B / Ctrl+B |
| Album grid view | Shipped | Embedded artwork; double-click plays the album |
| Search | Shipped | Every word must match; ⌘F / Ctrl+F |
| Star ratings, checkboxes | Shipped | Click the stars in place; unchecked songs are skipped in playback and sync |
| Play count, skip count, last played | Shipped | Counted as you listen |
| Get Info and editing tags | Shipped | One song or many at once; written into the files |
| Missing-file marks | Shipped | Missing (amber !) versus not connected (drive or network share away), told apart per place with timeouts so a dead share never hangs the app; network shares (NFS, SMB, AFP, WebDAV, SSHFS, Windows mapped drives and `\\server` paths) remembered in the library. Playback, sync, refresh and relinking respect both. `tunebox-cli missing` |
| Locate… a missing file | Shipped | From the song menu, File menu, Get Info, or by playing it; keeps ratings, plays and edits. Other missing songs that moved the same way are linked too, each only when a file of the same size and format is there. `tunebox-cli locate` |
| Watch library folders | Shipped | On by default (Preferences ▸ Library). Songs that appear are added, songs other programs change are read again, moved songs are relinked with their ratings and plays, deleted songs stay listed. `tunebox-cli folders watch` does the same without the window |
| Refresh from Files | Shipped | Reads chosen songs' tags again after another program changed them (File menu, song menu, `tunebox-cli refresh`). Ratings and play counts stay; edits kept in TuneBox only are kept unless you choose otherwise |
| Additional tag fields | Shipped | 31 more (credits, classical work and movement, release, sort names, mood and key, lyrics, encoding) under *Show all fields*; pin any to always show (Preferences ▸ Get Info). Fields a format cannot hold are not offered; dates and numbers are checked. Songs added before these fields were read get them on the next scan or import that finds them |
| Artwork pane | Shipped | Selected item or now playing |
| Add, change and remove album artwork | Shipped | Get Info's artwork (click, *Choose…*, *Remove*, or drop an image on it), Edit ▸ Choose Artwork… / Remove Artwork, the album grid's right-click menu, `tunebox-cli artwork set/remove/export`. One song or a whole album; written into each file atomically in the background, one failure never stops the rest. JPEG and PNG kept as they are; GIF, BMP and WebP converted; pictures over 1400 px scaled down. Songs in read-only folders keep the cover in TuneBox ([files.md](files.md)) |
| Delete from library (and disk) | Shipped | Asks first |
| Rename files from tags | Shipped | Tasks ▸ Rename Files from Tags…: a pattern of fields (`{album-artist\|artist}/{album}/{track:02} {title}`), ready-made patterns, files kept in their library folder or moved into the media folder, a preview with skipped songs and conflicts, lyrics and cue files moved along, emptied folders removed. Never replaces a file or touches a read-only folder. `tunebox-cli rename` |
| Keep media folder organised after edits | Partial | On request (Rename Files from Tags); files are not yet renamed by themselves when tags change |
| Consolidate library (copy every song into the media folder) | Planned | Finding missing files ships: see Missing-file marks and Locate… |
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
| Gapless playback | Shipped | The next song is opened ahead and follows in the same output stream, switching at the exact frame it is heard. A change of sample rate or channel count restarts the output, a short gap |
| Crossfade, Sound Check, equaliser | Planned | |
| Up Next / play queue editing, iTunes DJ | Planned | |

## CDs

| Feature | Status | Notes |
|---|---|---|
| Show the disc, its tracks and lengths | Shipped | cd-da-reader on macOS, Windows and Linux |
| Get Track Names | Shipped | MusicBrainz, by disc id; only when asked or when turned on |
| Edit names before importing | Shipped | |
| Import CD (choose tracks) | Shipped | FLAC, WAV or AIFF; tagged; atomic per track |
| Lossy encoders (AAC, MP3) | Planned | No permissively licensed pure-Rust encoders yet |
| Error correction / secure ripping | Partial | cd-da-reader retries failed reads; no offset correction; AccurateRip needs the database owner's approval ([investigation](accuraterip.md)) |
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
| MTP players | Built (Linux, macOS) | Android-based players and phones, recent Walkmans: found over USB, set up and synced like mass-storage players, each storage (internal, SD card) a player of its own. Built and tested against a simulated device; not yet run with real hardware. Windows needs Windows Portable Devices: not yet ([docs/mtp.md](mtp.md)) |
