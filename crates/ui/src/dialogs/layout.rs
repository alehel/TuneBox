//! The building blocks every TuneBox dialog is made of, so they all read the same way: an icon
//! tab bar, cards with a bold heading and one line of explanation, form rows with labels in a
//! fixed column, switches with their explanation beneath, and coloured banners.

use crate::glyphs::{self, Glyph};
use crate::panels::widgets;
use crate::theme;
use egui::{Align, Color32, Frame, Label, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

/// Width of the label column in form rows.
const LABEL_WIDTH: f32 = 120.0;

/// Centred icon tabs, in the manner of a classic preferences toolbar.
pub fn tab_bar<T: Copy + PartialEq>(ui: &mut Ui, current: &mut T, tabs: &[(T, Glyph, &str)]) {
    let size = vec2(92.0, 54.0);
    let total = size.x * tabs.len() as f32;
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::hover());
    let left = row.center().x - total / 2.0;
    for (i, (tab, glyph, label)) in tabs.iter().enumerate() {
        let rect = Rect::from_min_size(pos2(left + size.x * i as f32, row.top()), size);
        let response = ui.interact(rect, ui.scope_id().with(("dialog-tab", i)), Sense::click());
        let selected = *current == *tab;
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect.shrink2(vec2(6.0, 0.0)), 10.0, Color32::from_rgb(0xE4, 0xE9, 0xF2));
        } else if response.hovered() {
            painter.rect_filled(rect.shrink2(vec2(6.0, 0.0)), 10.0, Color32::from_black_alpha(10));
        }
        let color = if selected { theme::ACCENT } else { theme::GLYPH };
        glyphs::paint(painter, Rect::from_center_size(rect.center() - vec2(0.0, 9.0), vec2(22.0, 22.0)), *glyph, color);
        let font = if selected { crate::fonts::bold(12.0) } else { egui::FontId::proportional(12.0) };
        painter.text(rect.center() + vec2(0.0, 15.0), egui::Align2::CENTER_CENTER, *label, font, color);
        if response.clicked() {
            *current = *tab;
        }
    }
    ui.add_space(14.0);
}

/// A rounded white card with a heading and, if given, one line of explanation.
pub fn card(ui: &mut Ui, title: &str, description: &str, contents: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, theme::GRID_LINE))
        .corner_radius(10)
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(RichText::new(title).font(crate::fonts::bold(14.0)));
            }
            if description.is_empty() {
                ui.add_space(4.0);
            } else {
                ui.add(Label::new(muted(description)).wrap());
                ui.add_space(10.0);
            }
            contents(ui);
        });
    ui.add_space(12.0);
}

/// Secondary text.
pub fn muted(text: &str) -> RichText {
    RichText::new(text).size(12.0).color(theme::TEXT_MUTED)
}

/// A setting that is on or off: its name and explanation on the left, a switch on the right.
pub fn switch_row(ui: &mut Ui, title: &str, detail: &str, value: &mut bool) {
    ui.horizontal(|ui| {
        let text_width = ui.available_width() - 58.0;
        ui.allocate_ui_with_layout(vec2(text_width, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(text_width);
            ui.label(RichText::new(title).size(13.0));
            if !detail.is_empty() {
                ui.add(Label::new(muted(detail)).wrap());
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            widgets::switch(ui, value, true);
        });
    });
}

/// A row of a form: the label in a fixed-width column, then the control.
pub fn value_row(ui: &mut Ui, title: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(LABEL_WIDTH, 24.0), Layout::left_to_right(Align::Center), |ui| {
            ui.set_width(LABEL_WIDTH);
            ui.label(RichText::new(title).size(13.0).color(theme::TEXT_MUTED));
        });
        control(ui);
    });
}

/// A read-only fact: a muted label and a value that wraps.
pub fn fact_row(ui: &mut Ui, title: &str, value: &str) {
    value_row(ui, title, |ui| {
        ui.add(Label::new(RichText::new(value).size(13.0)).wrap());
    });
}

/// A labelled text field filling the rest of the row. `hint` shows when it is empty.
pub fn text_field(ui: &mut Ui, title: &str, value: &mut String, hint: &str) {
    value_row(ui, title, |ui| {
        ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(ui.available_width()));
    });
}

/// How a banner reads.
#[derive(Clone, Copy)]
pub enum Tone {
    Info,
    Warning,
}

/// A soft coloured strip with one message, above or inside a card.
pub fn banner(ui: &mut Ui, tone: Tone, glyph: Glyph, text: &str) {
    let (fill, ink) = match tone {
        Tone::Info => (Color32::from_rgb(0xE8, 0xF1, 0xFD), Color32::from_rgb(0x1E, 0x5B, 0xB8)),
        Tone::Warning => (Color32::from_rgb(0xFD, 0xEE, 0xEC), Color32::from_rgb(0xA3, 0x2A, 0x22)),
    };
    Frame::new().fill(fill).corner_radius(8).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (icon, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            glyphs::paint(ui.painter(), icon, glyph, ink);
            ui.add(Label::new(RichText::new(text).size(12.0).color(ink)).wrap());
        });
    });
    ui.add_space(10.0);
}

/// Scrolls `contents` when the window is too short to show it all, keeping a gutter on the right
/// so the scroll bar never covers the cards.
pub fn scrolling_body(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    let max_height = (ui.ctx().content_rect().height() - 240.0).max(220.0);
    egui::ScrollArea::vertical().max_height(max_height).auto_shrink([false, true]).show(ui, |ui| {
        Frame::new().inner_margin(Margin { right: 14, ..Margin::ZERO }).show(ui, contents);
    });
}
