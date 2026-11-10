//! Which mounted file systems are network shares (NFS, SMB, AFP, WebDAV, SSHFS…). TuneBox
//! remembers network places in the library, so a share that is not mounted reads as "not
//! connected" rather than as songs gone missing (an unmounted share often leaves its mount
//! point behind as an empty folder, which would otherwise look like a folder of deleted songs).
//!
//! Finding mounts must never hang on a share that stopped answering. On Linux the kernel's
//! mount table is read (`/proc/self/mounts`), which touches no share. Elsewhere the system's
//! volume list is used. Either way it runs on a thread of its own ([`NetworkMounts`]), so the
//! interface never waits for it.

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
        .collect();
    mounts.sort();
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
    fn a_broken_line_is_skipped_not_fatal() {
        assert!(network_mounts_in("garbage\n\n /x").is_empty());
        assert_eq!(unescape("a\\040b\\0"), "a b\\0");
    }
}
