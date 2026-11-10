//! The toolbar: transport buttons, volume, the status display, view switcher and search.

use crate::actions::Action;
use crate::glyphs::{self, Glyph};
use crate::panels::{PanelContext, lcd, widgets};
use crate::state::{SourceItem, ViewMode};
use crate::theme;
use egui::{Rect, Sense, Stroke, Ui, pos2, vec2};
use tunebox_engine::PlayState;
use tunebox_engine::commands::CommandId;

pub const HEIGHT: f32 = 66.0;

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let rect = ui.max_rect();
    theme::bar_background(ui.painter(), rect);
    let middle = rect.center().y;

    // Transport.
    let playing = cx.session.now_playing().is_some_and(|n| n.state == PlayState::Playing);
    let previous = Rect::from_center_size(pos2(rect.left() + 28.0, middle), vec2(22.0, 22.0));
    let play = Rect::from_center_size(pos2(rect.left() + 64.0, middle), vec2(30.0, 30.0));
    let next = Rect::from_center_size(pos2(rect.left() + 100.0, middle), vec2(22.0, 22.0));
    let has_queue = cx.session.now_playing().is_some();
    if widgets::glyph_button(ui, previous, Glyph::Previous, has_queue, "Previous").clicked() {
        cx.act(Action::Run(CommandId::PreviousTrack));
    }
    let play_glyph = if playing { Glyph::Pause } else { Glyph::Play };
    if widgets::glyph_button(ui, play, play_glyph, true, if playing { "Pause" } else { "Play" }).clicked() {
        cx.act(Action::Run(CommandId::PlayPause));
    }
    if widgets::glyph_button(ui, next, Glyph::Next, has_queue, "Next").clicked() {
        cx.act(Action::Run(CommandId::NextTrack));
    }

    // Volume.
    let quiet = Rect::from_center_size(pos2(rect.left() + 136.0, middle), vec2(12.0, 12.0));
    glyphs::paint_embossed(ui.painter(), quiet, Glyph::SpeakerQuiet, theme::GLYPH);
    let slider = Rect::from_min_size(pos2(rect.left() + 148.0, middle - 8.0), vec2(86.0, 16.0));
    if let Some(volume) = volume_slider(ui, slider, cx.session.volume()) {
        cx.act(Action::SetVolume(volume));
    }
    let loud = Rect::from_center_size(pos2(rect.left() + 248.0, middle), vec2(14.0, 14.0));
    glyphs::paint_embossed(ui.painter(), loud, Glyph::SpeakerLoud, theme::GLYPH);

    // Search, then the view switcher, from the right.
    let search_rect = Rect::from_min_size(pos2(rect.right() - 196.0, middle - 11.0), vec2(182.0, 22.0));
    search_field(ui, search_rect, cx);
    let list_button = Rect::from_min_size(pos2(search_rect.left() - 76.0, middle - 11.0), vec2(28.0, 22.0));
    let grid_button = list_button.translate(vec2(28.0, 0.0));
    let showing_tracks = !matches!(cx.view.source, SourceItem::Cd | SourceItem::Device(_));
    segment(
        ui,
        list_button,
        Glyph::ListView,
        cx.view.view_mode == ViewMode::List && showing_tracks,
        "View as list",
        || {
            cx.actions.push(Action::Run(CommandId::AsList));
        },
    );
    segment(
        ui,
        grid_button,
        Glyph::GridView,
        cx.view.view_mode == ViewMode::Albums && showing_tracks,
        "View as album grid",
        || {
            cx.actions.push(Action::Run(CommandId::AsAlbums));
        },
    );

    // The status display fills the middle.
    let left_edge = rect.left() + 272.0;
    let right_edge = list_button.left() - 18.0;
    let width = (right_edge - left_edge).clamp(160.0, 520.0);
    let lcd_rect = Rect::from_center_size(pos2((left_edge + right_edge) / 2.0, middle), vec2(width, 50.0));
    lcd::show(ui, lcd_rect, cx);
}

/// Returns a new volume while the knob is dragged or the track clicked.
fn volume_slider(ui: &mut Ui, rect: Rect, volume: f32) -> Option<f32> {
    let response = ui.interact(rect, ui.scope_id().with("volume"), Sense::click_and_drag());
    let painter = ui.painter();
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 10.0, 4.0));
    painter.rect_filled(track.translate(vec2(0.0, 1.0)), 2.0, egui::Color32::from_white_alpha(160));
    painter.rect_filled(track, 2.0, egui::Color32::from_rgb(0x6A, 0x6A, 0x6A));
    let knob = pos2(track.left() + track.width() * volume.clamp(0.0, 1.0), track.center().y);
    painter.circle_filled(knob + vec2(0.0, 1.0), 6.5, egui::Color32::from_black_alpha(50));
    painter.circle(knob, 6.0, egui::Color32::from_rgb(0xF4, 0xF4, 0xF4), Stroke::new(1.0, theme::BORDER));
    if response.dragged() || response.clicked() {
        let x = response.interact_pointer_pos()?.x;
        return Some(((x - track.left()) / track.width()).clamp(0.0, 1.0));
    }
    None
}

fn segment(ui: &mut Ui, rect: Rect, glyph: Glyph, on: bool, tooltip: &str, on_click: impl FnOnce()) {
    let response = ui.interact(rect, ui.scope_id().with(("segment", rect.min.x as i32)), Sense::click());
    let painter = ui.painter();
    let (top, bottom) = if on {
        (egui::Color32::from_rgb(0x8E, 0x8E, 0x8E), egui::Color32::from_rgb(0xB4, 0xB4, 0xB4))
    } else {
        (egui::Color32::from_rgb(0xFA, 0xFA, 0xFA), egui::Color32::from_rgb(0xCF, 0xCF, 0xCF))
    };
    theme::vertical_gradient(painter, rect.shrink(1.0), top, bottom);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0, theme::BORDER_DARK), egui::StrokeKind::Inside);
    let color = if on { egui::Color32::WHITE } else { theme::GLYPH };
    glyphs::paint(painter, glyphs::centered(rect, 12.0), glyph, color);
    if response.on_hover_text(tooltip).clicked() {
        on_click();
    }
}

fn search_field(ui: &mut Ui, rect: Rect, cx: &mut PanelContext) {
    let painter = ui.painter();
    painter.rect_filled(rect, 11.0, egui::Color32::WHITE);
    painter.rect_stroke(rect, 11.0, Stroke::new(1.0, theme::BORDER_DARK), egui::StrokeKind::Inside);
    glyphs::paint(
        painter,
        Rect::from_center_size(pos2(rect.left() + 13.0, rect.center().y), vec2(11.0, 11.0)),
        Glyph::Search,
        theme::TEXT_MUTED,
    );
    let field_rect =
        Rect::from_min_max(pos2(rect.left() + 24.0, rect.top() + 3.0), pos2(rect.right() - 10.0, rect.bottom() - 2.0));
    let hint = match cx.view.source {
        SourceItem::Playlist(_) => "Search Playlist",
        SourceItem::Device(_) => "Search Device",
        _ => "Search Music",
    };
    let edit = egui::TextEdit::singleline(&mut cx.view.search)
        .hint_text(hint)
        .frame(egui::Frame::NONE)
        .desired_width(field_rect.width());
    let response = ui.put(field_rect, edit);
    if std::mem::take(&mut cx.view.focus_search) {
        response.request_focus();
    }
    if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        cx.view.search.clear();
    }
}
