//! The bar along the bottom: new playlist, shuffle, repeat, artwork, and the selection summary.

use crate::actions::Action;
use crate::glyphs::Glyph;
use crate::panels::{PanelContext, widgets};
use crate::state::SourceItem;
use crate::theme;
use egui::{Align2, FontId, Ui, pos2};
use tunebox_engine::RepeatMode;
use tunebox_engine::commands::CommandId;
use tunebox_engine::model::{format_clock, format_total_time};

pub const HEIGHT: f32 = 30.0;

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let rect = ui.max_rect();
    theme::bar_background(ui.painter(), rect);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(10.0, 5.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if widgets::toggle_button(ui, Glyph::Plus, false, "New Playlist").clicked() {
                cx.act(Action::Run(CommandId::NewPlaylist));
            }
            if widgets::toggle_button(ui, Glyph::Shuffle, cx.session.shuffle(), "Shuffle").clicked() {
                cx.act(Action::Run(CommandId::Shuffle));
            }
            let repeat = cx.session.repeat();
            let glyph = if repeat == RepeatMode::One { Glyph::RepeatOne } else { Glyph::Repeat };
            if widgets::toggle_button(ui, glyph, repeat != RepeatMode::Off, "Repeat").clicked() {
                cx.act(Action::Run(CommandId::Repeat));
            }
            if widgets::toggle_button(ui, Glyph::Artwork, cx.view.show_artwork, "Show or hide artwork").clicked() {
                cx.view.show_artwork = !cx.view.show_artwork;
            }
            if widgets::toggle_button(ui, Glyph::Visualizer, cx.view.show_visualizer, "Visualizer").clicked() {
                cx.act(Action::Run(CommandId::Visualizer));
            }
        },
    );
    ui.painter().text(
        pos2(rect.center().x + 60.0, rect.center().y),
        Align2::CENTER_CENTER,
        summary(cx),
        FontId::proportional(11.5),
        theme::TEXT,
    );
}

fn summary(cx: &PanelContext) -> String {
    match &cx.view.source {
        SourceItem::Cd => cx
            .session
            .cd()
            .map(|cd| format!("{} songs, {}", cd.audio_numbers().len(), format_total_time(cd.toc.total_duration())))
            .unwrap_or_default(),
        _ => {
            let selected = cx.view.selected_tracks();
            if selected.len() > 1 {
                let summary = cx.session.summary(&selected);
                format!(
                    "{} of {} songs selected, {}",
                    selected.len(),
                    cx.view.summary().count,
                    format_clock(summary.duration)
                )
            } else {
                cx.view.summary().label()
            }
        }
    }
}
