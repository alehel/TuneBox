# Syncing MTP players: investigation and design

Many newer portable players connect only through the Media Transfer Protocol: Android phones
and Android-based players (FiiO, HiBy, Shanling, recent Sony Walkman NW-A/ZX models) and many
older Sony, Creative and SanDisk players in "MTP mode". They never appear as a drive, so
TuneBox's mass-storage sync cannot see them. This document records what it takes to sync them
within the rules of [AGENTS.md](../AGENTS.md) (no `unsafe` in our code, permissive dependencies
only, clean room, layers, never crash), what was decided, and why.

Investigated November 2026. Crate versions and dates are as published on crates.io then.

## The protocol in brief

MTP is an extension of PTP, the Picture Transfer Protocol (ISO 15740, published by PIMA as
PIMA 15740). Both are public specifications:

- *Media Transfer Protocol v1.1*, USB Implementers Forum, 6 April 2011
  (<https://usb.org/node/1677>; a copy of the PDF is at
  <https://www.pjrc.com/tmp/Media_Transfer_Protocol.pdf>).
- *PIMA 15740:2000 / ISO 15740*, Picture Transfer Protocol
  (<https://imaging.org/IST/ist/standards/PTP_Standards.aspx>).
- The USB transport: *USB Still Image Capture Device Definition* (USB-IF, 2000), which MTP 1.1
  Appendix D also describes.

What matters for TuneBox:

- The host opens a **session** and performs **operations**, one at a time. Each is a
  *command* container, an optional *data* phase in either direction, and a *response*
  container with a response code (`0x2001` is OK). Containers are little-endian: a 32-bit
  length, a 16-bit type (1 command, 2 data, 3 response, 4 event), a 16-bit code and a 32-bit
  transaction id, then up to five 32-bit parameters or the data. On USB they travel over a
  pair of bulk endpoints; a container ends with a short packet (a zero-length one when its
  length is a multiple of the packet size).
- The device has one or more **storages** (internal memory, SD card), each with a capacity
  and free space (`GetStorageIDs`, `GetStorageInfo`).
- Files and folders are **objects** addressed by 32-bit **handles**, not by path. Folders are
  objects of format *Association*. Finding `Music/Artist/Album/01 Song.flac` means listing the
  children of each folder (`GetObjectHandles` with a parent) and reading their names
  (`GetObjectInfo`).
- A new file is two operations: `SendObjectInfo` (name, parent, format, **declared size**),
  which returns the new handle, then `SendObject` with the bytes. `GetObject` reads a file,
  `DeleteObject` removes one.
- There is **no rename and no partial write** in the core operation set. MTP's optional object
  properties allow renaming (`SetObjectPropValue` on *Object File Name*), but support varies
  by device.

Only these operations are needed: `GetDeviceInfo`, `OpenSession`, `CloseSession`,
`GetStorageIDs`, `GetStorageInfo`, `GetObjectHandles`, `GetObjectInfo`, `GetObject`,
`DeleteObject`, `SendObjectInfo`, `SendObject`. All are in PTP itself; MTP adds the
vendor-extension id (`6`, Microsoft) and audio format codes (FLAC `0xB906`, OGG `0xB902`,
AAC `0xB903`, M4A/MP4 `0xB982`, WMA `0xB901`; MP3 `0x3009`, WAV `0x3008`, AIFF `0x3007` are
PTP's).

## Platform by platform

### Linux and macOS: raw USB in pure Rust

Neither system has an MTP driver in the kernel. Desktop software (gvfs, KIO, Android File
Transfer, Image Capture) talks to the device from user space over raw USB. TuneBox can do the
same.

| Crate | Version | Licence | State | Verdict |
|---|---|---|---|---|
| [`nusb`](https://crates.io/crates/nusb) | 0.2.9 (24 Oct 2026) | Apache-2.0 OR MIT | Actively maintained (Kevin Mehall; releases monthly; ~1M downloads in 90 days). Pure Rust: usbfs on Linux through `rustix`, IOKit on macOS, WinUSB on Windows. No libusb | **Use** for the USB transport |
| [`mtp-rs`](https://crates.io/crates/mtp-rs) | 0.38.0 (6 Nov 2026) | MIT OR Apache-2.0 | Pure-Rust MTP over `nusb`. Created Feb 2026, 38 minor versions in nine months: the API is still moving. Async (`futures`, `async-trait`) | Not used: we need a blocking API on a job thread, a transport we can replace with a simulator in tests, and control over the "never a truncated song" rules; the protocol subset we need is small |
| [`mtp`](https://crates.io/crates/mtp) / [`mtp_spec`](https://crates.io/crates/mtp_spec) | 0.1.0 (Jun 2026) | MIT OR Apache-2.0 | One release, few downloads | Not used: too young |
| [`mtpkit`](https://crates.io/crates/mtpkit) | 0.1.0 | **LGPL-2.1-or-later** | — | Refused by the licence rules (and not read) |
| libmtp (C) | — | **LGPL** | — | Refused, both as a dependency and as code to read |

`cargo deny check` (our `deny.toml`, targets Linux, macOS and Windows) passes for `nusb`,
`mtp-rs` and `winmtp`: licences, bans, sources and advisories all OK.

The plan is therefore **our own MTP layer, written from the specification, over `nusb`**:
the operations above, encoded and decoded by pure functions, behind a small `Transport` trait
(send one container, receive one container) whose USB implementation is `nusb`'s blocking
`EndpointWrite`/`EndpointRead`. All `unsafe` stays inside `nusb`; ours is
`unsafe_code = "forbid"` as always.

What the listener's system must allow:

- **Linux: permission to open the device.** USB device nodes (`/dev/bus/usb/BBB/DDD`) belong
  to root. systemd's `70-uaccess.rules` gives the logged-in user access to media players and
  to devices with a PTP/MTP-class interface (`ID_MEDIA_PLAYER`, interface class `06/01/01`;
  see [systemd's rules](https://sources.debian.org/src/systemd/259-1/rules.d/70-uaccess.rules.in)),
  and distributions' libmtp packages ship udev rules (data files, not linked code) that mark
  more players. A device that is not covered fails to open with *permission denied*; TuneBox
  must say so and what to do (a udev rule), not crash.
- **macOS:** no permission is needed to open a USB device without a kernel driver. Untested
  here.

### Permission

Where the system does not give the listener access to a player (TuneBox then says "The system
does not let TuneBox open the player"), a udev rule grants it. Find the player's vendor and
product ids with `lsusb` (e.g. `054c:0d00`), then, as root, create
`/etc/udev/rules.d/70-music-player.rules` (the number must sort before `73-seat-late.rules`,
which applies `uaccess`):

```
SUBSYSTEM=="usb", ATTR{idVendor}=="054c", ATTR{idProduct}=="0d00", TAG+="uaccess"
```

and plug the player in again (or run `udevadm control --reload` and `udevadm trigger`).

### Windows: Windows Portable Devices, not raw USB

On Windows the system's MTP class driver (`WpdMtp.inf`, a user-mode driver with `WinUsb.sys`
as its lower filter) owns every MTP device
([Microsoft: the MTP .inf file](https://learn.microsoft.com/en-us/windows-hardware/drivers/portable/the-mtp-setup-information---inf--file),
[WPD drivers overview](https://learn.microsoft.com/windows-hardware/drivers/portable/wpd-drivers-overview)).
`nusb` on Windows opens devices through WinUSB only when WinUSB is the device's *function*
driver, which it is not for MTP devices; replacing the driver (Zadig and the like) would break
Explorer and is not something to ask a listener to do. Microsoft's answer is the **WPD COM
API** (`IPortableDevice*`).

| Crate | Version | Licence | State | Verdict |
|---|---|---|---|---|
| `windows` (Microsoft) | current | MIT OR Apache-2.0 | Maintained | Every COM call is an `unsafe fn`: calling WPD through it directly would put `unsafe` in our code. **Not allowed** |
| [`winmtp`](https://crates.io/crates/winmtp) | 0.3.2 (23 Mar 2026) | MIT | One maintainer; five releases since 2023; ~400 downloads in 90 days. Depends on `windows` 0.52 (two years old) | **Candidate.** Its public API is safe (the `unsafe` is inside it): enumerate devices, walk folders, create folders, stream a file in with a declared size, delete. Passes `cargo deny`. Not adopted now: it cannot be built or tested here, it works in WPD's object model (object ids, not MTP handles) so it needs its own storage implementation, and its maintenance is thin |

### The verdict per platform

| Platform | Route | Rules met? | Built now? |
|---|---|---|---|
| Linux | Our MTP layer over `nusb` | Yes | **Yes** |
| macOS | The same code (nusb's IOKit backend) | Yes | **Yes** (compiles; not run on a Mac) |
| Windows | `winmtp` (WPD) behind the same storage abstraction | Yes, if `winmtp` holds up | **No**: later, once it can be tested on Windows |

## Who else holds the device

Only one program at a time can claim the MTP interface. On a desktop, something usually
claims it first.

- **GNOME (gvfs).** `gvfsd-mtp` claims an MTP device as soon as it is plugged in, so the Files
  app can show it, and keeps it while it is "mounted". Other programs then fail to claim the
  interface ("device busy"); this is a long-standing, well-reported conflict
  ([gvfs-list, 2010](https://lists.gnome.org/archives/gvfs-list/2010-September/msg00000.html),
  [Ubuntu users, 2025](https://lists.snapcraft.io/archives/ubuntu-users/2025-September/314968.html)).
  GNOME also warns that unmounting at the wrong moment can wedge the device until it is
  replugged ([GNOME bug 720238](https://bugzilla.gnome.org/show_bug.cgi?id=720238)).
- **KDE (KIO's `kiod`/MTP worker)** claims the device while Dolphin browses it.
- **macOS:** the photo-import daemon (`ptpcamerad`, behind Image Capture and Photos) claims
  devices with a PTP-class interface; Android File Transfer, if installed, grabs Android
  devices.

**What TuneBox does:** not fight. It never kills another program or detaches a driver. When
claiming fails because the device is busy, the player is still listed, with a plain
explanation and the fix: on Linux, "Eject it in the Files app (or close Dolphin), then try
again"; on macOS, "Quit Image Capture, Photos or Android File Transfer". On Linux TuneBox can
tell that gvfs in particular holds it: gvfs puts a `mtp:host=…` folder in
`$XDG_RUNTIME_DIR/gvfs/`, which TuneBox can see without touching it.

**gvfs's FUSE mount as a fallback.** gvfs exposes MTP devices as folders under
`/run/user/<uid>/gvfs/mtp:host=…`. TuneBox's volume list ignores `/run/user` today, and
`crates/platform/src/mounts.rs` classifies `fuse.gvfsd-fuse` as a *network* file system (only
for deciding whether library songs are missing or merely out of reach; it does not affect
sync). Using that folder as an ordinary mass-storage player would mostly work for reading but
is not a sound base for writing: gvfs maps POSIX writes onto whole-object `SendObject`s, so
the `*.part`-then-rename scheme turns into a full upload plus a device-dependent rename, it is
slow, and an unplugged device mid-write leaves whatever gvfs and the device make of it. It also
only exists on GNOME. **Not used**; the direct route gives TuneBox control over exactly the
guarantees below.

## What MTP changes for sync

**No rename: keeping "never a truncated song".** Today a song is written as `*.part` and
renamed when complete. On MTP the object exists, with its final name and its *declared* size,
from `SendObjectInfo` on, so neither its name nor its reported size proves it is complete. Only
`SendObject` answering OK does. So:

1. Before copying a batch, the manifest on the device records the batch's paths as
   **pending** (and drops the old entries of songs about to be replaced).
2. Each song is sent with its final name. If `SendObject` fails but the device still answers,
   the object is deleted at once. Only on OK is the song recorded as synced.
3. If the player is unplugged mid-song, the manifest still lists that path as pending. The
   next sync deletes every pending path before anything else (TuneBox put them there, so it may)
   and copies the song again. A half-sent song is therefore never recorded as synced and never
   kept.

The same journal works for folders (it is harmless there, as `*.part` already covers it), so
`execute` keeps one code path.

**Storing the manifest.** `.tunebox/device.json` can't be replaced atomically by rename. It is
written as a new object `.tunebox/device-<n>.json` (*n* one higher than the newest), and only
then is the previous one deleted. Opening reads the newest copy that parses. At every moment a
complete manifest exists, and an interrupted write leaves a truncated newest copy that fails to
parse and is skipped (and removed on the next save).

**Free space per storage.** Each storage is offered as its own player (a phone with an SD card
shows two), named after the storage's description, with capacity and free space from
`GetStorageInfo`. The plan's `fits` check is the same. A device that still runs out of space
answers `StoreFull`; that song fails, its object is deleted, and the sync carries on.

**Playlists.** TuneBox writes M3U8 files today. Android's media scanner reads `.m3u`/`.m3u8`
files on the storage as playlists, and Android-based players (FiiO, HiBy, Sony's Android
Walkmans) use the Android scanner or their own that also reads M3U8. Older non-Android MTP
players (Sony NWZ, Creative Zen, SanDisk Sansa in MTP mode) index playlists only as MTP
*Abstract Audio Video Playlist* objects with references (`SetObjectReferences`), and some build
their song database from object properties sent with the file rather than by reading tags.
**First version: M3U8 files**, sent with the M3U playlist format code (`0xBA11`). Playlist
objects and metadata properties are a follow-up once a device that needs them can be tested.

**Object metadata.** Each song is sent with its MTP format code (FLAC, MP3…), so devices that
dispatch on the format see it as audio. Modification dates are left empty (optional in the
dataset). Files of 4 GiB or more declare size `0xFFFFFFFF`, as the specification asks.

**Speed.** MTP is one operation at a time, and finding a path means walking folders. TuneBox
lists each folder once per sync and caches the names and handles, creates folders as needed,
and streams each song in large USB transfers. USB 2.0 MTP devices typically manage 10–30 MB/s;
manifest writes happen once per twenty songs, as today.

**Unplugged mid-sync.** `nusb` reports a disconnected device as an error on the next
transfer; reads and writes also time out (30 seconds) so a device that stops answering cannot
hang the sync. The job ends with "The player was disconnected…" and the journal above makes the
next sync clean up.

## Design

```
tunebox-mtp (L1)     protocol: containers, datasets, codes (pure, tested on hand-built packets)
                     Session<T: Transport>: the operations, blocking
                     Transport trait; UsbTransport over nusb (Linux, macOS); usb::ports()
                     simulator (feature "simulator"): an in-memory MTP responder
tunebox-sync (L2)    Storage trait: put_file, write_file, remove_file, read/write manifest
                     FolderStorage (today's behaviour), MtpStorage (paths → handles, journal)
                     plan (unchanged, pure), execute_on(&mut dyn Storage) (shared)
tunebox-engine (L6)  devices: MTP players next to mass-storage volumes, same job, same notices
tunebox-platform(L7) MtpPlayers: lists USB devices and probes new ones on its own thread
```

- `tunebox-mtp` is L1, like `rip`'s CD access through `cd-da-reader` at L2: OS device access
  that is not UI, needed by the engine's jobs (which cannot depend on `platform`).
- `plan` stays pure and untouched. `execute` takes a `Storage`; the existing
  `execute(device, plan, …)` stays as the folder case, so today's tests and behaviour are
  unchanged.
- A `Volume` gains `access`: `Files` (a mounted folder, as today) or `Mtp(port, storage)`. The
  volume's `root` stays its identity in the source list, as `mtp:<device>/<storage>`.
- Probing a device (opening it, reading storages and manifests) happens off the UI thread: in
  the platform's watcher for the app, inline for `tunebox-cli`.

## Testing without hardware

`tunebox-mtp` ships a **simulated responder** behind the `simulator` feature: a `Transport`
whose other end parses command containers and answers as a device would, with storages,
objects and handles in memory. It can be told to fail: cut the connection after N bytes of an
object (an unplug), refuse writes past a capacity (`StoreFull`), or answer busy. The protocol
layer is also tested on hand-built packets taken byte by byte from the specification's
dataset layouts.

Sync is tested end to end against the simulator: copies, deletes of only TuneBox's files, a
transfer cut half way (no complete-looking song, the manifest right, and the next sync cleaning
up), out of space, unplugged mid-sync. The engine's integration tests drive an MTP player
through `Session` the same way.

What the simulator cannot prove: real devices' quirks (Android's "Allow access to phone data?"
prompt, during which a phone reports no storages; devices that keep a stub after an aborted
transfer; slow media scans), Linux permission rules, and gvfs timing. Those need hardware.

## Recommendation

**Build it for Linux and macOS now; Windows later.** The Linux/macOS route meets every rule:
`nusb` is permissive, maintained and keeps `unsafe` out of our code, and the protocol subset is
small and fully specified in public documents. Windows needs the WPD API; the only safe-API
crate, `winmtp`, is permissively licensed and passes `cargo deny` but cannot be built or tested
here and is thinly maintained, so it waits until it can be tried on Windows. The storage
abstraction makes adding it a matter of one more `Storage` implementation.

## What was built (phase 2)

As recommended, for Linux and macOS:

- **`crates/mtp` (`tunebox-mtp`, L1).** `codes`, `container`, `dataset`: the protocol's bytes,
  pure, tested on hand-built packets and against hostile input (short datasets, arrays longer
  than their data). `Session`: the eleven operations, blocking, one at a time; a connection
  failure marks it broken so later operations fail at once; `send_file` deletes the object
  when a transfer fails and the device still answers, and refuses to send a file that shrinks
  part way. `usb`: `nusb` bulk endpoints with 30-second timeouts and 256 KiB transfers;
  devices found by interface class (06/01/01, or vendor-specific named "MTP"); a busy, denied
  or vanished device is an `io::Error` of the matching kind. `simulator`: the in-memory device.
- **`crates/sync`.** `Storage` with `FolderStorage` (today's behaviour, unchanged) and
  `MtpStorage`; `execute_on` shared by both, with the *pending* journal; `mtp::probe` (each
  writable storage a player, cameras left out, a busy or locked device listed with what to do).
  A folder player also reads a numbered manifest, so a player synced over MTP and later mounted
  as a drive is still recognised.
- **Engine, platform, CLI, UI.** `Volume::access` (`Folder` or `Mtp`); the same sync job, with
  free space read from the device; `Session::refresh_mtp_devices`, fed by
  `tunebox_platform::MtpPlayers` (a thread that opens each new device once, and busy ones
  again every few seconds) in the app and by `find_mtp_players` in `tunebox-cli devices`,
  `setup-device` and `sync` (players named by their `mtp:` id). The device page says where an
  MTP player is ("Internal storage, over USB (MTP)"), saves an MTP player's changed settings
  or name in a background job (opening the player can take seconds; the change shows at
  once, changes made meanwhile are saved when it ends, latest last, and a failed save is
  written by the next sync), shows the problem instead of the set-up
  form when it cannot be used, and Eject just says it can be unplugged.

**Tested** against the simulated device: copies, re-syncs, deletes of only TuneBox's files (the
listener's own file stays), empty folders removed, a transfer cut half way (the partial file
reports its full size, is not recorded, is listed as pending, and the next sync removes it and
copies the song whole), an unplug between songs (every song the manifest lists is complete),
out of space (that song fails, nothing of it is left), a manifest write cut off (the previous
manifest is used, the broken copy tidied away), folders in another case reused, a busy device,
a locked phone, two storages; and through `Session` end to end. **Not tested:** any real
device, macOS, and the USB transport itself (nothing to plug in here).
