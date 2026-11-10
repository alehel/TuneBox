//! Dialogs. One is open at a time; each edits a draft and turns it into actions on OK.

mod get_info;
mod preferences;
mod simple;
mod smart_playlist;

pub use get_info::GetInfoDialog;
pub use preferences::PreferencesDialog;
pub use simple::{ConfirmRemove, RenamePlaylist};
pub use smart_playlist::SmartPlaylistDialog;

use crate::actions::Action;
use crate::artwork::ArtworkCache;
use tunebox_engine::Session;
use tunebox_engine::model::PlaylistId;

#[derive(Clone, Debug)]
pub enum Dialog {
    GetInfo(Box<GetInfoDialog>),
    Preferences(Box<PreferencesDialog>),
    SmartPlaylist(Box<SmartPlaylistDialog>),
    RenamePlaylist(RenamePlaylist),
    ConfirmRemove(ConfirmRemove),
    About,
}

impl Dialog {
    pub fn rename_playlist(id: PlaylistId, name: &str) -> Dialog {
        Dialog::RenamePlaylist(RenamePlaylist { id, name: name.to_owned() })
    }

    /// Draws the dialog. Returns `false` once it is closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        artwork: &mut ArtworkCache,
        actions: &mut Vec<Action>,
    ) -> bool {
        match self {
            Dialog::GetInfo(dialog) => dialog.show(ctx, session, artwork, actions),
            Dialog::Preferences(dialog) => dialog.show(ctx, actions),
            Dialog::SmartPlaylist(dialog) => dialog.show(ctx, actions),
            Dialog::RenamePlaylist(dialog) => dialog.show(ctx, actions),
            Dialog::ConfirmRemove(dialog) => dialog.show(ctx, actions),
            Dialog::About => simple::about(ctx),
        }
    }
}

/// A centred, fixed window with OK/Cancel along the bottom. `contents` draws the body;
/// returns `Some(true)` for OK, `Some(false)` for Cancel or Escape.
pub(crate) fn window(
    ctx: &egui::Context,
    title: &str,
    width: f32,
    ok_label: &str,
    contents: impl FnOnce(&mut egui::Ui),
) -> Option<bool> {
    let mut result = None;
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .fixed_size(egui::vec2(width, 0.0))
        .show(ctx, |ui| {
            ui.set_max_width(width);
            contents(ui);
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(ok_label).min_size(egui::vec2(80.0, 24.0))).clicked() {
                    result = Some(true);
                }
                if ui.add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 24.0))).clicked() {
                    result = Some(false);
                }
            });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        result = Some(false);
    }
    result
}
