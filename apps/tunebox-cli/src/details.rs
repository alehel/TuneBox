//! Song details from the command line: show every field, edit any of them, read them again
//! from the files, and choose which additional fields the app's Get Info always shows.

use crate::search_ids;
use tunebox_engine::model::{FieldGroup, TagField, TrackId, TrackMeta};
use tunebox_engine::{LibraryOnlyEdits, LibraryView, Preferences, Session, Source};

/// Prints every field that has a value, everyday and additional, for each matching song.
pub fn show(view: LibraryView<'_>, search: &str) -> Result<(), String> {
    let ids = matching(view, search)?;
    for id in ids {
        let Some(track) = view.track(id) else { continue };
        println!("{}", track.location.display());
        let keys = TrackMeta::CORE_KEYS.into_iter().chain(TagField::ALL.map(TagField::key));
        for key in keys {
            let value = track.meta.get(key).unwrap_or_default().replace('\n', " / ");
            if !value.is_empty() {
                println!("  {key:<18} {value}");
            }
        }
        println!();
    }
    Ok(())
}

/// Applies `key=value` assignments to every matching song.
pub fn edit(session: &mut Session, search: &str, assignments: &[String]) -> Result<(), String> {
    let ids = matching(session.view(), search)?;
    let pairs: Vec<(&str, &str)> = assignments
        .iter()
        .map(|a| a.split_once('=').ok_or_else(|| format!("“{a}” is not in the form field=value")))
        .collect::<Result<_, _>>()?;
    // Check every assignment before changing anything, so a typo edits nothing.
    let mut check = TrackMeta::default();
    for (key, value) in &pairs {
        check.set(key, value).map_err(|e| e.to_string())?;
    }
    let mut edited = 0;
    for id in &ids {
        let Some(track) = session.track(*id) else { continue };
        let format = track.file.format;
        let mut meta = track.meta.clone();
        for (key, value) in &pairs {
            if let Some(field) = TagField::from_key(key)
                && !tunebox_engine::can_store(format, field)
            {
                eprintln!("{}: {} files have no place for {key}; left unchanged", track.title(), format.kind_label());
                continue;
            }
            meta.set(key, value).map_err(|e| e.to_string())?;
        }
        session.edit_track(*id, meta).map_err(|e| e.to_string())?;
        edited += 1;
    }
    println!("Edited {edited} songs.");
    Ok(())
}

/// Starts reading the matching songs (all without a search) from their files again.
pub fn refresh(session: &mut Session, search: Option<&str>, replace_library_edits: bool) -> Result<(), String> {
    let ids = match search {
        Some(search) => matching(session.view(), search)?,
        None => search_ids(session.view(), Source::Library, None),
    };
    let edits = if replace_library_edits { LibraryOnlyEdits::Replace } else { LibraryOnlyEdits::Keep };
    session.refresh_from_files(&ids, edits).map_err(|e| e.to_string())?;
    Ok(())
}

/// Pins and unpins additional fields for Get Info, then lists them.
pub fn info_fields(session: &mut Session, pin: &[String], unpin: &[String]) -> Result<(), String> {
    let mut prefs = session.preferences().clone();
    for (keys, pinned) in [(pin, true), (unpin, false)] {
        for key in keys {
            let field = TagField::from_key(key).ok_or_else(|| {
                let known: Vec<&str> = TagField::ALL.iter().map(|f| f.key()).collect();
                format!("there is no field “{key}”; the fields are {}", known.join(", "))
            })?;
            prefs.pin_info_field(field, pinned);
        }
    }
    session.set_preferences(prefs);
    print_info_fields(session.preferences());
    Ok(())
}

/// The additional fields by group, marked when Get Info always shows them.
pub fn print_info_fields(prefs: &Preferences) {
    for group in FieldGroup::ALL {
        let fields: Vec<String> = group
            .fields()
            .map(|f| {
                let mark = if prefs.info_fields.contains(&f) { "[x]" } else { "[ ]" };
                format!("{mark} {}", f.key())
            })
            .collect();
        println!("{:<13} {}", group.title(), fields.join("  "));
    }
}

fn matching(view: LibraryView<'_>, search: &str) -> Result<Vec<TrackId>, String> {
    let ids = search_ids(view, Source::Library, Some(search));
    if ids.is_empty() {
        return Err(format!("no songs match “{search}”"));
    }
    Ok(ids)
}
