//! Finding audio files under folders.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tunebox_model::AudioFormat;

/// How deep [`find_audio_files`] descends. Real music folders are a few levels deep; the limit
/// stops runaway walks through unusual mounts.
pub const MAX_DEPTH: usize = 32;

/// Whether `path` names a file TuneBox can import, by its extension.
pub fn is_audio_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).and_then(AudioFormat::from_extension).is_some()
}

/// Every audio file in `roots` (files are taken as they are, folders are walked), sorted by
/// path. Hidden entries are skipped and symbolic links to folders are not followed, so a link
/// loop cannot trap the walk. Unreadable folders are skipped.
pub fn find_audio_files(roots: &[PathBuf], cancel: &AtomicBool) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in roots {
        if root.is_file() {
            if is_audio_file(root) {
                found.push(root.clone());
            }
        } else {
            walk(root, 0, cancel, &mut found);
        }
    }
    found.sort();
    found.dedup();
    found
}

fn walk(dir: &Path, depth: usize, cancel: &AtomicBool, found: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || cancel.load(Ordering::Relaxed) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            walk(&path, depth + 1, cancel, found);
        } else if (kind.is_file() || kind.is_symlink() && path.is_file()) && is_audio_file(&path) {
            found.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_audio_skips_hidden_and_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("x/y")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        for name in ["a.mp3", "x/b.FLAC", "x/y/c.m4a", "x/readme.txt", ".hidden/d.mp3", "x/._e.mp3"] {
            std::fs::write(root.join(name), b"").unwrap();
        }
        let found = find_audio_files(&[root.to_path_buf()], &AtomicBool::new(false));
        let names: Vec<_> = found.iter().map(|p| p.strip_prefix(root).unwrap().to_path_buf()).collect();
        assert_eq!(names, [PathBuf::from("a.mp3"), PathBuf::from("x/b.FLAC"), PathBuf::from("x/y/c.m4a")]);
    }
}
