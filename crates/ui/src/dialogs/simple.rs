//! The small dialogs: rename, confirm removal, confirm refresh, about.

use crate::actions::Action;
use crate::dialogs::window;
use egui::RichText;
use tunebox_engine::LibraryOnlyEdits;
use tunebox_engine::model::{PlaylistId, TrackId};

#[derive(Clone, Debug)]
pub struct RenamePlaylist {
    pub id: PlaylistId,
    pub name: String,
}

impl RenamePlaylist {
    pub fn show(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let name = &mut self.name;
        let result = window(ctx, "Playlist Name", 340.0, "OK", |ui| {
            let response = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
            response.request_focus();
        });
        let submitted = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        match result {
            Some(true) => self.finish(actions),
            Some(false) => false,
            None if submitted => self.finish(actions),
            None => true,
        }
    }

    fn finish(&self, actions: &mut Vec<Action>) -> bool {
        if self.name.trim().is_empty() {
            return true;
        }
        actions.push(Action::RenamePlaylist(self.id, self.name.trim().to_owned()));
        false
    }
}

#[derive(Clone, Debug)]
pub struct ConfirmRemove {
    pub tracks: Vec<TrackId>,
    pub delete_files: bool,
}

impl ConfirmRemove {
    pub fn show(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let count = self.tracks.len();
        let delete_files = &mut self.delete_files;
        let result = window(ctx, "Delete from Library", 420.0, "Delete", |ui| {
            let what = if count == 1 { "this song".to_owned() } else { format!("these {count} songs") };
            ui.label(RichText::new(format!("Are you sure you want to delete {what} from your library?")).strong());
            ui.label("They are also removed from every playlist.");
            ui.add_space(6.0);
            ui.checkbox(delete_files, "Also delete the files from disk");
        });
        match result {
            Some(true) => {
                actions.push(Action::RemoveTracks { tracks: self.tracks.clone(), delete_files: self.delete_files });
                false
            }
            Some(false) => false,
            None => true,
        }
    }
}

/// Offered when a song whose file is missing is played: say where it is now.
#[derive(Clone, Debug)]
pub struct ConfirmLocate {
    pub track: TrackId,
    pub title: String,
}

impl ConfirmLocate {
    pub fn show(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let title = &self.title;
        let result = window(ctx, "Song File Missing", 420.0, "Locate…", |ui| {
            ui.label(RichText::new(format!("The file of “{title}” cannot be found.")).strong());
            ui.label("It may have been moved, renamed or deleted. If you know where it is now, locate it: the song keeps its ratings, plays and playlists. TuneBox also looks for other missing songs that moved the same way.");
        });
        match result {
            Some(true) => {
                actions.push(Action::Locate(self.track));
                false
            }
            Some(false) => false,
            None => true,
        }
    }
}

/// Asked before refreshing songs whose edits are kept in TuneBox only, which the refresh would
/// replace with what their files say.
#[derive(Clone, Debug)]
pub struct ConfirmRefresh {
    pub tracks: Vec<TrackId>,
    /// How many of `tracks` have edits kept in TuneBox only.
    pub with_edits: usize,
    pub replace_edits: bool,
}

impl ConfirmRefresh {
    pub fn show(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let with_edits = self.with_edits;
        let replace = &mut self.replace_edits;
        let result = window(ctx, "Refresh from Files", 440.0, "Refresh", |ui| {
            let what = if with_edits == 1 {
                "1 of these songs has changes you made in TuneBox".to_owned()
            } else {
                format!("{with_edits} of these songs have changes you made in TuneBox")
            };
            ui.label(RichText::new(format!("{what} that are not in the files.")).strong());
            ui.label("They are in a read-only library folder, or their files could not be written. Unless you choose otherwise, they are left as they are and the other songs are refreshed.");
            ui.add_space(6.0);
            ui.checkbox(replace, "Replace my changes with what the files say");
        });
        match result {
            Some(true) => {
                let edits = if self.replace_edits { LibraryOnlyEdits::Replace } else { LibraryOnlyEdits::Keep };
                actions.push(Action::RefreshFromFiles(self.tracks.clone(), edits));
                false
            }
            Some(false) => false,
            None => true,
        }
    }
}

pub fn about(ctx: &egui::Context) -> bool {
    let mut open = true;
    super::modal(ctx, "about", |ui| {
        ui.set_width(380.0);
        ui.vertical_centered(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("TuneBox").font(crate::fonts::bold(24.0)));
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.add_space(8.0);
                ui.label("Your music library, CD importing and player syncing,\nin a fast native app written in Rust.");
                ui.add_space(8.0);
                ui.label(RichText::new("Decoding by symphonia through craft-audio · tags by lofty\nCD access by cd-da-reader · sound by cpal · interface by egui").size(11.0).weak());
                ui.label(RichText::new("MIT OR Apache-2.0").size(11.0).weak());
                ui.add_space(10.0);
                if ui.add(egui::Button::new("Close").min_size(egui::vec2(88.0, 28.0))).clicked() {
                    open = false;
                }
                ui.add_space(4.0);
            });
    });
    open && !ctx.input(|i| i.key_pressed(egui::Key::Escape))
}
