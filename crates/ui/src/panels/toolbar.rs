//! The toolbar: transport capsule, volume, the status display, view switcher and search.

use crate::actions::Action;
use crate::glyphs::{self, Glyph};
use crate::panels::{PanelContext, lcd};
use crate::state::{SourceItem, ViewMode};
use crate::theme;
use egui::{Color32, CornerRadius, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use tunebox_engine::PlayState;
use tunebox_engine::commands::CommandId;

pub const HEIGHT: f32 = 74.0;

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let rect = ui.max_rect();
    theme::bar_background(ui.painter(), rect);
    let middle = rect.center().y;

    let transport_right = transport(ui, cx, pos2(rect.left() + 18.0, middle));
    volume(ui, cx, pos2(transport_right + 22.0, middle));

    // From the right: search, then the view switcher.
    let search = Rect::from_min_size(pos2(rect.right() - 210.0, middle - 13.0), vec2(194.0, 26.0));
    search_field(ui, search, cx);
    let switcher = Rect::from_min_size(pos2(search.left() - 126.0, middle - 13.0), vec2(108.0, 26.0));
    view_switcher(ui, switcher, cx);

    // The status display takes the middle.
    let left_edge = transport_right + 160.0;
    let right_edge = switcher.left() - 22.0;
    let width = (right_edge - left_edge).clamp(180.0, 580.0);
    let display = Rect::from_center_size(pos2((left_edge + right_edge) / 2.0, middle), vec2(width, 54.0));
    lcd::show(ui, display, cx);
}

/// Previous and Next in a capsule, with a round Play button standing proud in its middle.
/// Returns the capsule's right edge.
fn transport(ui: &mut Ui, cx: &mut PanelContext, left_center: egui::Pos2) -> f32 {
    let capsule = Rect::from_min_size(left_center - vec2(0.0, 15.0), vec2(124.0, 30.0));
    let has_queue = cx.session.now_playing().is_some();
    let playing = cx.session.now_playing().is_some_and(|n| n.state == PlayState::Playing);

    let previous = Rect::from_min_size(capsule.min, vec2(40.0, capsule.height()));
    let next = Rect::from_min_max(pos2(capsule.right() - 40.0, capsule.top()), capsule.max);
    let previous_response = ui.interact(previous, ui.scope_id().with("previous"), sense(has_queue));
    let next_response = ui.interact(next, ui.scope_id().with("next"), sense(has_queue));
    theme::capsule(ui.painter(), capsule, false);
    pressed_half(ui, previous, &previous_response, true);
    pressed_half(ui, next, &next_response, false);
    glyphs::paint(
        ui.painter(),
        glyphs::centered(previous, 14.0),
        Glyph::Previous,
        glyph_color(has_queue, &previous_response),
    );
    glyphs::paint(ui.painter(), glyphs::centered(next, 14.0), Glyph::Next, glyph_color(has_queue, &next_response));
    if previous_response.on_hover_text("Previous").clicked() {
        cx.act(Action::Run(CommandId::PreviousTrack));
    }
    if next_response.on_hover_text("Next").clicked() {
        cx.act(Action::Run(CommandId::NextTrack));
    }

    let play = Rect::from_center_size(capsule.center(), vec2(42.0, 42.0));
    let response = ui.interact(play, ui.scope_id().with("play"), Sense::click());
    let painter = ui.painter();
    theme::soft_shadow(painter, play, 21.0, 8);
    let (top, bottom) = if response.is_pointer_button_down_on() {
        (Color32::from_rgb(0xD8, 0xDA, 0xDE), Color32::from_rgb(0xF4, 0xF5, 0xF6))
    } else {
        (Color32::WHITE, Color32::from_rgb(0xDE, 0xE1, 0xE5))
    };
    theme::rounded_gradient(painter, play, 21.0, top, bottom);
    painter.circle_stroke(play.center(), 20.5, Stroke::new(1.0, Color32::from_rgb(0xAC, 0xB0, 0xB6)));
    let glyph = if playing { Glyph::Pause } else { Glyph::Play };
    let nudge = if playing { vec2(0.0, 0.0) } else { vec2(1.5, 0.0) };
    glyphs::paint(painter, glyphs::centered(play.translate(nudge), 17.0), glyph, glyph_color(true, &response));
    if response.on_hover_text(if playing { "Pause" } else { "Play" }).clicked() {
        cx.act(Action::Run(CommandId::PlayPause));
    }
    capsule.right()
}

fn sense(enabled: bool) -> Sense {
    if enabled { Sense::click() } else { Sense::hover() }
}

fn glyph_color(enabled: bool, response: &Response) -> Color32 {
    if !enabled {
        theme::GLYPH_DISABLED
    } else if response.is_pointer_button_down_on() || response.hovered() {
        theme::GLYPH_PRESSED
    } else {
        theme::GLYPH
    }
}

/// Darkens the pressed end of the transport capsule.
fn pressed_half(ui: &Ui, rect: Rect, response: &Response, left: bool) {
    if response.is_pointer_button_down_on() {
        let radius = rect.height() / 2.0;
        let corners = if left {
            CornerRadius { nw: radius as u8, sw: radius as u8, ne: 0, se: 0 }
        } else {
            CornerRadius { nw: 0, sw: 0, ne: radius as u8, se: radius as u8 }
        };
        ui.painter().rect_filled(rect.shrink(1.0), corners, Color32::from_black_alpha(22));
    }
}

/// Speaker, slider, speaker. The slider fills from the left as the volume rises.
fn volume(ui: &mut Ui, cx: &mut PanelContext, left_center: egui::Pos2) {
    let quiet = Rect::from_center_size(left_center + vec2(6.0, 0.0), vec2(12.0, 12.0));
    glyphs::paint(ui.painter(), quiet, Glyph::SpeakerQuiet, theme::GLYPH);
    let slider = Rect::from_min_size(left_center + vec2(18.0, -9.0), vec2(96.0, 18.0));
    if let Some(volume) = volume_slider(ui, slider, cx.session.volume()) {
        cx.act(Action::SetVolume(volume));
    }
    let loud = Rect::from_center_size(left_center + vec2(128.0, 0.0), vec2(15.0, 15.0));
    glyphs::paint(ui.painter(), loud, Glyph::SpeakerLoud, theme::GLYPH);
}

/// Returns a new volume while the knob is dragged or the track clicked.
fn volume_slider(ui: &mut Ui, rect: Rect, volume: f32) -> Option<f32> {
    let response = ui.interact(rect, ui.scope_id().with("volume"), Sense::click_and_drag());
    let painter = ui.painter();
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 12.0, 5.0));
    let level = track.left() + track.width() * volume.clamp(0.0, 1.0);
    painter.rect_filled(track.translate(vec2(0.0, 1.0)), 2.5, Color32::from_white_alpha(200));
    painter.rect_filled(track, 2.5, Color32::from_rgb(0xBC, 0xC0, 0xC6));
    let filled = Rect::from_min_max(track.min, pos2(level, track.bottom()));
    theme::rounded_gradient(
        painter,
        filled,
        2.5,
        Color32::from_rgb(0x6A, 0x70, 0x78),
        Color32::from_rgb(0x44, 0x49, 0x50),
    );
    let knob = Rect::from_center_size(pos2(level, track.center().y), vec2(15.0, 15.0));
    theme::soft_shadow(painter, knob, 7.5, 4);
    theme::rounded_gradient(painter, knob, 7.5, Color32::WHITE, Color32::from_rgb(0xE4, 0xE6, 0xE9));
    painter.circle_stroke(knob.center(), 7.0, Stroke::new(1.0, Color32::from_rgb(0xA8, 0xAC, 0xB2)));
    if response.dragged() || response.clicked() {
        let x = response.interact_pointer_pos()?.x;
        return Some(((x - track.left()) / track.width()).clamp(0.0, 1.0));
    }
    None
}

/// List, album grid and visualizer as one segmented control.
fn view_switcher(ui: &mut Ui, rect: Rect, cx: &mut PanelContext) {
    let showing_tracks = !matches!(cx.view.source, SourceItem::Cd | SourceItem::Device(_));
    let visualizer = cx.view.show_visualizer;
    let segments = [
        (
            Glyph::ListView,
            "View as list",
            CommandId::AsList,
            !visualizer && showing_tracks && cx.view.view_mode == ViewMode::List,
        ),
        (
            Glyph::GridView,
            "View as album grid",
            CommandId::AsAlbums,
            !visualizer && showing_tracks && cx.view.view_mode == ViewMode::Albums,
        ),
        (Glyph::Visualizer, "Visualizer", CommandId::Visualizer, visualizer),
    ];
    theme::capsule(ui.painter(), rect, false);
    let width = rect.width() / segments.len() as f32;
    let radius = rect.height() / 2.0;
    for (i, (glyph, tooltip, command, on)) in segments.into_iter().enumerate() {
        let segment = Rect::from_min_size(pos2(rect.left() + width * i as f32, rect.top()), vec2(width, rect.height()));
        let response = ui.interact(segment, ui.scope_id().with(("view", i)), Sense::click());
        if on || response.is_pointer_button_down_on() {
            let r = radius as u8;
            let corners = match i {
                0 => CornerRadius { nw: r, sw: r, ne: 0, se: 0 },
                2 => CornerRadius { nw: 0, sw: 0, ne: r, se: r },
                _ => CornerRadius::ZERO,
            };
            let fill = if on { Color32::from_rgb(0x5A, 0x60, 0x68) } else { Color32::from_black_alpha(25) };
            ui.painter().rect_filled(segment.shrink(1.0), corners, fill);
        }
        if i > 0 {
            ui.painter().vline(
                segment.left(),
                segment.shrink2(vec2(0.0, 4.0)).y_range(),
                Stroke::new(1.0, Color32::from_rgb(0xC4, 0xC7, 0xCC)),
            );
        }
        let color = if on { Color32::WHITE } else { theme::GLYPH };
        glyphs::paint(ui.painter(), glyphs::centered(segment, 13.0), glyph, color);
        if response.on_hover_text(tooltip).clicked() {
            cx.act(Action::Run(command));
        }
    }
}

fn search_field(ui: &mut Ui, rect: Rect, cx: &mut PanelContext) {
    let radius = rect.height() / 2.0;
    let field_rect =
        Rect::from_min_max(pos2(rect.left() + 28.0, rect.top() + 4.0), pos2(rect.right() - 26.0, rect.bottom() - 3.0));
    let hint = match cx.view.source {
        SourceItem::Playlist(_) => "Search Playlist",
        SourceItem::Device(_) => "Search Device",
        _ => "Search Music",
    };
    // The field's own response decides the focus ring, so draw the frame first, then the text.
    let painter = ui.painter().clone();
    painter.rect_filled(rect, radius, Color32::WHITE);
    let edit = egui::TextEdit::singleline(&mut cx.view.search)
        .hint_text(hint)
        .frame(egui::Frame::NONE)
        .desired_width(field_rect.width());
    let response = ui.put(field_rect, edit);
    let ring = if response.has_focus() {
        Stroke::new(2.0, theme::ACCENT.gamma_multiply(0.7))
    } else {
        Stroke::new(1.0, Color32::from_rgb(0xB0, 0xB4, 0xBA))
    };
    painter.rect_stroke(rect, radius, ring, egui::StrokeKind::Inside);
    glyphs::paint(
        &painter,
        Rect::from_center_size(pos2(rect.left() + 15.0, rect.center().y), vec2(12.0, 12.0)),
        Glyph::Search,
        theme::TEXT_MUTED,
    );
    if std::mem::take(&mut cx.view.focus_search) {
        response.request_focus();
    }
    if !cx.view.search.is_empty() {
        let clear = Rect::from_center_size(pos2(rect.right() - 13.0, rect.center().y), vec2(14.0, 14.0));
        let clear_response = ui.interact(clear, ui.scope_id().with("clear-search"), Sense::click());
        painter.circle_filled(
            clear.center(),
            7.0,
            if clear_response.hovered() { theme::GLYPH } else { Color32::from_rgb(0xA8, 0xAC, 0xB2) },
        );
        let x = Stroke::new(1.5, Color32::WHITE);
        painter.line_segment([clear.center() + vec2(-2.8, -2.8), clear.center() + vec2(2.8, 2.8)], x);
        painter.line_segment([clear.center() + vec2(2.8, -2.8), clear.center() + vec2(-2.8, 2.8)], x);
        if clear_response.on_hover_text("Clear").clicked() {
            cx.view.search.clear();
        }
    }
    if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        cx.view.search.clear();
    }
}
