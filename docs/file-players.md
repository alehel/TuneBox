# Players that read files off a card

Many players have no sync protocol at all: they play whatever files are on their memory or a
microSD card. Rockbox players work this way, and so do small players such as Snowsky's Echo
Mini and Echo Nano (FiiO's sub-brand). TuneBox syncs them as folders, like any mass-storage
player. This page records what such players need, and the option TuneBox gives for them.
Investigated November 2026.

## What the Snowsky Echo players do

From published specifications, reviews and owners' reports (none of it tested on a device
here):

- **Storage:** 8 GB inside (Echo Mini) plus a microSD card up to 256 GB, FAT32 or exFAT; NTFS
  is not read ([Notebookcheck](https://www.notebookcheck.net/Snowsky-Echo-Nano-leak-hints-at-microSD-support-and-ultra-compact-form-factor-for-new-digital-audio-player.1172817.0.html),
  [inkl hands-on](https://www.inkl.com/news/fiio-snowsky-echo-nano-hands-on-review-this-tiny-50-music-player-makes-unplugging-fun),
  [Head-Fi thread](https://www.head-fi.org/threads/fiio%E2%80%99s-innovative-sub-brand-snowsky-pure-music-player-echo-mini-is-officially-released.975162/page-31)).
- **Formats:** MP3, FLAC, WAV, APE, M4A, WMA, and DSD in some listings; OGG is listed, but an
  owner reports OGG files do not play ([HeadphoneCheck](https://www.headphonecheck.com/test/fiio-snowsky-echo-mini/),
  [HiFi Oasis](https://www.hifioasis.com/?p=11040)).
- **Browsing:** a folder browser, and (since firmware 3.0.0) a "Media Library" read from tags;
  the artist list uses Album Artist. Owners report songs listed in the order their files were
  *created*, not by track number ([Nerds Chalk](https://nerdschalk.com/echo-mini-gets-new-v3-0-0-firmware-update),
  [Head-Fi](https://www.head-fi.org/threads/fiio%E2%80%99s-innovative-sub-brand-snowsky-pure-music-player-echo-mini-is-officially-released.975162/page-35)).
- **Playlists:** disputed. One review says no playlist files are read; an owner says M3U files
  worked ([HiFi Oasis](https://www.hifioasis.com/?p=11040), [Head-Fi](https://www.head-fi.org/threads/fiio%E2%80%99s-innovative-sub-brand-snowsky-pure-music-player-echo-mini-is-officially-released.975162/post-18645070)).
- **Extras:** embedded JPEG cover art; lyrics from an `.lrc` file beside the song
  ([FiiO](https://www.fiio.com/echo)).

## The option: say which player a drive is for

When a player or card is set up for syncing, the listener picks the player that will play it
(**Other player**, **Rockbox**, **Snowsky Echo Mini**, **Snowsky Echo Nano**). TuneBox writes
the choice into the small file it already keeps at the drive's root, `.tunebox/device.json`
(`"player": "SnowskyEchoMini"`). A separate marker file is not needed: that manifest travels
with the card, so the card is recognised and synced the same way in the player or in a card
reader, on any computer running TuneBox. The player can be changed later on the device page.

What the choice changes (`crates/sync/src/players.rs`):

| | Other player, Rockbox | Snowsky Echo Mini, Echo Nano |
|---|---|---|
| Songs copied | All | MP3, FLAC, WAV, APE, M4A, WMA, DSF, DFF. Others are left off, and the sync says how many and why. A song put there earlier for another player, in a format this one cannot play, is removed (TuneBox put it there) |
| Playlists (M3U8) | On by default | Off by default (support is disputed); can be turned on |

For every player, songs are now copied in artist, album and track order, so a player that
lists songs in the order their files were made (the Echo players) shows an album in order on a
fresh sync.

A blank card does not look like a player (no Music or Rockbox folder), so it is not listed by
itself. **File ▸ Set Up Drive for Syncing…** lists the removable drives that are not players
yet; pick one, the player and a name. On the command line:
`tunebox-cli setup-device /media/me/CARD "Echo" --player echo-mini` (`tunebox-cli devices`
lists such drives too).

## Not done yet

- Copying `.lrc` lyrics files beside the songs (the Echo players show them).
- Converting songs the player cannot play to one it can (waits for an encoder; see TODO.md).
- Re-copying an album whose songs were added at different times, so the creation order matches
  track order again.
- Other file players' profiles (FiiO, HiBy, Shanling, older Sony): add one to
  `PlayerModel` with the formats it plays.
- Trying any of this on a real Echo Mini or Echo Nano.
