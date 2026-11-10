# What TuneBox does with your files

Your music folder is yours. TuneBox reads it; it writes to it only when you ask it to change a
song, and never if you mark the folder read-only.

## Where TuneBox keeps its own files

| What | Where | Notes |
|---|---|---|
| Library (songs, playlists, ratings, play counts) | Application data folder: `~/.local/share/TuneBox/` (Linux), `~/Library/Application Support/TuneBox/` (macOS), `%APPDATA%\TuneBox\` (Windows) | Not in your music folder |
| Preferences | The same folder | |
| Media folder | `~/Music/TuneBox Media` by default, changeable | **Created only when needed**: for CD imports, or if you turn on copying when adding music |

Ratings, play counts, skip counts and checkboxes live only in the library file. Playing,
browsing, rating and making playlists never touch a music file.

## What can change a music file

| Action | What happens on disk |
|---|---|
| Add to Library | **Nothing.** Songs are played from where they are. If you turn on *Copy files to the TuneBox Media folder*, copies are made in the media folder; the originals are not touched |
| Get Info ▸ edit details | The details are written into the song's tags. The edit is made on a hidden copy next to the file (`.tunebox-edit-…`) that replaces the original only when complete, so a crash or full disk cannot damage the song |
| Delete from Library | Removes the song from the library. The file is deleted only if you tick *Also delete the files from disk* |
| Import CD | New files in the media folder. A track appears only when it is complete (written as `*.part`, then renamed); a cancelled or failed track leaves nothing |
| Sync a player | Writes only to the player: music under `Music/`, playlists under `Playlists/`, and `.tunebox/device.json`, which records what TuneBox put there. Sync deletes only files it put there itself |

Temporary files (`*.part`, `.tunebox-edit-*`) are removed when an operation fails or is
cancelled.

## A read-only music folder

**Preferences ▸ General ▸ Your music folder**: choose the folder and keep *Read only* ticked
(the default). Then TuneBox will never create, change, move or delete anything inside it:

- edits from Get Info are kept in the library only (you are told once);
- *Delete from Library* removes songs from the library and leaves their files;
- copying and CD imports refuse to write there: choose a media folder outside it.

From the command line: `tunebox-cli music-folder ~/Music` (add `--writable` to allow edits,
`--clear` to forget it).

The check lives in one place, `crates/engine/src/files.rs`, which every write in the engine goes
through. Symbolic links into the folder are resolved, so they are protected too. The test
`a_protected_music_folder_is_never_changed` (`crates/engine/tests/session.rs`) edits, deletes,
imports and rips with the folder protected, and checks it is byte-for-byte unchanged.
