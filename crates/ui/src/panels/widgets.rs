//! Small custom controls shared by the panels.

use crate::glyphs::{self, Glyph};
use crate::theme;
use egui::{Color32, Rect, Response, Sense, Stroke, Ui, vec2};
use tunebox_engine::model::Rating;

/// A borderless toolbar button showing an embossed glyph.
pub fn glyph_button(ui: &mut Ui, rect: Rect, glyph: Glyph, enabled: bool, tooltip: &str) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, ui.scope_id().with(("glyph", rect.min.x as i32, rect.min.y as i32)), sense);
    let color = if !enabled {
        theme::GLYPH_DISABLED
    } else if response.is_pointer_button_down_on() {
        theme::GLYPH_PRESSED
    } else {
        theme::GLYPH
    };
    glyphs::paint_embossed(ui.painter(), rect, glyph, color);
    response.on_hover_text(tooltip)
}

/// A small bordered toggle with a glyph, as in the status bar. `on` draws it pressed in.
pub fn toggle_button(ui: &mut Ui, glyph: Glyph, on: bool, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 20.0), Sense::click());
    let painter = ui.painter();
    let (top, bottom) = if on || response.is_pointer_button_down_on() {
        (Color32::from_rgb(0x9C, 0x9C, 0x9C), Color32::from_rgb(0xBE, 0xBE, 0xBE))
    } else {
        (Color32::from_rgb(0xFA, 0xFA, 0xFA), Color32::from_rgb(0xD2, 0xD2, 0xD2))
    };
    theme::vertical_gradient(painter, rect.shrink(1.0), top, bottom);
    theme::outline(painter, rect, 3.0, theme::BORDER);
    let color = if on { theme::ACCENT } else { theme::GLYPH };
    glyphs::paint(painter, glyphs::centered(rect, 13.0), glyph, color);
    response.on_hover_text(tooltip)
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
    painter.rect_filled(box_rect, 2.0, Color32::WHITE);
    painter.rect_stroke(box_rect, 2.0, Stroke::new(1.0, Color32::from_gray(0x8A)), egui::StrokeKind::Inside);
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
