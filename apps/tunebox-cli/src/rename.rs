//! `tunebox-cli rename`: Rename Files from Tags. Without `--apply` it only shows what would
//! happen, reading the library as last saved (so it works while the app is open); with
//! `--apply` it renames, through the session.

use crate::search_ids;
use std::collections::HashMap;
use std::path::Path;
use tunebox_engine::model::TrackId;
use tunebox_engine::{
    FileStatus, LibraryView, Planned, RenameBase, RenameConflicts, RenamePlan, RenameRequest, Session, Source,
};

/// Where `--to` files the songs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum RenameTo {
    /// Inside the library folder each song is in.
    Folder,
    /// Inside the media folder.
    Media,
}

fn request(view: LibraryView<'_>, search: &str, pattern: &str, to: RenameTo) -> Result<RenameRequest, String> {
    let tracks = search_ids(view, Source::Music, Some(search));
    if tracks.is_empty() {
        return Err(format!("no songs match “{search}”"));
    }
    let base = match to {
        RenameTo::Folder => RenameBase::OwnFolder,
        RenameTo::Media => RenameBase::MediaFolder,
    };
    Ok(RenameRequest { tracks, pattern: pattern.to_owned(), base })
}

/// The preview, on a snapshot: nothing is changed.
pub fn preview(view: LibraryView<'_>, search: &str, pattern: &str, to: RenameTo) -> Result<(), String> {
    let request = request(view, search, pattern, to)?;
    let unplayable: HashMap<TrackId, FileStatus> =
        view.find_unplayable(&tunebox_platform::network_mounts()).into_iter().collect();
    let plan = view
        .preview_rename(&request, |id| unplayable.get(&id).cloned().unwrap_or(FileStatus::Present))
        .map_err(|e| e.to_string())?;
    print_plan(view, &plan);
    if plan.renamed() > 0 {
        let skip = if plan.conflicts() > 0 { " --skip-conflicts" } else { "" };
        println!("Nothing was changed. Run again with --apply{skip} to rename.");
    }
    Ok(())
}

/// Renames, after showing the same preview.
pub fn apply(
    session: &mut Session,
    search: &str,
    pattern: &str,
    to: RenameTo,
    skip_conflicts: bool,
) -> Result<(), String> {
    // A fresh look, waited for: only files that are there now are renamed.
    session.set_network_mounts(tunebox_platform::network_mounts());
    session.check_files_now();
    let request = request(session.view(), search, pattern, to)?;
    let plan = session.preview_rename(&request).map_err(|e| e.to_string())?;
    print_plan(session.view(), &plan);
    let conflicts = if skip_conflicts { RenameConflicts::Skip } else { RenameConflicts::Refuse };
    let hint = if plan.conflicts() > 0 && !skip_conflicts { " (or add --skip-conflicts)" } else { "" };
    session.rename_files(&plan, conflicts).map_err(|e| format!("{e}{hint}"))?;
    crate::wait_for_jobs(session);
    Ok(())
}

fn print_plan(view: LibraryView<'_>, plan: &RenamePlan) {
    let title = |id: TrackId| view.track(id).map(|t| t.title().to_owned()).unwrap_or_default();
    for item in &plan.items {
        let short = |path: &Path| shorten(path, item.base.as_deref());
        match &item.planned {
            Planned::Rename { to } => println!("  {}\n    → {}", short(&item.from), short(to)),
            Planned::Unchanged => {}
            Planned::Skipped(reason) => println!("  skipped:  {} ({})", title(item.id), reason.text()),
            Planned::Conflict { to, reason } => {
                println!("  conflict: {} → {} ({})", title(item.id), short(to), reason.text())
            }
        }
    }
    println!("{}", plan.summary());
}

/// `path` inside `base`, or all of it when it is elsewhere.
fn shorten(path: &Path, base: Option<&Path>) -> String {
    base.and_then(|base| path.strip_prefix(base).ok()).unwrap_or(path).display().to_string()
}
