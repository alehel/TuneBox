//! TuneBox's look: a light, brushed-grey window with a blue-grey source list and a crisp white
//! track list, in the spirit of the classic desktop jukebox. Every colour lives here.

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Stroke, TextStyle, Visuals};

pub const WINDOW_BG: Color32 = Color32::from_rgb(0xE4, 0xE4, 0xE4);
pub const TEXT: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x6E, 0x6E, 0x6E);
pub const BORDER: Color32 = Color32::from_rgb(0x8E, 0x8E, 0x8E);
pub const BORDER_DARK: Color32 = Color32::from_rgb(0x5C, 0x5C, 0x5C);

/// Toolbar and status bar: a vertical brushed-metal gradient.
pub const BAR_TOP: Color32 = Color32::from_rgb(0xEC, 0xEC, 0xEC);
pub const BAR_BOTTOM: Color32 = Color32::from_rgb(0xB9, 0xB9, 0xB9);
pub const BAR_HIGHLIGHT: Color32 = Color32::from_rgb(0xF8, 0xF8, 0xF8);

pub const SIDEBAR_BG: Color32 = Color32::from_rgb(0xDB, 0xE1, 0xE8);
pub const SIDEBAR_HEADING: Color32 = Color32::from_rgb(0x6A, 0x73, 0x7E);
pub const SIDEBAR_ICON: Color32 = Color32::from_rgb(0x5B, 0x63, 0x6E);
pub const SELECTION_TOP: Color32 = Color32::from_rgb(0x6C, 0x9C, 0xDB);
pub const SELECTION_BOTTOM: Color32 = Color32::from_rgb(0x3C, 0x70, 0xC0);

pub const LIST_BG: Color32 = Color32::WHITE;
pub const ROW_ALT: Color32 = Color32::from_rgb(0xEE, 0xF3, 0xFA);
pub const ROW_SELECTED: Color32 = Color32::from_rgb(0x3D, 0x7F, 0xDB);
pub const HEADER_TOP: Color32 = Color32::from_rgb(0xFB, 0xFB, 0xFB);
pub const HEADER_BOTTOM: Color32 = Color32::from_rgb(0xDE, 0xDE, 0xDE);
pub const HEADER_SORTED_TOP: Color32 = Color32::from_rgb(0xD6, 0xE4, 0xF6);
pub const HEADER_SORTED_BOTTOM: Color32 = Color32::from_rgb(0xA9, 0xC4, 0xE8);

/// The status display in the middle of the toolbar.
pub const LCD_TOP: Color32 = Color32::from_rgb(0xF8, 0xF9, 0xF3);
pub const LCD_BOTTOM: Color32 = Color32::from_rgb(0xE1, 0xE4, 0xD8);
pub const LCD_TEXT: Color32 = Color32::from_rgb(0x26, 0x28, 0x22);
pub const LCD_TRACK: Color32 = Color32::from_rgb(0xA4, 0xA7, 0x9C);
pub const LCD_FILL: Color32 = Color32::from_rgb(0x4B, 0x4D, 0x47);

pub const GLYPH: Color32 = Color32::from_rgb(0x3A, 0x3A, 0x3A);
pub const GLYPH_PRESSED: Color32 = Color32::from_rgb(0x15, 0x15, 0x15);
pub const GLYPH_DISABLED: Color32 = Color32::from_rgb(0xA0, 0xA0, 0xA0);
pub const ACCENT: Color32 = Color32::from_rgb(0x2F, 0x6F, 0xD0);

/// Capacity bar segments on the device page.
pub const CAPACITY_AUDIO: Color32 = Color32::from_rgb(0x4C, 0x8F, 0xE0);
pub const CAPACITY_OTHER: Color32 = Color32::from_rgb(0xE9, 0xA9, 0x3B);
pub const CAPACITY_FREE: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF2);

pub const ROW_HEIGHT: f32 = 19.0;
pub const SMALL: f32 = 11.0;
pub const BODY: f32 = 12.5;

/// Installs TuneBox's visuals and text sizes.
pub fn install(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    let mut visuals = Visuals::light();
    visuals.panel_fill = WINDOW_BG;
    visuals.window_fill = Color32::from_rgb(0xEE, 0xEE, 0xEE);
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.selection.bg_fill = ROW_SELECTED;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.hyperlink_color = ACCENT;
    visuals.override_text_color = Some(TEXT);
    style_widgets(&mut visuals.widgets);
    ctx.set_visuals_of(egui::Theme::Light, visuals);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::new(SMALL, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(BODY, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(BODY, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(17.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(BODY, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
    });
}

/// Buttons, fields, checkboxes and radio buttons with visible outlines, as on a classic desktop:
/// egui's light theme draws them borderless, which leaves a checkbox's box invisible.
fn style_widgets(widgets: &mut egui::style::Widgets) {
    let radius = CornerRadius::same(4);
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_gray(0xC4));
    for (state, fill, border) in [
        (&mut widgets.inactive, Color32::from_gray(0xF7), Color32::from_gray(0x9A)),
        (&mut widgets.hovered, Color32::WHITE, Color32::from_gray(0x70)),
        (&mut widgets.active, Color32::from_gray(0xD6), Color32::from_gray(0x5A)),
        (&mut widgets.open, Color32::from_gray(0xE8), Color32::from_gray(0x70)),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, border);
        state.corner_radius = radius;
        state.fg_stroke = Stroke::new(1.0, TEXT);
        state.expansion = 0.0;
    }
}

/// Fills `rect` with a vertical gradient from `top` to `bottom`.
pub fn vertical_gradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    let corners =
        [(rect.left_top(), top), (rect.right_top(), top), (rect.right_bottom(), bottom), (rect.left_bottom(), bottom)];
    for (pos, color) in corners {
        mesh.vertices.push(Vertex { pos, uv: WHITE_UV, color });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// A toolbar-style bar: gradient, a highlight line on top and a dark line below.
pub fn bar_background(painter: &Painter, rect: Rect) {
    vertical_gradient(painter, rect, BAR_TOP, BAR_BOTTOM);
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, BAR_HIGHLIGHT));
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, BORDER_DARK));
}

/// The blue selection gradient of the source list.
pub fn selection_background(painter: &Painter, rect: Rect) {
    vertical_gradient(painter, rect, SELECTION_TOP, SELECTION_BOTTOM);
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, Color32::from_rgb(0x5A, 0x8A, 0xCC)));
}

/// A rounded rectangle outline, for fields and the LCD.
pub fn outline(painter: &Painter, rect: Rect, radius: f32, color: Color32) {
    painter.rect_stroke(rect, CornerRadius::from(radius), Stroke::new(1.0, color), egui::StrokeKind::Inside);
}

/// Text centred vertically at `left`, clipped to `max_width` with an ellipsis.
pub fn text_in_row(painter: &Painter, left: Pos2, text: &str, size: f32, color: Color32, max_width: f32) {
    let galley = painter.layout_job(elided_job(text, size, color, max_width));
    painter.galley(left - egui::vec2(0.0, galley.size().y / 2.0), galley, color);
}

fn elided_job(text: &str, size: f32, color: Color32, max_width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), FontId::proportional(size), color);
    job.wrap = egui::text::TextWrapping {
        max_width: max_width.max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    job
}
