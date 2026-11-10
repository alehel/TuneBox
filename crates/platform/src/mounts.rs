//! Which mounted file systems are network shares (NFS, SMB, AFP, WebDAV, SSHFS…). TuneBox
//! remembers network places in the library, so a share that is not mounted reads as "not
//! connected" rather than as songs gone missing (an unmounted share often leaves its mount
//! point behind as an empty folder, which would otherwise look like a folder of deleted songs).
//!
//! Finding mounts must never hang on a share that stopped answering. On Linux the kernel's
//! mount table is read (`/proc/self/mounts`), which touches no share. On Windows, mapped network
//! drive letters (`Z:`) come from `net use`, which lists the user's mappings without visiting
//! them (the system's volume list leaves network drives out). Elsewhere the volume list is used.
//! Either way it runs on a thread of its own ([`NetworkMounts`]), so the interface never waits.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// How often the mounts are looked at again.
const EVERY: Duration = Duration::from_secs(5);

/// File system types that live on another computer.
const NETWORK_TYPES: [&str; 15] = [
    "nfs",
    "nfs4",
    "cifs",
    "smb",
    "smb2",
    "smb3",
    "smbfs",
    "afpfs",
    "afp",
    "webdav",
    "davfs",
    "fuse.sshfs",
    "sshfs",
    "9p",
    "ncpfs",
];

fn is_network_type(fs_type: &str) -> bool {
    let fs_type = fs_type.to_ascii_lowercase();
    NETWORK_TYPES.contains(&fs_type.as_str()) || fs_type.starts_with("fuse.rclone") || fs_type == "fuse.gvfsd-fuse"
}

/// The mount points of network shares in a Linux mount table (`/proc/self/mounts` format:
/// device, mount point, type, options…, with spaces in paths written as `\040`).
pub fn network_mounts_in(table: &str) -> Vec<PathBuf> {
    let mut mounts: Vec<PathBuf> = table
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _device = fields.next()?;
            let mount_point = fields.next()?;
            let fs_type = fields.next()?;
            is_network_type(fs_type).then(|| PathBuf::from(unescape(mount_point)))
        })
        .collect();
    mounts.sort();
    mounts.dedup();
    mounts
}

/// Undoes the mount table's octal escapes (`\040` space, `\011` tab, `\012` newline, `\134` backslash).
fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let digits: String = chars.clone().take(3).collect();
            if digits.len() == 3
                && let Ok(code) = u8::from_str_radix(&digits, 8)
            {
                out.push(char::from(code));
                for _ in 0..3 {
                    chars.next();
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// The drive letters mapped to network shares in the output of Windows' `net use`, as `Z:\`.
/// The output's headings and status words are translated into the system's language, so only
/// what never changes is relied on: a drive letter followed by a `\\server\share` path. A
/// mapping whose server is away is listed too (as unavailable), and still counts: it is a
/// network drive, and the engine then finds it not connected.
pub fn mapped_drives_in(net_use: &str) -> Vec<PathBuf> {
    let mut drives: Vec<PathBuf> = net_use
        .lines()
        .filter_map(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            let letter = tokens.iter().position(|t| is_drive_letter(t))?;
            let remote = tokens.get(letter + 1)?;
            let drive = tokens.get(letter)?.to_ascii_uppercase();
            remote.starts_with("\\\\").then(|| PathBuf::from(format!("{drive}\\")))
        })
        .collect();
    drives.sort();
    drives.dedup();
    drives
}

fn is_drive_letter(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// The mapped network drives, from `net use`. Never a console window: TuneBox may be a window
/// app, which would otherwise flash one every few seconds.
#[cfg(windows)]
fn mapped_drives() -> Vec<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    match std::process::Command::new("net").arg("use").creation_flags(CREATE_NO_WINDOW).output() {
        Ok(output) => mapped_drives_in(&String::from_utf8_lossy(&output.stdout)),
        Err(e) => {
            log::warn!("could not list mapped network drives: {e}");
            Vec::new()
        }
    }
}

#[cfg(not(windows))]
fn mapped_drives() -> Vec<PathBuf> {
    Vec::new()
}

/// The network shares mounted now. Reading may take a moment on some systems; call it from a
/// thread that can wait, or use [`NetworkMounts`].
pub fn network_mounts() -> Vec<PathBuf> {
    if cfg!(target_os = "linux") {
        return std::fs::read_to_string("/proc/self/mounts").map(|t| network_mounts_in(&t)).unwrap_or_default();
    }
    let disks = match sysinfo::Disks::new_with_refreshed_list() {
        Ok(disks) => disks,
        Err(e) => {
            log::warn!("could not list mounted volumes: {e}");
            return Vec::new();
        }
    };
    let mut mounts: Vec<PathBuf> = disks
        .iter()
        .filter(|disk| is_network_type(&disk.file_system().to_string_lossy()))
        .map(|disk| disk.mount_point().to_path_buf())
        .chain(mapped_drives())
        .collect();
    mounts.sort();
    mounts.dedup();
    mounts
}

/// Keeps an up-to-date list of network shares, looked up on a thread of its own.
pub struct NetworkMounts {
    latest: Arc<Mutex<Option<Vec<PathBuf>>>>,
    /// What [`NetworkMounts::changed`] last handed out.
    reported: Option<Vec<PathBuf>>,
}

impl NetworkMounts {
    pub fn start() -> Self {
        let latest = Arc::new(Mutex::new(None));
        let shared = Arc::clone(&latest);
        let spawned = std::thread::Builder::new().name("tunebox-mounts".into()).spawn(move || {
            // Ends when the last handle is dropped.
            while Arc::strong_count(&shared) > 1 {
                let mounts = network_mounts();
                *shared.lock().unwrap_or_else(PoisonError::into_inner) = Some(mounts);
                std::thread::sleep(EVERY);
            }
        });
        if let Err(e) = spawned {
            log::warn!("cannot look for network shares: {e}");
        }
        NetworkMounts { latest, reported: None }
    }

    /// The network shares, when they differ from what was last handed out.
    pub fn changed(&mut self) -> Option<Vec<PathBuf>> {
        let latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner).clone()?;
        if self.reported.as_ref() == Some(&latest) {
            return None;
        }
        self.reported = Some(latest.clone());
        Some(latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_network_shares_in_a_mount_table() {
        let table = "\
/dev/nvme0n1p2 / ext4 rw,relatime 0 0
proc /proc proc rw,nosuid 0 0
nas:/volume1/music /mnt/nas nfs4 rw,relatime,vers=4.1 0 0
//nas/Music\\040Library /media/alex/Music\\040Library cifs rw 0 0
alex@box:/srv /home/alex/box fuse.sshfs rw 0 0
/dev/sdb1 /media/alex/USB vfat rw 0 0
";
        assert_eq!(
            network_mounts_in(table),
            [PathBuf::from("/home/alex/box"), PathBuf::from("/media/alex/Music Library"), PathBuf::from("/mnt/nas")]
        );
    }

    #[test]
    fn finds_mapped_drive_letters_in_net_use_output_whatever_the_language() {
        let english = r"New connections will be remembered.

Status       Local     Remote                    Network

-------------------------------------------------------------------------------
OK           Z:        \\nas\music               Microsoft Windows Network
Unavailable  y:        \\server\My Music          Microsoft Windows Network
             X:        \\tsclient\C              Microsoft Terminal Services
OK                     \\nas\IPC$                Microsoft Windows Network
The command completed successfully.
";
        let german = r"Status       Lokal     Remote                    Netzwerk
-------------------------------------------------------------------------------
Nicht verfügbar  W:    \\nas\archiv              Microsoft Windows Network
Der Befehl wurde erfolgreich ausgeführt.
";
        let drive = |d: &str| PathBuf::from(format!("{d}:\\"));
        assert_eq!(mapped_drives_in(english), [drive("X"), drive("Y"), drive("Z")]);
        assert_eq!(mapped_drives_in(german), [drive("W")]);
        assert!(mapped_drives_in("Es sind keine Einträge in der Liste.").is_empty());
        assert!(mapped_drives_in("C:  D:  not a mapping").is_empty());
    }

    #[test]
    fn a_broken_line_is_skipped_not_fatal() {
        assert!(network_mounts_in("garbage\n\n /x").is_empty());
        assert_eq!(unescape("a\\040b\\0"), "a b\\0");
    }
}
