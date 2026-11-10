//! The small dialogs: rename, confirm removal, about.

use crate::actions::Action;
use crate::dialogs::window;
use egui::RichText;
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
