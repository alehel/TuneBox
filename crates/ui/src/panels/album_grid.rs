//! The album grid: one tile per album. Covers sit on soft shadows with a faint reflection
//! below, a nod to the classic Cover Flow; hovering lifts a cover and offers to play it.

use crate::actions::Action;
use crate::artwork::{self, AlbumKey, album_key};
use crate::glyphs::{self, Glyph};
use crate::panels::PanelContext;
use crate::theme;
use egui::epaint::{Mesh, Vertex};
use egui::{Color32, FontId, Rect, Sense, Stroke, TextureHandle, Ui, pos2, vec2};
use tunebox_engine::model::TrackId;

const TILE: f32 = 164.0;
const REFLECTION: f32 = 26.0;
const CAPTION: f32 = 40.0;
const GAP: f32 = 28.0;
const RADIUS: f32 = 6.0;

/// An album and its tracks, in list order.
struct Album {
    key: AlbumKey,
    tracks: Vec<TrackId>,
}

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let rect = ui.max_rect();
    theme::vertical_gradient(ui.painter(), rect, theme::LIST_BG, Color32::from_rgb(0xF4, 0xF5, 0xF7));
    let albums = group(cx);
    let per_row = (((rect.width() - GAP) / (TILE + GAP)).floor() as usize).max(1);
    let row_count = albums.len().div_ceil(per_row);
    let margin = (rect.width() - per_row as f32 * (TILE + GAP) + GAP) / 2.0;
    let row_height = TILE + REFLECTION + CAPTION + GAP;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_height, row_count, |ui, range| {
        for row in range {
            let (row_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::hover());
            for column in 0..per_row {
                let Some(album) = albums.get(row * per_row + column) else { break };
                let left = row_rect.left() + margin + column as f32 * (TILE + GAP);
                tile(ui, cx, album, Rect::from_min_size(pos2(left, row_rect.top() + GAP), vec2(TILE, TILE)));
            }
        }
    });
}

fn group(cx: &PanelContext) -> Vec<Album> {
    let mut albums: Vec<Album> = Vec::new();
    for id in cx.view.rows() {
        let Some(track) = cx.session.track(*id) else { continue };
        let key = album_key(track);
        match albums.iter_mut().find(|a| a.key == key) {
            Some(album) => album.tracks.push(*id),
            None => albums.push(Album { key, tracks: vec![*id] }),
        }
    }
    albums
}

fn tile(ui: &mut Ui, cx: &mut PanelContext, album: &Album, slot: Rect) {
    let whole = Rect::from_min_max(slot.min, slot.max + vec2(0.0, REFLECTION + CAPTION));
    let response = ui.interact(whole, ui.scope_id().with(("album", &album.key)), Sense::click());
    let hovered = response.hovered();
    let selected = album.tracks.iter().all(|id| cx.view.selection.contains(*id));
    let texture = album.tracks.first().and_then(|id| cx.session.track(*id)).and_then(|t| cx.artwork.get(t));
    let art = if hovered { slot.translate(vec2(0.0, -3.0)) } else { slot };

    theme::soft_shadow(ui.painter(), art, RADIUS, if hovered { 18 } else { 10 });
    match &texture {
        Some(texture) => {
            egui::Image::from_texture(egui::load::SizedTexture::from_handle(texture))
                .corner_radius(RADIUS as u8)
                .paint_at(ui, art);
            reflection(ui, texture, slot);
        }
        None => artwork::paint_placeholder(ui.painter(), art),
    }
    let painter = ui.painter();
    painter.rect_stroke(art, RADIUS, Stroke::new(1.0, Color32::from_black_alpha(25)), egui::StrokeKind::Inside);
    if selected {
        painter.rect_stroke(art.expand(3.5), RADIUS + 3.0, Stroke::new(3.0, theme::ACCENT), egui::StrokeKind::Middle);
    }

    let (artist, title) = &album.key;
    let title = if title.is_empty() { "Unknown Album" } else { title };
    let caption_top = slot.bottom() + REFLECTION - 6.0;
    let center_x = slot.center().x;
    centered_text(painter, pos2(center_x, caption_top + 8.0), title, 12.5, theme::TEXT, TILE);
    centered_text(painter, pos2(center_x, caption_top + 24.0), artist, 11.0, theme::TEXT_MUTED, TILE);

    let play_clicked = hovered && play_button(ui, art, album);
    if play_clicked || response.double_clicked() {
        cx.act(Action::PlayTracks { tracks: album.tracks.clone(), start: 0 });
    } else if response.clicked() {
        cx.view.selection.set(album.tracks.iter().copied());
    }
}

/// The cover mirrored below itself, fading out: the bottom of the picture is nearest the floor.
fn reflection(ui: &Ui, texture: &TextureHandle, art: Rect) {
    let top = art.bottom() + 2.0;
    let fraction = REFLECTION / art.height();
    let near = Color32::from_white_alpha(60);
    let far = Color32::TRANSPARENT;
    let mut mesh = Mesh::with_texture(texture.id());
    let corners = [
        (pos2(art.left(), top), pos2(0.0, 1.0), near),
        (pos2(art.right(), top), pos2(1.0, 1.0), near),
        (pos2(art.right(), top + REFLECTION), pos2(1.0, 1.0 - fraction), far),
        (pos2(art.left(), top + REFLECTION), pos2(0.0, 1.0 - fraction), far),
    ];
    for (pos, uv, color) in corners {
        mesh.vertices.push(Vertex { pos, uv, color });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
}

/// A round play button in the corner of a hovered cover. Returns whether it was clicked.
fn play_button(ui: &mut Ui, art: Rect, album: &Album) -> bool {
    let button = Rect::from_center_size(art.left_bottom() + vec2(24.0, -24.0), vec2(32.0, 32.0));
    let response = ui.interact(button, ui.scope_id().with(("play-album", &album.key)), Sense::click());
    let painter = ui.painter();
    let fill = if response.hovered() { Color32::from_black_alpha(200) } else { Color32::from_black_alpha(150) };
    painter.circle_filled(button.center(), 16.0, fill);
    painter.circle_stroke(button.center(), 16.0, Stroke::new(1.0, Color32::from_white_alpha(90)));
    glyphs::paint(painter, glyphs::centered(button.translate(vec2(1.5, 0.0)), 13.0), Glyph::Play, Color32::WHITE);
    response.on_hover_text("Play album").clicked()
}

fn centered_text(painter: &egui::Painter, center: egui::Pos2, text: &str, size: f32, color: Color32, max_width: f32) {
    let galley = painter.layout_job({
        let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), FontId::proportional(size), color);
        job.wrap =
            egui::text::TextWrapping { max_width, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
        job
    });
    painter.galley(center - galley.size() / 2.0, galley, color);
}
