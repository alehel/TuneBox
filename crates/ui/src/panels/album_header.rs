//! What the CD page and the audiobook page share: a header laid out like an album page, with the
//! art (a cover, or a disc), names as text that becomes a field when hovered or focused, a
//! line of details, and a row of buttons along the art's bottom edge.

use crate::panels::PanelContext;
use crate::theme;
use egui::{Align2, Color32, FontId, Rect, Stroke, Ui, pos2, vec2};

/// Space around the art and between it and the text.
pub const PAD: f32 = 20.0;
/// The disc or cover; the text column and the buttons fill its height.
pub const ART: f32 = 140.0;
/// Height of the buttons.
pub const BUTTON: f32 = 26.0;
/// The genre and year: smaller than the names, larger than the details.
pub const META_SIZE: f32 = 13.5;
/// The text column stops growing here, so a long title stays near the art on a wide window.
pub const MAX_TEXT_WIDTH: f32 = 640.0;

/// Draws the header's background across the top of `area`; returns the header and the art's
/// place in it.
pub fn background(ui: &Ui, area: Rect) -> (Rect, Rect) {
    let header = Rect::from_min_size(area.min, vec2(area.width(), ART + 2.0 * PAD));
    theme::vertical_gradient(
        ui.painter(),
        header,
        Color32::from_rgb(0xFA, 0xFA, 0xFB),
        Color32::from_rgb(0xEC, 0xEE, 0xF1),
    );
    ui.painter().hline(header.x_range(), header.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));
    let art = Rect::from_min_size(pos2(header.left() + PAD, header.top() + PAD), vec2(ART, ART));
    (header, art)
}

/// The text column beside the art: its left edge and right edge.
pub fn text_column(header: Rect, art: Rect) -> (f32, f32) {
    let left = art.right() + PAD;
    let right = (header.right() - PAD).min(left + MAX_TEXT_WIDTH).max(left);
    (left, right)
}

/// The line of details, ending in an ellipsis when the window is narrow.
pub fn details_line(ui: &Ui, header: Rect, at: egui::Pos2, text: &str) {
    theme::text_in_row(ui.painter(), at, text, 11.5, theme::TEXT_MUTED, header.right() - PAD - at.x);
}

/// A field with no frame until it is hovered or focused, for names shown as text.
pub fn quiet_field(ui: &mut Ui, rect: Rect, value: &mut String, hint: &str, font: FontId, color: Color32) -> bool {
    let edit = egui::TextEdit::singleline(value)
        .hint_text(hint)
        .font(font)
        .text_color(color)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .align(Align2::LEFT_CENTER);
    let response = ui.put(rect, edit);
    if response.hovered() || response.has_focus() {
        let stroke = if response.has_focus() { theme::ACCENT } else { theme::BORDER };
        // Drawn a little outside the field, so the text stays in line with the rest of the
        // column instead of shifting right to make room for a frame.
        let frame = rect.expand2(vec2(5.0, 0.0));
        ui.painter().rect_stroke(frame, 4.0, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    }
    response.changed()
}

/// The width of a quiet field showing `text`, or its hint when empty.
pub fn fit(ui: &Ui, text: &str, hint: &str, font: &FontId) -> f32 {
    let shown = if text.is_empty() { hint } else { text };
    // The extra room is for the text cursor at the end.
    ui.painter().layout_no_wrap(shown.to_owned(), font.clone(), Color32::BLACK).size().x + 2.0
}

/// The dot between the details of a line.
pub fn dot(ui: &Ui, at: egui::Pos2) {
    ui.painter().text(at, Align2::CENTER_CENTER, "·", FontId::proportional(META_SIZE), theme::TEXT_MUTED);
}

/// A plain button filling `rect`. `put` centres its label; egui otherwise aligns a button's
/// label with the surrounding layout.
pub fn secondary_button(ui: &mut Ui, rect: Rect, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| ui.put(rect, egui::Button::new(label))).inner
}

/// The cover (`key` names its texture), or a disc when there is none.
pub fn paint_art(ui: &mut Ui, cx: &mut PanelContext, art: Rect, cover: Option<(&str, &[u8])>) {
    match cover {
        Some((key, data)) => {
            let texture = cx.artwork.picture(ui.ctx(), key, data);
            theme::soft_shadow(ui.painter(), art, 6.0, 10);
            crate::artwork::paint(ui, art, texture.as_ref());
            ui.painter().rect_stroke(
                art,
                6.0,
                Stroke::new(1.0, Color32::from_black_alpha(25)),
                egui::StrokeKind::Inside,
            );
        }
        None => paint_disc(ui.painter(), art),
    }
}

/// A compact disc: a silver platter with a faint rainbow sheen, the clear hub and the hole.
pub fn paint_disc(painter: &egui::Painter, rect: Rect) {
    let center = rect.center();
    let radius = rect.width() / 2.0;
    theme::soft_shadow(painter, rect, radius, 10);
    theme::rounded_gradient(
        painter,
        rect,
        radius,
        Color32::from_rgb(0xF2, 0xF4, 0xF7),
        Color32::from_rgb(0xBF, 0xC5, 0xCE),
    );
    // The sheen: thin coloured arcs, as light breaks on the grooves.
    for (i, color) in [
        (0, Color32::from_rgba_unmultiplied(120, 180, 255, 70)),
        (1, Color32::from_rgba_unmultiplied(255, 150, 210, 60)),
        (2, Color32::from_rgba_unmultiplied(150, 255, 200, 60)),
    ] {
        let r = radius * (0.55 + 0.13 * i as f32);
        let points: Vec<egui::Pos2> = (0..=16)
            .map(|k| {
                let a = -2.4 + 0.07 * k as f32 + 0.2 * i as f32;
                center + vec2(a.cos(), a.sin()) * r
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(radius * 0.09, color)));
    }
    painter.circle_stroke(center, radius - 0.5, Stroke::new(1.0, Color32::from_rgb(0xA4, 0xAB, 0xB5)));
    painter.circle_filled(center, radius * 0.3, Color32::from_rgba_unmultiplied(235, 238, 242, 230));
    painter.circle_stroke(center, radius * 0.3, Stroke::new(1.0, Color32::from_rgb(0xB8, 0xBE, 0xC6)));
    painter.circle_filled(center, radius * 0.1, Color32::from_rgb(0xFA, 0xFA, 0xFB));
    painter.circle_stroke(center, radius * 0.1, Stroke::new(1.0, Color32::from_rgb(0xA4, 0xAB, 0xB5)));
}
