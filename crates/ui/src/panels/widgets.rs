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
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, label, crate::fonts::bold(13.0), Color32::WHITE);
}

/// Room at the start of a menu item for a painted mark (a tick, stars).
const MENU_GUTTER: &str = "      ";

/// A menu item with a gutter on the left, where a tick is painted when `checked`. Every item
/// gets the gutter so labels line up, as in a classic menu.
pub fn menu_item(ui: &mut Ui, label: &str, shortcut: &str, checked: bool, enabled: bool) -> Response {
    let button = egui::Button::new(format!("{MENU_GUTTER}{label}")).shortcut_text(shortcut.to_owned());
    let response = ui.add_enabled(enabled, button);
    if checked {
        let mark = egui::pos2(response.rect.left() + 13.0, response.rect.center().y);
        let color = if enabled { theme::TEXT } else { theme::TEXT_MUTED };
        let stroke = Stroke::new(1.8, color);
        ui.painter().line_segment([mark + vec2(-4.0, 0.0), mark + vec2(-1.2, 3.0)], stroke);
        ui.painter().line_segment([mark + vec2(-1.2, 3.0), mark + vec2(4.5, -3.5)], stroke);
    }
    response
}

/// A menu item showing `stars` of five painted stars (or "None" for none).
pub fn rating_menu_item(ui: &mut Ui, stars: u8) -> Response {
    if stars == 0 {
        return ui.button(format!("{MENU_GUTTER}None"));
    }
    let response = ui.button(format!("{MENU_GUTTER}{MENU_GUTTER}{MENU_GUTTER}"));
    for i in 0..Rating::MAX_STARS {
        let center = egui::pos2(response.rect.left() + 12.0 + f32::from(i) * 13.0, response.rect.center().y);
        let color = if i < stars { theme::TEXT } else { Color32::from_gray(0xC8) };
        paint_star(ui.painter(), center, 5.0, color);
    }
    response
}

/// An on/off switch, the modern stand-in for a checkbox on a settings page. Returns whether it
/// was flipped this frame.
pub fn switch(ui: &mut Ui, on: &mut bool, enabled: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(38.0, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    if response.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool(response.id, *on);
    let painter = ui.painter();
    let off = Color32::from_rgb(0xD3, 0xD6, 0xDB);
    let track = lerp(off, theme::ACCENT, t);
    let track = if enabled { track } else { track.gamma_multiply(0.5) };
    painter.rect_filled(rect, 11.0, track);
    let knob = egui::pos2(rect.left() + 11.0 + t * (rect.width() - 22.0), rect.center().y);
    painter.circle_filled(knob + vec2(0.0, 1.0), 9.0, Color32::from_black_alpha(35));
    painter.circle_filled(knob, 9.0, Color32::WHITE);
    response.clicked()
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// A row of mutually exclusive choices in one rounded control. Returns whether the choice
/// changed this frame.
pub fn segmented(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    options: &[&str],
    selected: &mut usize,
) -> bool {
    let font = egui::FontId::proportional(12.0);
    let widths: Vec<f32> = options
        .iter()
        .map(|o| ui.painter().layout_no_wrap((*o).to_owned(), font.clone(), theme::TEXT).size().x + 22.0)
        .collect();
    let (rect, _) = ui.allocate_exact_size(vec2(widths.iter().sum(), 24.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 7.0, Color32::from_rgb(0xE6, 0xE8, 0xEB));
    let mut changed = false;
    let mut x = rect.left();
    for (i, (option, width)) in options.iter().zip(&widths).enumerate() {
        let segment = Rect::from_min_size(egui::pos2(x, rect.top()), vec2(*width, rect.height()));
        let response = ui.interact(segment, ui.scope_id().with(("segment", &id, i)), Sense::click());
        if i == *selected {
            let chosen = segment.shrink(2.0);
            theme::soft_shadow(ui.painter(), chosen, 5.0, 3);
            ui.painter().rect_filled(chosen, 5.0, Color32::WHITE);
        }
        let color = if i == *selected { theme::TEXT } else { theme::TEXT_MUTED };
        ui.painter().text(segment.center(), egui::Align2::CENTER_CENTER, *option, font.clone(), color);
        if response.clicked() && i != *selected {
            *selected = i;
            changed = true;
        }
        x += width;
    }
    changed
}

/// A pill that is on or off, for choosing several things from a set (like columns).
pub fn chip(ui: &mut Ui, label: &str, on: &mut bool, enabled: bool) -> bool {
    let font = egui::FontId::proportional(12.5);
    let text = ui.painter().layout_no_wrap(label.to_owned(), font.clone(), theme::TEXT);
    let (rect, response) =
        ui.allocate_exact_size(vec2(text.size().x + 34.0, 28.0), if enabled { Sense::click() } else { Sense::hover() });
    if response.clicked() {
        *on = !*on;
    }
    let painter = ui.painter();
    let (fill, border, color) = match (*on, response.hovered()) {
        (true, _) => (theme::ACCENT.gamma_multiply(0.12), theme::ACCENT.gamma_multiply(0.6), theme::ACCENT),
        (false, true) => (Color32::from_rgb(0xF3, 0xF4, 0xF6), Color32::from_gray(0xA8), theme::TEXT),
        (false, false) => (Color32::WHITE, Color32::from_rgb(0xCC, 0xCF, 0xD4), theme::TEXT_MUTED),
    };
    painter.rect_filled(rect, 14.0, fill);
    painter.rect_stroke(rect, 14.0, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    let mark = egui::pos2(rect.left() + 14.0, rect.center().y);
    if *on {
        let stroke = Stroke::new(1.8, color);
        painter.line_segment([mark + vec2(-3.5, 0.0), mark + vec2(-1.0, 2.8)], stroke);
        painter.line_segment([mark + vec2(-1.0, 2.8), mark + vec2(3.8, -2.8)], stroke);
    } else {
        painter.circle_stroke(mark, 3.5, Stroke::new(1.2, color));
    }
    let color = if enabled { color } else { color.gamma_multiply(0.6) };
    painter.text(egui::pos2(rect.left() + 24.0, rect.center().y), egui::Align2::LEFT_CENTER, label, font, color);
    response.clicked()
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

pub fn paint_star(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
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
