//! The CD page's header, laid out like an album page: the disc (or its cover, once fetched
//! from the Cover Art Archive), the album's names as titles, a line of details, and the
//! actions in a row beneath them.
//!
//! The names are edited where they are shown, in fields with no frame until they are hovered
//! or focused, as the track names in the table below are.

use crate::actions::Action;
use crate::panels::{PanelContext, widgets};
use crate::theme;
use egui::{Align, Align2, Color32, FontId, Layout, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use tunebox_engine::commands::CommandId;
use tunebox_engine::{CoverOf, DiscCover, DiscInfo, JobKind};

/// Space around the art and between it and the text.
const PAD: f32 = 20.0;
/// The disc or cover; the text column and the buttons fill its height.
const ART: f32 = 140.0;
/// Height of the buttons.
const BUTTON: f32 = 26.0;
/// The genre and year: smaller than the names, larger than the details.
const META_SIZE: f32 = 13.5;
/// The text column stops growing here, so a long title stays near the art on a wide window.
const MAX_TEXT_WIDTH: f32 = 640.0;

/// What the header shows. `info` is edited in place.
pub struct Disc<'a> {
    pub info: &'a mut DiscInfo,
    pub cover: &'a DiscCover,
    /// The MusicBrainz release, which keys the cover's texture.
    pub release: &'a str,
    /// "9 songs, 34 minutes".
    pub summary: &'a str,
}

/// Draws the header across the top of `area`. Returns whether the names changed and the
/// header's bottom edge, where the track table starts.
pub fn show(ui: &mut Ui, cx: &mut PanelContext, area: Rect, disc: Disc) -> (bool, f32) {
    let header = Rect::from_min_size(area.min, vec2(area.width(), ART + 2.0 * PAD));
    theme::vertical_gradient(
        ui.painter(),
        header,
        Color32::from_rgb(0xFA, 0xFA, 0xFB),
        Color32::from_rgb(0xEC, 0xEE, 0xF1),
    );
    ui.painter().hline(header.x_range(), header.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));
    let top = header.top() + PAD;
    let art = Rect::from_min_size(pos2(header.left() + PAD, top), vec2(ART, ART));
    paint_art(ui, cx, art, disc.cover, disc.release);

    // The text column, top to bottom: album (0–30), artist (32–56), genre · year · compilation
    // (58–80), details (centred on 92), then the buttons along the art's bottom edge.
    let left = art.right() + PAD;
    let right = (header.right() - PAD).min(left + MAX_TEXT_WIDTH).max(left);
    let info = disc.info;
    let mut changed = false;
    let album = Rect::from_min_max(pos2(left, top - 2.0), pos2(right, top + 30.0));
    changed |= quiet_field(ui, album, &mut info.album, "Album", crate::fonts::bold(22.0), theme::TEXT);
    let artist = Rect::from_min_max(pos2(left, top + 32.0), pos2(right, top + 56.0));
    let artist_color = Color32::from_rgb(0x4A, 0x4E, 0x55);
    changed |= quiet_field(ui, artist, &mut info.artist, "Artist", FontId::proportional(16.0), artist_color);
    changed |= meta_row(ui, info, pos2(left, top + 58.0));

    let details = match cover_note(disc.cover) {
        Some(note) => format!("{} · {note}", disc.summary),
        None => format!("{} · {}", disc.summary, import_settings(cx)),
    };
    let details_at = pos2(left, top + 92.0);
    theme::text_in_row(ui.painter(), details_at, &details, 11.5, theme::TEXT_MUTED, header.right() - PAD - left);

    let y = art.bottom() - BUTTON;
    let import = Rect::from_min_size(pos2(left, y), vec2(124.0, BUTTON));
    let names = Rect::from_min_size(pos2(import.right() + 10.0, y), vec2(136.0, BUTTON));
    let cover = Rect::from_min_size(pos2(names.right() + 10.0, y), vec2(116.0, BUTTON));
    import_button(ui, cx, import);
    names_button(ui, cx, names);
    cover_button(ui, cx, cover);
    (changed, header.bottom())
}

/// Genre · Year · Compilation, starting at `at`. Each field is as wide as its text, so the
/// row reads as one line of details.
fn meta_row(ui: &mut Ui, info: &mut DiscInfo, at: egui::Pos2) -> bool {
    let font = FontId::proportional(META_SIZE);
    let mut changed = false;
    let genre = Rect::from_min_size(at, vec2(fit(ui, &info.genre, "Genre", &font), 22.0));
    changed |= quiet_field(ui, genre, &mut info.genre, "Genre", font.clone(), theme::TEXT_MUTED);
    dot(ui, pos2(genre.right() + 7.0, genre.center().y));
    let mut year = info.year.map(|y| y.to_string()).unwrap_or_default();
    let year_rect = Rect::from_min_size(pos2(genre.right() + 14.0, at.y), vec2(fit(ui, &year, "Year", &font), 22.0));
    if quiet_field(ui, year_rect, &mut year, "Year", font, theme::TEXT_MUTED) {
        info.year = year.trim().parse().ok();
        changed = true;
    }
    dot(ui, pos2(year_rect.right() + 7.0, year_rect.center().y));
    let compilation = Rect::from_min_size(pos2(year_rect.right() + 16.0, at.y), vec2(110.0, 22.0));
    changed |= ui
        .scope_builder(UiBuilder::new().max_rect(compilation).layout(Layout::left_to_right(Align::Center)), |ui| {
            ui.checkbox(&mut info.compilation, "Compilation").changed()
        })
        .inner;
    changed
}

/// A field with no frame until it is hovered or focused, for names shown as text.
fn quiet_field(ui: &mut Ui, rect: Rect, value: &mut String, hint: &str, font: FontId, color: Color32) -> bool {
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
fn fit(ui: &Ui, text: &str, hint: &str, font: &FontId) -> f32 {
    let shown = if text.is_empty() { hint } else { text };
    // The extra room is for the text cursor at the end.
    ui.painter().layout_no_wrap(shown.to_owned(), font.clone(), Color32::BLACK).size().x + 2.0
}

fn dot(ui: &Ui, at: egui::Pos2) {
    ui.painter().text(at, Align2::CENTER_CENTER, "·", FontId::proportional(META_SIZE), theme::TEXT_MUTED);
}

fn cover_note(cover: &DiscCover) -> Option<String> {
    match cover {
        DiscCover::NotAsked | DiscCover::Found { from: CoverOf::Release, .. } => None,
        DiscCover::Found { .. } => Some("Cover from another edition of this album".to_owned()),
        DiscCover::Fetching => Some("Getting the cover from the Cover Art Archive…".to_owned()),
        DiscCover::NoneFound => Some("The Cover Art Archive has no cover for this release".to_owned()),
        DiscCover::Failed(reason) => Some(format!("The cover could not be fetched: {reason}")),
    }
}

fn import_settings(cx: &PanelContext) -> String {
    let format = cx.session.preferences().rip_format.encoder_name();
    let offset = cx.session.cd_read_offset().map(|(_, offset)| offset).unwrap_or(0);
    format!("Imports with the {format} · Drive read offset {offset:+} samples")
}

fn import_button(ui: &mut Ui, cx: &mut PanelContext, rect: Rect) {
    let busy = cx.session.is_busy(JobKind::Rip);
    let label = if busy { "Importing…" } else { "Import CD" };
    let sense = if busy { Sense::hover() } else { Sense::click() };
    let response = ui.interact(rect, ui.scope_id().with("import-cd"), sense);
    widgets::paint_primary(ui, rect, label, !busy, &response);
    if response.clicked() {
        cx.act(Action::Run(CommandId::ImportCd));
    }
}

fn names_button(ui: &mut Ui, cx: &mut PanelContext, rect: Rect) {
    let looking = cx.session.is_busy(JobKind::LookupNames);
    if secondary_button(ui, rect, "Get Track Names", !looking).clicked() {
        cx.act(Action::Run(CommandId::GetTrackNames));
    }
}

fn cover_button(ui: &mut Ui, cx: &mut PanelContext, rect: Rect) {
    let can_fetch = cx.session.is_enabled(CommandId::GetCdCover, Default::default());
    let response = secondary_button(ui, rect, "Get CD Cover", can_fetch)
        .on_disabled_hover_text("Get Track Names first: the cover is found by the MusicBrainz release they come from.");
    if response.clicked() {
        cx.act(Action::Run(CommandId::GetCdCover));
    }
}

/// A plain button filling `rect`. `put` centres its label; egui otherwise aligns a button's
/// label with the surrounding layout.
fn secondary_button(ui: &mut Ui, rect: Rect, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| ui.put(rect, egui::Button::new(label))).inner
}

fn paint_art(ui: &mut Ui, cx: &mut PanelContext, art: Rect, cover: &DiscCover, release: &str) {
    match cover {
        DiscCover::Found { artwork, .. } => {
            let texture = cx.artwork.picture(ui.ctx(), &format!("cd:{release}"), &artwork.data);
            theme::soft_shadow(ui.painter(), art, 6.0, 10);
            crate::artwork::paint(ui, art, texture.as_ref());
            ui.painter().rect_stroke(
                art,
                6.0,
                Stroke::new(1.0, Color32::from_black_alpha(25)),
                egui::StrokeKind::Inside,
            );
        }
        _ => paint_disc(ui.painter(), art),
    }
}

/// A compact disc: a silver platter with a faint rainbow sheen, the clear hub and the hole.
fn paint_disc(painter: &egui::Painter, rect: Rect) {
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
