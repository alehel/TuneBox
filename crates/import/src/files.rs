//! Copying files into the organised media folder without ever overwriting one.

use std::io;
use std::path::{Path, PathBuf};
use tunebox_model::{AudioFormat, TrackMeta, organize};

/// Copies `source` to its organised place in `media_folder` and returns the new path. If a file
/// of the same size is already there it is taken to be this file, imported before, and reused.
pub fn copy_into_media_folder(
    source: &Path,
    meta: &TrackMeta,
    format: AudioFormat,
    media_folder: &Path,
) -> io::Result<PathBuf> {
    let fallback = source.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled");
    let extension = source.extension().and_then(|e| e.to_str()).map(str::to_lowercase);
    let extension = extension.as_deref().unwrap_or(format.extension());
    let wanted = media_folder.join(organize::relative_path(meta, fallback, extension));
    let source_len = std::fs::metadata(source)?.len();
    if std::fs::metadata(&wanted).is_ok_and(|m| m.len() == source_len) {
        return Ok(wanted);
    }
    let target = unique_path(&wanted);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Copy beside the target and rename when complete, so a failed copy leaves no torn file.
    let mut part_name = target.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    part_name.push(".part");
    let part = target.with_file_name(part_name);
    if let Err(e) = std::fs::copy(source, &part).and_then(|_| std::fs::rename(&part, &target)) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    Ok(target)
}

/// `path` if nothing is there, else the first free "name 1.ext", "name 2.ext", …
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (1..u32::MAX)
        .map(|n| path.with_file_name(format!("{stem} {n}{extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_path_counts_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.mp3");
        assert_eq!(unique_path(&path), path);
        std::fs::write(&path, b"1").unwrap();
        std::fs::write(dir.path().join("song 1.mp3"), b"2").unwrap();
        assert_eq!(unique_path(&path), dir.path().join("song 2.mp3"));
    }

    #[test]
    fn same_sized_file_is_reused_different_one_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("in.mp3");
        std::fs::write(&source, b"abc").unwrap();
        let media = dir.path().join("media");
        let meta = TrackMeta { title: "T".into(), artist: "A".into(), album: "B".into(), ..TrackMeta::default() };
        let first = copy_into_media_folder(&source, &meta, AudioFormat::Mp3, &media).unwrap();
        assert_eq!(first, media.join("A/B/T.mp3"));
        assert_eq!(copy_into_media_folder(&source, &meta, AudioFormat::Mp3, &media).unwrap(), first);
        std::fs::write(&source, b"different length").unwrap();
        assert_eq!(
            copy_into_media_folder(&source, &meta, AudioFormat::Mp3, &media).unwrap(),
            media.join("A/B/T 1.mp3")
        );
    }
}
