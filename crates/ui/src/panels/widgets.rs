//! Small custom controls shared by the panels.

use crate::glyphs::{self, Glyph};
use crate::theme;
use egui::{Color32, Rect, Response, Sense, Stroke, Ui, vec2};
use tunebox_engine::model::Rating;

/// A small rounded toggle with a glyph, as in the status bar. `on` shows it lit: darker, with
/// the glyph in the accent colour.
pub fn toggle_button(ui: &mut Ui, glyph: Glyph, on: bool, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(32.0, 22.0), Sense::click());
    let painter = ui.painter();
    let (top, bottom) = if on || response.is_pointer_button_down_on() {
        (Color32::from_rgb(0xD3, 0xD6, 0xDB), Color32::from_rgb(0xE6, 0xE8, 0xEB))
    } else {
        (Color32::WHITE, Color32::from_rgb(0xE6, 0xE8, 0xEB))
    };
    crate::theme::soft_shadow(painter, rect, 6.0, 2);
    crate::theme::rounded_gradient(painter, rect, 6.0, top, bottom);
    let border = if response.hovered() { Color32::from_gray(0x8C) } else { Color32::from_rgb(0xB4, 0xB8, 0xBE) };
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    let color = if on { theme::ACCENT } else { theme::GLYPH };
    glyphs::paint(painter, glyphs::centered(rect, 13.0), glyph, color);
    response.on_hover_text(tooltip)
}

/// The main action of a page or dialog: a blue, glossy button with white text.
pub fn primary_button(ui: &mut Ui, label: &str, enabled: bool, size: egui::Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    paint_primary(ui, rect, label, enabled, &response);
    response
}

/// Paints a primary button into `rect` (for buttons placed with `ui.interact`).
pub fn paint_primary(ui: &Ui, rect: Rect, label: &str, enabled: bool, response: &Response) {
    let painter = ui.painter();
    let radius = 7.0;
    let (top, bottom) = match (enabled, response.is_pointer_button_down_on()) {
        (false, _) => (Color32::from_rgb(0xA9, 0xC4, 0xEC), Color32::from_rgb(0x93, 0xB4, 0xE6)),
        (true, true) => (Color32::from_rgb(0x2A, 0x66, 0xC4), Color32::from_rgb(0x3A, 0x7A, 0xD8)),
        (true, false) if response.hovered() => {
            (Color32::from_rgb(0x6D, 0xA8, 0xF6), Color32::from_rgb(0x34, 0x77, 0xDE))
        }
        (true, false) => (theme::SELECTION_TOP, theme::SELECTION_BOTTOM),
    };
    theme::soft_shadow(painter, rect, radius, 4);
    theme::rounded_gradient(painter, rect, radius, top, bottom);
    painter.rect_stroke(rect, radius, Stroke::new(1.0, Color32::from_rgb(0x24, 0x5E, 0xB8)), egui::StrokeKind::Inside);
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(13.0), Color32::WHITE);
}

/// Five stars; clicking a star sets the rating, clicking the current one clears it.
/// Returns the new rating when clicked.
pub fn rating(ui: &mut Ui, rect: Rect, rating: Rating, interactive: bool, on_selected_row: bool) -> Option<Rating> {
    let star = 11.0;
    let painter = ui.painter_at(rect);
    let response =
        interactive.then(|| ui.interact(rect, ui.scope_id().with(("stars", rect.min.y as i32)), Sense::click()));
    let hovered_stars =
        response.as_ref().and_then(|r| r.hover_pos()).map(|p| (((p.x - rect.left()) / star).floor() as u8 + 1).min(5));
    let shown = hovered_stars.unwrap_or(rating.stars());
    let filled = if on_selected_row { Color32::WHITE } else { theme::TEXT };
    let empty = if on_selected_row { Color32::from_white_alpha(110) } else { Color32::from_gray(0xC4) };
    for i in 0..Rating::MAX_STARS {
        let center = egui::pos2(rect.left() + star * (f32::from(i) + 0.5), rect.center().y);
        if i < shown {
            paint_star(&painter, center, 5.0, filled);
        } else if hovered_stars.is_some() || rating.is_rated() {
            painter.circle_filled(center, 1.5, empty);
        }
    }
    if !response.as_ref().is_some_and(Response::clicked) {
        return None;
    }
    let stars = hovered_stars.unwrap_or(0);
    Some(if stars == rating.stars() { Rating::UNRATED } else { Rating::from_stars(stars) })
}

fn paint_star(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    let points: Vec<egui::Pos2> = (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { radius } else { radius * 0.45 };
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            center + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    // A star is not convex: draw it as five triangles around the centre plus the inner pentagon.
    // `points` has ten entries, so every index below is in range.
    for i in 0..5 {
        let tip = points[i * 2];
        let left = points[(i * 2 + 9) % 10];
        let right = points[i * 2 + 1];
        painter.add(egui::Shape::convex_polygon(vec![left, tip, right], color, Stroke::NONE));
    }
    let inner: Vec<egui::Pos2> = (0..5).map(|i| points[i * 2 + 1]).collect();
    painter.add(egui::Shape::convex_polygon(inner, color, Stroke::NONE));
}

/// A square checkbox for table rows. Returns the new value when clicked.
pub fn row_checkbox(ui: &mut Ui, rect: Rect, checked: bool) -> Option<bool> {
    let box_rect = Rect::from_center_size(rect.center(), vec2(11.0, 11.0));
    let response = ui.interact(box_rect, ui.scope_id().with(("check", rect.min.y as i32)), Sense::click());
    let painter = ui.painter();
    let border = if response.hovered() { Color32::from_gray(0x70) } else { Color32::from_gray(0xA8) };
    crate::theme::rounded_gradient(painter, box_rect, 3.0, Color32::WHITE, Color32::from_gray(0xEE));
    painter.rect_stroke(box_rect, 3.0, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    if checked {
        let stroke = Stroke::new(1.8, theme::TEXT);
        let a = box_rect.left_top() + vec2(2.5, 5.5);
        let b = box_rect.left_top() + vec2(4.8, 8.2);
        let c = box_rect.left_top() + vec2(8.8, 2.5);
        painter.line_segment([a, b], stroke);
        painter.line_segment([b, c], stroke);
    }
    response.clicked().then_some(!checked)
}
