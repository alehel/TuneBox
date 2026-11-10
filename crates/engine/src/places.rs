//! Where a song lives: the place whose presence decides whether a missing file is *missing*
//! (the place is there, the file is not) or *unavailable* (the place itself is gone: a network
//! share not mounted, a drive unplugged). Pure: the caller supplies what it knows about
//! network shares; nothing here touches the disk.

use crate::prefs::LibraryFolder;
use std::path::{Component, Path, PathBuf, Prefix};

/// A drive, share or library folder songs live on.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Place {
    pub root: PathBuf,
    /// On another computer: reachable only while it is mounted, and slow or hanging when it
    /// stops answering.
    pub network: bool,
}

impl Place {
    /// A short name for messages: "NAS", "Music", the share of `\\server\share`.
    pub fn name(&self) -> String {
        match self.root.components().next_back() {
            Some(Component::Normal(name)) => name.to_string_lossy().into_owned(),
            _ => self.root.display().to_string(),
        }
    }
}

/// The place `location` is on. `network_places` are the network shares TuneBox knows of
/// (remembered in the library or mounted now); the deepest one containing the song wins, then
/// a Windows network path, then the library folder it is in, then the drive it is on.
pub fn place_of(location: &Path, folders: &[LibraryFolder], network_places: &[PathBuf]) -> Option<Place> {
    if let Some(root) = deepest(location, network_places.iter()) {
        return Some(Place { root, network: true });
    }
    if let Some(root) = unc_root(location) {
        return Some(Place { root, network: true });
    }
    if let Some(root) = deepest(location, folders.iter().map(|f| &f.path)) {
        return Some(Place { root, network: false });
    }
    volume_root(location).map(|root| Place { root, network: false })
}

fn deepest<'a>(location: &Path, roots: impl Iterator<Item = &'a PathBuf>) -> Option<PathBuf> {
    roots.filter(|root| location.starts_with(root)).max_by_key(|root| root.components().count()).cloned()
}

/// `\\server\share\` for a Windows network path.
fn unc_root(path: &Path) -> Option<PathBuf> {
    match path.components().next()? {
        Component::Prefix(prefix) if matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..)) => {
            let mut root = PathBuf::from(prefix.as_os_str());
            root.push(std::path::MAIN_SEPARATOR_STR);
            Some(root)
        }
        _ => None,
    }
}

/// The removable or mounted drive a path is on, by the systems' conventions: a Windows drive
/// letter, `/Volumes/<name>` on macOS, `/mnt/<name>`, `/media/<user>/<name>` (or
/// `/media/<name>`), `/run/media/<user>/<name>` and GVFS shares on Linux. `None` for the
/// system's own disk.
fn volume_root(path: &Path) -> Option<PathBuf> {
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        let mut root = PathBuf::from(prefix.as_os_str());
        root.push(std::path::MAIN_SEPARATOR_STR);
        return Some(root);
    }
    let names: Vec<&str> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    let depth = match names.as_slice() {
        ["Volumes", _, _, ..] | ["mnt", _, _, ..] => 2,
        // /media/<user>/<label>/… when there is room for both; /media/<label>/file otherwise.
        ["media", _, _, _, ..] => 3,
        ["media", _, _] => 2,
        ["run", "media", _, _, _, ..] => 4,
        ["run", "user", _, "gvfs", _, _, ..] => 5,
        _ => return None,
    };
    let mut root = PathBuf::from("/");
    root.extend(names.iter().take(depth));
    Some(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(path: &str, network: bool) -> Option<Place> {
        Some(Place { root: PathBuf::from(path), network })
    }

    #[test]
    fn a_remembered_network_share_wins_even_over_a_library_folder_inside_it() {
        let folders = [LibraryFolder { path: "/mnt/nas/Music".into(), read_only: true }];
        let network = [PathBuf::from("/mnt/nas")];
        assert_eq!(place_of(Path::new("/mnt/nas/Music/a.flac"), &folders, &network), place("/mnt/nas", true));
        assert_eq!(place_of(Path::new("/mnt/nas/Music/a.flac"), &folders, &[]), place("/mnt/nas/Music", false));
    }

    #[test]
    fn drives_are_found_by_the_systems_conventions() {
        let at = |p: &str| place_of(Path::new(p), &[], &[]);
        assert_eq!(at("/Volumes/NAS/Album/a.flac"), place("/Volumes/NAS", false));
        assert_eq!(at("/mnt/usb/a.flac"), place("/mnt/usb", false));
        assert_eq!(at("/media/alex/Music Drive/Album/a.flac"), place("/media/alex/Music Drive", false));
        assert_eq!(at("/media/USB/a.flac"), place("/media/USB", false));
        assert_eq!(at("/run/media/alex/USB/Album/a.flac"), place("/run/media/alex/USB", false));
        assert_eq!(
            at("/run/user/1000/gvfs/smb-share:server=nas,share=music/a.flac"),
            place("/run/user/1000/gvfs/smb-share:server=nas,share=music", false)
        );
        assert_eq!(at("/home/alex/Music/a.flac"), None, "the system's own disk");
    }

    #[test]
    fn a_place_is_named_by_its_last_part() {
        assert_eq!(Place { root: "/Volumes/NAS".into(), network: true }.name(), "NAS");
    }
}
