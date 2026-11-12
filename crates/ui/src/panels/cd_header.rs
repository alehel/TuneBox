//! The CD page's header, laid out like an album page: the disc (or its cover, once fetched
//! from the Cover Art Archive), the album's names as titles, a line of details, and the
//! actions in a row beneath them.
//!
//! The names are edited where they are shown, in fields with no frame until they are hovered
//! or focused, as the track names in the table below are. The parts it shares with the
//! audiobook page are in `album_header`.

use crate::actions::Action;
use crate::panels::album_header::{
    BUTTON, META_SIZE, background, details_line, dot, fit, paint_art, quiet_field, secondary_button, text_column,
};
use crate::panels::{PanelContext, widgets};
use crate::state::SourceItem;
use crate::theme;
use egui::{Align, Color32, FontId, Layout, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use tunebox_engine::commands::CommandId;
use tunebox_engine::{CoverOf, DiscCover, DiscInfo, JobKind};

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
    let (header, art) = background(ui, area);
    let top = art.top();
    let cover = match disc.cover {
        DiscCover::Found { artwork, .. } => Some((format!("cd:{}", disc.release), artwork.data.as_slice())),
        _ => None,
    };
    paint_art(ui, cx, art, cover.as_ref().map(|(key, data)| (key.as_str(), *data)));

    // The text column, top to bottom: album (0–30), artist (32–56), genre · year · compilation
    // (58–80), details (centred on 92), then the buttons along the art's bottom edge.
    let (left, right) = text_column(header, art);
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
    details_line(ui, header, pos2(left, top + 92.0), &details);

    let y = art.bottom() - BUTTON;
    let import = Rect::from_min_size(pos2(left, y), vec2(124.0, BUTTON));
    let names = Rect::from_min_size(pos2(import.right() + 10.0, y), vec2(136.0, BUTTON));
    let cover = Rect::from_min_size(pos2(names.right() + 10.0, y), vec2(116.0, BUTTON));
    let book = Rect::from_min_size(pos2(cover.right() + 10.0, y), vec2(164.0, BUTTON));
    import_button(ui, cx, import);
    names_button(ui, cx, names);
    cover_button(ui, cx, cover);
    audiobook_button(ui, cx, book);
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

/// Import as Audiobook…, or Show Audiobook when a book is in progress.
fn audiobook_button(ui: &mut Ui, cx: &mut PanelContext, rect: Rect) {
    if cx.session.audiobook().is_some() {
        if secondary_button(ui, rect, "Show Audiobook", true).clicked() {
            cx.act(Action::SelectSource(SourceItem::Book));
        }
        return;
    }
    let enabled = cx.session.is_enabled(CommandId::ImportAsAudiobook, Default::default());
    if secondary_button(ui, rect, "Import as Audiobook…", enabled).clicked() {
        cx.act(Action::Run(CommandId::ImportAsAudiobook));
    }
}
