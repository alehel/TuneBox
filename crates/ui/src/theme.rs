//! TuneBox's look: modern-retro. The classic desktop jukebox's anatomy (brushed toolbar, status
//! display, blue-grey source list, zebra-striped list) drawn with today's manners: softer greys,
//! rounded selections, gentle shadows and generous spacing. Every colour and shared shape lives
//! here.

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{
    Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Shadow, Stroke, TextStyle, Visuals, pos2, vec2,
};

pub const WINDOW_BG: Color32 = Color32::from_rgb(0xEE, 0xEF, 0xF1);
pub const TEXT: Color32 = Color32::from_rgb(0x1D, 0x1F, 0x22);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x78, 0x7C, 0x82);
/// The missing-file mark: a warm amber that stands out in the list without shouting.
pub const CAUTION: Color32 = Color32::from_rgb(0xD9, 0x7B, 0x1E);
/// Text that warns: a value that will not be saved, a folder that cannot be used.
pub const WARNING: Color32 = Color32::from_rgb(0xA3, 0x2A, 0x22);
pub const BORDER: Color32 = Color32::from_rgb(0xC3, 0xC6, 0xCB);
pub const BORDER_DARK: Color32 = Color32::from_rgb(0x9A, 0x9D, 0xA3);

/// Toolbar and status bar: brushed aluminium, lighter than it used to be.
pub const BAR_TOP: Color32 = Color32::from_rgb(0xF6, 0xF6, 0xF7);
pub const BAR_BOTTOM: Color32 = Color32::from_rgb(0xD5, 0xD7, 0xDA);
pub const BAR_HIGHLIGHT: Color32 = Color32::WHITE;

pub const SIDEBAR_BG: Color32 = Color32::from_rgb(0xE4, 0xE9, 0xEF);
pub const SIDEBAR_HEADING: Color32 = Color32::from_rgb(0x82, 0x8C, 0x98);
pub const SIDEBAR_HOVER: Color32 = Color32::from_rgba_premultiplied(0x10, 0x18, 0x28, 0x12);
pub const SELECTION_TOP: Color32 = Color32::from_rgb(0x5F, 0x9E, 0xF0);
pub const SELECTION_BOTTOM: Color32 = Color32::from_rgb(0x2D, 0x6F, 0xD6);

/// Source-list icon colours, one per kind of source.
pub const ICON_LIBRARY: Color32 = Color32::from_rgb(0x2F, 0x7C, 0xE8);
pub const ICON_DEVICE: Color32 = Color32::from_rgb(0x4A, 0x52, 0x5C);
pub const ICON_DISC: Color32 = Color32::from_rgb(0x2A, 0x9D, 0x8F);
pub const ICON_SMART: Color32 = Color32::from_rgb(0x8E, 0x5C, 0xD9);
pub const ICON_PLAYLIST: Color32 = Color32::from_rgb(0x5B, 0x67, 0x76);

pub const LIST_BG: Color32 = Color32::WHITE;
pub const ROW_ALT: Color32 = Color32::from_rgb(0xF3, 0xF6, 0xFA);
pub const ROW_HOVER: Color32 = Color32::from_rgb(0xE4, 0xEE, 0xFC);
pub const ROW_SELECTED: Color32 = Color32::from_rgb(0x2F, 0x74, 0xDA);
pub const ROW_SELECTED_TOP: Color32 = Color32::from_rgb(0x4A, 0x8A, 0xE6);
pub const HEADER_TOP: Color32 = Color32::from_rgb(0xFD, 0xFD, 0xFD);
pub const HEADER_BOTTOM: Color32 = Color32::from_rgb(0xF0, 0xF1, 0xF3);
pub const HEADER_SORTED_TOP: Color32 = Color32::from_rgb(0xEF, 0xF5, 0xFE);
pub const HEADER_SORTED_BOTTOM: Color32 = Color32::from_rgb(0xDC, 0xE9, 0xFB);
pub const GRID_LINE: Color32 = Color32::from_rgb(0xE2, 0xE4, 0xE8);

/// The status display in the middle of the toolbar: the jukebox's signature, kept.
pub const LCD_TOP: Color32 = Color32::from_rgb(0xFC, 0xFD, 0xF8);
pub const LCD_BOTTOM: Color32 = Color32::from_rgb(0xE9, 0xEC, 0xE0);
pub const LCD_TEXT: Color32 = Color32::from_rgb(0x25, 0x28, 0x21);
pub const LCD_TEXT_MUTED: Color32 = Color32::from_rgb(0x5E, 0x63, 0x58);
pub const LCD_TRACK: Color32 = Color32::from_rgb(0xC8, 0xCC, 0xBF);
pub const LCD_FILL: Color32 = Color32::from_rgb(0x4B, 0x4F, 0x46);

pub const GLYPH: Color32 = Color32::from_rgb(0x3A, 0x3D, 0x42);
pub const GLYPH_PRESSED: Color32 = Color32::from_rgb(0x12, 0x14, 0x18);
pub const GLYPH_DISABLED: Color32 = Color32::from_rgb(0xAA, 0xAD, 0xB2);
pub const ACCENT: Color32 = Color32::from_rgb(0x2F, 0x74, 0xDA);

/// Capacity bar segments on the device page.
pub const CAPACITY_AUDIO: Color32 = Color32::from_rgb(0x4C, 0x8F, 0xE0);
pub const CAPACITY_OTHER: Color32 = Color32::from_rgb(0xE9, 0xA9, 0x3B);
pub const CAPACITY_FREE: Color32 = Color32::from_rgb(0xF4, 0xF5, 0xF6);

pub const ROW_HEIGHT: f32 = 22.0;
pub const SMALL: f32 = 11.0;
pub const BODY: f32 = 13.0;

/// Installs TuneBox's visuals and text sizes.
pub fn install(ctx: &egui::Context) {
    crate::fonts::install(ctx);
    ctx.set_theme(egui::Theme::Light);
    let mut visuals = Visuals::light();
    visuals.panel_fill = WINDOW_BG;
    visuals.window_fill = Color32::from_rgb(0xF7, 0xF7, 0xF8);
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.window_shadow = Shadow { offset: [0, 10], blur: 34, spread: 0, color: Color32::from_black_alpha(55) };
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.popup_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(40) };
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
            (TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(BODY, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 5.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.interact_size.y = 24.0;
        // Slim scroll bars that widen under the pointer, floating over the content.
        style.spacing.scroll = egui::style::ScrollStyle::floating();
    });
}

/// Buttons, fields, checkboxes and radio buttons with soft outlines and rounded corners.
fn style_widgets(widgets: &mut egui::style::Widgets) {
    let radius = CornerRadius::same(6);
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, GRID_LINE);
    widgets.noninteractive.corner_radius = radius;
    for (state, fill, border) in [
        (&mut widgets.inactive, Color32::WHITE, Color32::from_gray(0xB8)),
        (&mut widgets.hovered, Color32::WHITE, Color32::from_gray(0x8C)),
        (&mut widgets.active, Color32::from_gray(0xE2), Color32::from_gray(0x70)),
        (&mut widgets.open, Color32::from_gray(0xF0), Color32::from_gray(0x8C)),
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
    rounded_gradient(painter, rect, 0.0, top, bottom);
}

/// Fills `rect`, with rounded corners of `radius`, with a vertical gradient. The shape is a
/// fan of triangles around the centre, each vertex coloured by its height.
pub fn rounded_gradient(painter: &Painter, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
    let color_at = |y: f32| lerp_color(top, bottom, ((y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0));
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex { pos: rect.center(), uv: WHITE_UV, color: color_at(rect.center().y) });
    let outline = rounded_outline(rect, radius);
    for pos in &outline {
        mesh.vertices.push(Vertex { pos: *pos, uv: WHITE_UV, color: color_at(pos.y) });
    }
    let count = outline.len() as u32;
    for i in 0..count {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % count);
    }
    painter.add(mesh);
}

/// The outline of a rounded rectangle, clockwise from the top-left corner.
fn rounded_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    if r < 0.5 {
        return vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    }
    const STEPS: usize = 6;
    let corners = [
        (pos2(rect.left() + r, rect.top() + r), std::f32::consts::PI),
        (pos2(rect.right() - r, rect.top() + r), 1.5 * std::f32::consts::PI),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 0.5 * std::f32::consts::PI),
    ];
    corners
        .iter()
        .flat_map(|(center, start)| {
            (0..=STEPS).map(move |i| {
                let a = start + std::f32::consts::FRAC_PI_2 * i as f32 / STEPS as f32;
                *center + vec2(a.cos(), a.sin()) * r
            })
        })
        .collect()
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_premultiplied(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()), mix(a.a(), b.a()))
}

/// A soft drop shadow under a `radius`-rounded `rect`.
pub fn soft_shadow(painter: &Painter, rect: Rect, radius: f32, depth: u8) {
    let shadow = Shadow {
        offset: [0, (depth / 3).max(1) as i8],
        blur: depth,
        spread: 0,
        color: Color32::from_black_alpha(36 + depth / 2),
    };
    painter.add(shadow.as_shape(rect, CornerRadius::from(radius)));
}

/// A raised, glossy capsule: the shape of the toolbar's controls.
pub fn capsule(painter: &Painter, rect: Rect, pressed: bool) {
    let radius = rect.height() / 2.0;
    soft_shadow(painter, rect, radius, 4);
    let (top, bottom) = if pressed {
        (Color32::from_rgb(0xD6, 0xD8, 0xDC), Color32::from_rgb(0xEE, 0xEF, 0xF1))
    } else {
        (Color32::WHITE, Color32::from_rgb(0xE3, 0xE5, 0xE8))
    };
    rounded_gradient(painter, rect, radius, top, bottom);
    painter.rect_stroke(
        rect,
        CornerRadius::from(radius),
        Stroke::new(1.0, Color32::from_rgb(0xB4, 0xB8, 0xBE)),
        egui::StrokeKind::Inside,
    );
}

/// A toolbar-style bar: aluminium gradient, a highlight along the top and a soft line below.
pub fn bar_background(painter: &Painter, rect: Rect) {
    vertical_gradient(painter, rect, BAR_TOP, BAR_BOTTOM);
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, BAR_HIGHLIGHT));
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, BORDER_DARK));
}

/// The glossy blue of a selected source, as a rounded, inset pill.
pub fn selection_background(painter: &Painter, rect: Rect) {
    rounded_gradient(painter, rect, 6.0, SELECTION_TOP, SELECTION_BOTTOM);
    // A thin highlight along the top edge keeps the classic gloss.
    let gloss = Rect::from_min_size(rect.min + vec2(6.0, 1.0), vec2(rect.width() - 12.0, 1.0));
    painter.rect_filled(gloss, 0.0, Color32::from_white_alpha(70));
}

/// Text centred vertically at `left`, clipped to `max_width` with an ellipsis.
pub fn text_in_row(painter: &Painter, left: Pos2, text: &str, size: f32, color: Color32, max_width: f32) {
    font_in_row(painter, left, text, FontId::proportional(size), color, max_width);
}

/// [`text_in_row`] in any font (the bold face, for titles).
pub fn font_in_row(painter: &Painter, left: Pos2, text: &str, font: FontId, color: Color32, max_width: f32) {
    let galley = painter.layout_job(elided_job(text, font, color, max_width, 0.0));
    painter.galley(left - egui::vec2(0.0, galley.size().y / 2.0), galley, color);
}

/// Small capitals with a little air between the letters: the source list's headings.
pub fn spaced_caps(painter: &Painter, left_bottom: Pos2, text: &str, color: Color32) {
    let galley =
        painter.layout_job(elided_job(&text.to_uppercase(), FontId::proportional(10.0), color, f32::INFINITY, 0.9));
    painter.galley(left_bottom - egui::vec2(0.0, galley.size().y), galley, color);
}

fn elided_job(text: &str, font_id: FontId, color: Color32, max_width: f32, spacing: f32) -> egui::text::LayoutJob {
    let format = egui::TextFormat { font_id, color, extra_letter_spacing: spacing, ..Default::default() };
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(), format);
    job.wrap = egui::text::TextWrapping {
        max_width: max_width.max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_outlines_are_closed_and_inside_the_rect() {
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(80.0, 24.0));
        let points = rounded_outline(rect, 12.0);
        assert_eq!(points.len(), 28);
        assert!(points.iter().all(|p| rect.expand(0.01).contains(*p)));
        assert_eq!(rounded_outline(rect, 0.0).len(), 4);
    }

    #[test]
    fn colours_blend_evenly() {
        assert_eq!(lerp_color(Color32::BLACK, Color32::WHITE, 0.5), Color32::from_rgb(128, 128, 128));
    }
}
