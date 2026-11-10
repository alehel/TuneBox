//! Dialogs. One is open at a time; each edits a draft and turns it into actions on OK.

mod extra_fields;
mod get_info;
mod layout;
mod preferences;
mod simple;
mod smart_playlist;

pub use get_info::GetInfoDialog;
pub use preferences::{PreferencesDialog, Tab as PreferencesTab};
pub use simple::{ConfirmLocate, ConfirmRefresh, ConfirmRemove, RenamePlaylist};
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
    ConfirmRefresh(ConfirmRefresh),
    ConfirmLocate(ConfirmLocate),
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
            Dialog::ConfirmRefresh(dialog) => dialog.show(ctx, actions),
            Dialog::ConfirmLocate(dialog) => dialog.show(ctx, actions),
            Dialog::About => simple::about(ctx),
        }
    }
}

/// Shows `contents` as a modal dialog: centred, over a dimmed window that ignores clicks.
pub(crate) fn modal(ctx: &egui::Context, id: &str, contents: impl FnOnce(&mut egui::Ui)) {
    let frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(16));
    egui::Modal::new(egui::Id::unique(("tunebox-dialog", id)))
        .frame(frame)
        .backdrop_color(egui::Color32::from_black_alpha(70))
        .show(ctx, contents);
}

/// A dialog's title: centred, with a hairline below. (egui's own title bar is hidden: it does
/// not span a fixed-width window.)
pub(crate) fn title_heading(ui: &mut egui::Ui, title: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(2.0);
        ui.label(egui::RichText::new(title).font(crate::fonts::bold(15.0)).color(crate::theme::TEXT));
    });
    ui.add_space(4.0);
    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(rect.x_range(), rect.top(), egui::Stroke::new(1.0, crate::theme::GRID_LINE));
    ui.add_space(10.0);
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
    modal(ctx, title, |ui| {
        ui.set_width(width);
        title_heading(ui, title);
        contents(ui);
        ui.add_space(12.0);
        // A row of bounded height: a right-to-left layout would otherwise claim all the
        // height it is offered.
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 28.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if crate::panels::widgets::primary_button(ui, ok_label, true, egui::vec2(88.0, 28.0)).clicked() {
                    result = Some(true);
                }
                if ui.add(egui::Button::new("Cancel").min_size(egui::vec2(88.0, 28.0))).clicked() {
                    result = Some(false);
                }
            },
        );
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        result = Some(false);
    }
    result
}
