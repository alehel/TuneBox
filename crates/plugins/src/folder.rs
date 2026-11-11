//! Finding plug-ins: every `.wasm` file directly in the `Visualizers` folder.

use crate::error::LoadError;
use crate::plugin::Plugin;
use crate::sandbox::MAX_FILE_BYTES;
use std::io::Read;
use std::path::Path;

/// Most plug-in files looked at in one scan.
pub const MAX_PLUGINS: usize = 64;

/// What a scan of the folder found.
#[derive(Debug, Default)]
pub struct FolderReport {
    /// The plug-ins that loaded, in file-name order.
    pub plugins: Vec<Plugin>,
    /// `(file name, why)` for each file that did not.
    pub problems: Vec<(String, LoadError)>,
}

/// Loads every `.wasm` file directly inside `folder`, in file-name order. A folder that does not
/// exist holds no plug-ins (it is not an error). A plug-in whose id another already uses is
/// left out. Only reads: TuneBox never writes in this folder except to create it.
pub fn load_folder(folder: &Path) -> FolderReport {
    let mut report = FolderReport::default();
    let Ok(entries) = std::fs::read_dir(folder) else { return report };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wasm")) && path.is_file())
        .collect();
    files.sort();
    for path in files.into_iter().take(MAX_PLUGINS) {
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let loaded = read_bounded(&path).and_then(|bytes| Plugin::load(&bytes, name.clone()));
        match loaded {
            Ok(plugin) => match report.plugins.iter().find(|p| p.info().id == plugin.info().id) {
                Some(first) => report.problems.push((name, LoadError::DuplicateId(first.file().to_owned()))),
                None => report.plugins.push(plugin),
            },
            Err(e) => report.problems.push((name, e)),
        }
    }
    report
}

/// The file's bytes, refusing one larger than plug-ins may be without reading all of it.
fn read_bounded(path: &Path) -> Result<Vec<u8>, LoadError> {
    let file = std::fs::File::open(path).map_err(|e| LoadError::Unreadable(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).map_err(|e| LoadError::Unreadable(e.to_string()))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(LoadError::TooBig(bytes.len() as u64));
    }
    Ok(bytes)
}
