# What TuneBox does with your files

Your music folders are yours. TuneBox reads them; it writes to them only when you ask it to
change a song, and never to a folder you mark read-only.

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
| Get Info ▸ edit details | The details are written into the song's tags. Only fields you changed are written: the additional fields (credits, label, sort names, lyrics…) you did not touch stay exactly as they are, and a field the format has no place for is not offered. The edit is made on a hidden copy next to the file (`.tunebox-edit-…`) that replaces the original only when complete, so a crash or full disk cannot damage the song |
| Watching library folders | **Nothing.** New songs are added to the library, changed songs are read again, a moved song is relinked (keeping its ratings and plays), and a deleted song stays in the library |
| Refresh from Files | **Nothing.** The songs' tags are read again and the library takes what they say |
| Delete from Library | Removes the song from the library. The file is deleted only if you tick *Also delete the files from disk* |
| Import CD | New files in the media folder. A track appears only when it is complete (written as `*.part`, then renamed); a cancelled or failed track leaves nothing |
| Sync a player | Writes only to the player: music under `Music/`, playlists under `Playlists/`, and `.tunebox/device.json`, which records what TuneBox put there. Sync deletes only files it put there itself |

Temporary files (`*.part`, `.tunebox-edit-*`) are removed when an operation fails or is
cancelled.

## Library folders, read-only or writable

**Preferences ▸ Library ▸ Library folders** lists the folders your music lives in. You can list
as many as you like, and each is **read only** (the default for a new folder) or **writable**:

- In a **read-only** folder TuneBox never creates, changes, moves or deletes anything. Edits
  from Get Info are kept in the library only (you are told once); *Delete from Library* removes
  songs from the library and leaves their files; copies and CD imports refuse to go there.
- In a **writable** folder TuneBox may write edited details into songs, delete files you ask it
  to delete, and put CD imports and copies there (*Use for Imports*).
- **The deepest listed folder decides.** A typical setup is a curated collection that is read
  only, with a writable `Imports` folder inside it (or anywhere else) for CD rips:

  | Folder | Rule |
  |---|---|
  | `~/Music/Collection` | read only |
  | `~/Music/Collection/Imports` | writable, imports go here |
  | `~/Music/Vinyl Rips` | read only |

- Songs from folders that are not listed (added with *Add to Library* from anywhere) are
  writable, as they always were.
- *Scan All* (or File ▸ Scan Library Folders) adds songs that appeared in the folders since the
  last scan. Songs already in the library are left alone, with one exception that only reads:
  songs added before TuneBox read the additional tag fields (credits, label, sort names…) have
  those read from their files once. Anything you edited in TuneBox is kept.

From the command line:

```sh
tunebox-cli folders add ~/Music/Collection                       # read only, and add its songs
tunebox-cli folders add ~/Music/Collection/Imports --writable
tunebox-cli folders imports ~/Music/Collection/Imports           # CD imports go here
tunebox-cli folders scan
tunebox-cli folders                                              # list them
```

The rule lives in one place, `crates/engine/src/files.rs`, which every write in the engine goes
through. Symbolic links are resolved, so a link into a read-only folder is read only too. The
tests `a_read_only_library_folder_is_never_changed` and
`a_writable_imports_folder_inside_a_read_only_collection` (`crates/engine/tests/session.rs`)
edit, delete, import and rip, and check read-only folders byte for byte.
