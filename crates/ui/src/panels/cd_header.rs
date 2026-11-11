//! The CD page's header: the disc (or its cover), its names and the import actions.
//!
//! TRIAL: three layouts behind `TUNEBOX_CD_HEADER` (a, b, c) to compare in screenshots.

use crate::actions::Action;
use crate::panels::{PanelContext, widgets};
use crate::theme;
use egui::{Align, Align2, Color32, FontId, Layout, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use tunebox_engine::DiscInfo;
use tunebox_engine::commands::CommandId;
use tunebox_engine::{DiscCover, JobKind};

/// Height of a field or button row.
const ROW: f32 = 26.0;
/// Space between rows.
const GAP: f32 = 8.0;

pub struct Disc<'a> {
    pub info: &'a mut DiscInfo,
    pub cover: &'a DiscCover,
    pub release: &'a str,
    pub summary: &'a str,
}

/// Draws the header at the top of `area`; returns whether the names changed and the header's
/// bottom edge.
pub fn show(ui: &mut Ui, cx: &mut PanelContext, area: Rect, disc: Disc) -> (bool, f32) {
    match std::env::var("TUNEBOX_CD_HEADER").as_deref() {
        Ok("b") => hero(ui, cx, area, disc),
        Ok("c") => action_bar(ui, cx, area, disc),
        _ => grid(ui, cx, area, disc),
    }
}

// ----- A: one grid for fields and buttons -----

fn grid(ui: &mut Ui, cx: &mut PanelContext, area: Rect, disc: Disc) -> (bool, f32) {
    const PAD: f32 = 18.0;
    const NOTE: f32 = 16.0;
    const BUTTONS: f32 = 150.0;
    const LABEL: f32 = 48.0;
    let art_size = 3.0 * ROW + 3.0 * GAP + NOTE;
    let header = Rect::from_min_size(area.min, vec2(area.width(), art_size + 2.0 * PAD));
    background(ui, header);
    let top = header.top() + PAD;
    let art = Rect::from_min_size(pos2(header.left() + 20.0, top), vec2(art_size, art_size));
    paint_art(ui, cx, art, disc.cover, disc.release);

    let buttons_left = header.right() - 20.0 - BUTTONS;
    let label_left = art.right() + 20.0;
    let field_left = label_left + LABEL + 10.0;
    let field_right = (buttons_left - 32.0).min(field_left + 560.0).max(field_left + 200.0);
    let row = |i: f32| top + i * (ROW + GAP);
    let mut changed = false;
    let info = disc.info;

    for (i, (label, value)) in [("Album", &mut info.album), ("Artist", &mut info.artist)].into_iter().enumerate() {
        let y = row(i as f32);
        right_label(ui, Rect::from_min_size(pos2(label_left, y), vec2(LABEL, ROW)), label);
        changed |= text_field(ui, Rect::from_min_max(pos2(field_left, y), pos2(field_right, y + ROW)), value);
    }
    // Genre takes what Year and Compilation leave, so the row ends where Album and Artist do.
    let y = row(2.0);
    right_label(ui, Rect::from_min_size(pos2(label_left, y), vec2(LABEL, ROW)), "Genre");
    let compilation = Rect::from_min_max(pos2(field_right - 104.0, y), pos2(field_right, y + ROW));
    let year = Rect::from_min_size(pos2(compilation.left() - 16.0 - 56.0, y), vec2(56.0, ROW));
    let year_label = Rect::from_min_size(pos2(year.left() - 8.0 - 32.0, y), vec2(32.0, ROW));
    let genre = Rect::from_min_max(pos2(field_left, y), pos2(year_label.left() - 14.0, y + ROW));
    changed |= text_field(ui, genre, &mut info.genre);
    right_label(ui, year_label, "Year");
    changed |= year_field(ui, year, &mut info.year);
    changed |= checkbox(ui, compilation, &mut info.compilation, "Compilation");

    let button = |i: f32| Rect::from_min_size(pos2(buttons_left, row(i)), vec2(BUTTONS, ROW));
    import_button(ui, cx, button(0.0));
    names_button(ui, cx, button(1.0));
    cover_button(ui, cx, button(2.0));

    // One quiet line under everything: the cover note on the left, how it imports on the right.
    let note = Rect::from_min_max(pos2(label_left, row(3.0)), pos2(header.right() - 20.0, row(3.0) + NOTE));
    if let Some(text) = cover_note(disc.cover) {
        muted(ui, note.left_center(), Align2::LEFT_CENTER, &text);
    }
    muted(ui, note.right_center(), Align2::RIGHT_CENTER, &import_settings(cx));
    (changed, header.bottom())
}

// ----- B: an album page, with the names as titles -----

fn hero(ui: &mut Ui, cx: &mut PanelContext, area: Rect, disc: Disc) -> (bool, f32) {
    const PAD: f32 = 20.0;
    const ART: f32 = 140.0;
    let header = Rect::from_min_size(area.min, vec2(area.width(), ART + 2.0 * PAD));
    background(ui, header);
    let top = header.top() + PAD;
    let art = Rect::from_min_size(pos2(header.left() + 22.0, top), vec2(ART, ART));
    paint_art(ui, cx, art, disc.cover, disc.release);
    let left = art.right() + 22.0;
    let right = (header.right() - 22.0).min(left + 640.0);
    let info = disc.info;
    let mut changed = false;

    changed |= quiet_field(
        ui,
        Rect::from_min_max(pos2(left, top - 2.0), pos2(right, top + 30.0)),
        &mut info.album,
        "Album",
        crate::fonts::bold(22.0),
        theme::TEXT,
    );
    changed |= quiet_field(
        ui,
        Rect::from_min_max(pos2(left, top + 32.0), pos2(right, top + 56.0)),
        &mut info.artist,
        "Artist",
        FontId::proportional(16.0),
        Color32::from_rgb(0x4A, 0x4E, 0x55),
    );
    // Genre · Year · Compilation, each field as wide as its text, then the disc's length.
    let y = top + 58.0;
    let meta = FontId::proportional(13.5);
    let genre_width = fit(ui, &info.genre, "Genre", &meta);
    let genre = Rect::from_min_size(pos2(left, y), vec2(genre_width, 22.0));
    changed |= quiet_field(ui, genre, &mut info.genre, "Genre", meta.clone(), theme::TEXT_MUTED);
    dot(ui, pos2(genre.right() + 7.0, genre.center().y));
    let mut year = info.year.map(|y| y.to_string()).unwrap_or_default();
    let year_rect = Rect::from_min_size(pos2(genre.right() + 14.0, y), vec2(fit(ui, &year, "Year", &meta), 22.0));
    if quiet_field(ui, year_rect, &mut year, "Year", meta, theme::TEXT_MUTED) {
        info.year = year.trim().parse().ok();
        changed = true;
    }
    dot(ui, pos2(year_rect.right() + 7.0, year_rect.center().y));
    let compilation = Rect::from_min_size(pos2(year_rect.right() + 16.0, y), vec2(110.0, 22.0));
    changed |= checkbox(ui, compilation, &mut info.compilation, "Compilation");
    let details = match cover_note(disc.cover) {
        Some(note) => format!("{} · {note}", disc.summary),
        None => format!("{} · {}", disc.summary, import_settings(cx)),
    };
    muted(ui, pos2(left, top + 92.0), Align2::LEFT_CENTER, &details);

    let y = art.bottom() - ROW;
    import_button(ui, cx, Rect::from_min_size(pos2(left, y), vec2(124.0, ROW)));
    names_button(ui, cx, Rect::from_min_size(pos2(left + 134.0, y), vec2(136.0, ROW)));
    cover_button(ui, cx, Rect::from_min_size(pos2(left + 280.0, y), vec2(116.0, ROW)));
    (changed, header.bottom())
}

// ----- C: a compact form over an action bar -----

fn action_bar(ui: &mut Ui, cx: &mut PanelContext, area: Rect, disc: Disc) -> (bool, f32) {
    const PAD: f32 = 16.0;
    const ART: f32 = 2.0 * ROW + GAP + 2.0 * PAD;
    const BAR: f32 = 42.0;
    const LABEL: f32 = 48.0;
    let form = Rect::from_min_size(area.min, vec2(area.width(), ART + 2.0 * PAD));
    let bar = Rect::from_min_size(pos2(area.left(), form.bottom()), vec2(area.width(), BAR));
    background(ui, form.union(bar));
    ui.painter().hline(bar.x_range(), bar.top() + 0.5, Stroke::new(1.0, theme::GRID_LINE));
    let art = Rect::from_min_size(form.min + vec2(20.0, PAD), vec2(ART, ART));
    paint_art(ui, cx, art, disc.cover, disc.release);

    // Two columns: Album | Artist, then Genre | Year and Compilation.
    let top = art.top() + (ART - 2.0 * ROW - GAP) / 2.0;
    let left = art.right() + 20.0;
    let right = (form.right() - 20.0).min(left + 900.0);
    let column = (right - left - 24.0) / 2.0;
    let second = left + column + 24.0;
    let field = |x: f32, y: f32, width: f32| Rect::from_min_max(pos2(x + LABEL + 10.0, y), pos2(x + width, y + ROW));
    let label = |x: f32, y: f32| Rect::from_min_size(pos2(x, y), vec2(LABEL, ROW));
    let info = disc.info;
    let mut changed = false;
    right_label(ui, label(left, top), "Album");
    changed |= text_field(ui, field(left, top, column), &mut info.album);
    right_label(ui, label(second, top), "Artist");
    changed |= text_field(ui, field(second, top, column), &mut info.artist);
    let y = top + ROW + GAP;
    right_label(ui, label(left, y), "Genre");
    changed |= text_field(ui, field(left, y, column), &mut info.genre);
    right_label(ui, label(second, y), "Year");
    let year = Rect::from_min_size(pos2(second + LABEL + 10.0, y), vec2(64.0, ROW));
    changed |= year_field(ui, year, &mut info.year);
    let compilation = Rect::from_min_size(pos2(year.right() + 18.0, y), vec2(120.0, ROW));
    changed |= checkbox(ui, compilation, &mut info.compilation, "Compilation");

    // The bar: what importing will do on the left, the actions on the right, Import CD last.
    let y = bar.center().y - ROW / 2.0;
    let status = match cover_note(disc.cover) {
        Some(note) => format!("{} · {note}", import_settings(cx)),
        None => import_settings(cx),
    };
    muted(ui, pos2(bar.left() + 20.0, bar.center().y), Align2::LEFT_CENTER, &status);
    let import = Rect::from_min_size(pos2(bar.right() - 20.0 - 124.0, y), vec2(124.0, ROW));
    let cover = Rect::from_min_size(pos2(import.left() - 10.0 - 116.0, y), vec2(116.0, ROW));
    let names = Rect::from_min_size(pos2(cover.left() - 8.0 - 136.0, y), vec2(136.0, ROW));
    names_button(ui, cx, names);
    cover_button(ui, cx, cover);
    import_button(ui, cx, import);
    (changed, bar.bottom())
}

// ----- shared pieces -----

fn background(ui: &Ui, header: Rect) {
    theme::vertical_gradient(
        ui.painter(),
        header,
        Color32::from_rgb(0xFA, 0xFA, 0xFB),
        Color32::from_rgb(0xEC, 0xEE, 0xF1),
    );
    ui.painter().hline(header.x_range(), header.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));
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
        _ => super::cd_view::paint_disc(ui.painter(), art),
    }
}

fn cover_note(cover: &DiscCover) -> Option<String> {
    match cover {
        DiscCover::NotAsked | DiscCover::Found { from: tunebox_engine::CoverOf::Release, .. } => None,
        DiscCover::Found { .. } => Some("Cover from another edition of this album".to_owned()),
        DiscCover::Fetching => Some("Getting the cover from the Cover Art Archive…".to_owned()),
        DiscCover::NoneFound => Some("The Cover Art Archive has no cover for this release.".to_owned()),
        DiscCover::Failed(reason) => Some(format!("The cover could not be fetched: {reason}.")),
    }
}

fn import_settings(cx: &PanelContext) -> String {
    let format = cx.session.preferences().rip_format.encoder_name();
    let offset = cx.session.cd_read_offset().map(|(_, offset)| offset).unwrap_or(0);
    format!("Imports with the {format} · Drive read offset {offset:+} samples")
}

fn muted(ui: &Ui, at: egui::Pos2, align: Align2, text: &str) {
    ui.painter().text(at, align, text, FontId::proportional(11.5), theme::TEXT_MUTED);
}

fn right_label(ui: &Ui, rect: Rect, text: &str) {
    ui.painter().text(rect.right_center(), Align2::RIGHT_CENTER, text, FontId::proportional(13.0), theme::TEXT_MUTED);
}

fn text_field(ui: &mut Ui, rect: Rect, value: &mut String) -> bool {
    ui.put(rect, egui::TextEdit::singleline(value).align(Align2::LEFT_CENTER)).changed()
}

fn year_field(ui: &mut Ui, rect: Rect, year: &mut Option<u32>) -> bool {
    let mut text = year.map(|y| y.to_string()).unwrap_or_default();
    if text_field(ui, rect, &mut text) {
        *year = text.trim().parse().ok();
        return true;
    }
    false
}

/// The width of a quiet field showing `text` (or its hint, when empty).
fn fit(ui: &Ui, text: &str, hint: &str, font: &FontId) -> f32 {
    let shown = if text.is_empty() { hint } else { text };
    ui.painter().layout_no_wrap(shown.to_owned(), font.clone(), Color32::BLACK).size().x + 2.0
}

fn dot(ui: &Ui, at: egui::Pos2) {
    ui.painter().text(at, Align2::CENTER_CENTER, "·", FontId::proportional(13.5), theme::TEXT_MUTED);
}

/// A borderless field that shows a frame only while hovered or editing, for names shown as titles.
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
        // The frame reaches past the text, which stays in line with the rest of the header.
        ui.painter().rect_stroke(rect.expand2(vec2(5.0, 0.0)), 4.0, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    }
    response.changed()
}

fn checkbox(ui: &mut Ui, rect: Rect, value: &mut bool, label: &str) -> bool {
    ui.scope_builder(UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.checkbox(value, label).changed()
    })
    .inner
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
    if secondary(ui, rect, "Get Track Names", !looking).clicked() {
        cx.act(Action::Run(CommandId::GetTrackNames));
    }
}

fn cover_button(ui: &mut Ui, cx: &mut PanelContext, rect: Rect) {
    let can_fetch = cx.session.is_enabled(CommandId::GetCdCover, Default::default());
    let response = secondary(ui, rect, "Get CD Cover", can_fetch)
        .on_disabled_hover_text("Get Track Names first: the cover is found by the MusicBrainz release they come from.");
    if response.clicked() {
        cx.act(Action::Run(CommandId::GetCdCover));
    }
}

/// A plain button filling `rect`, its label centred.
fn secondary(ui: &mut Ui, rect: Rect, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| ui.put(rect, egui::Button::new(label))).inner
}
