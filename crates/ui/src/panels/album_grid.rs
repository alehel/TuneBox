//! The album grid: one tile per album, with its artwork.

use crate::actions::Action;
use crate::artwork::{self, AlbumKey, album_key};
use crate::panels::PanelContext;
use crate::theme;
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};
use tunebox_engine::model::TrackId;

const TILE: f32 = 150.0;
const CAPTION: f32 = 36.0;
const GAP: f32 = 22.0;

/// An album and its tracks, in list order.
struct Album {
    key: AlbumKey,
    tracks: Vec<TrackId>,
}

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, theme::LIST_BG);
    let albums = group(cx);
    let per_row = (((rect.width() - GAP) / (TILE + GAP)).floor() as usize).max(1);
    let row_count = albums.len().div_ceil(per_row);
    let margin = (rect.width() - per_row as f32 * (TILE + GAP) + GAP) / 2.0;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(
        ui,
        TILE + CAPTION + GAP,
        row_count,
        |ui, range| {
            for row in range {
                let (row_rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), TILE + CAPTION + GAP), Sense::hover());
                for column in 0..per_row {
                    let Some(album) = albums.get(row * per_row + column) else { break };
                    let left = row_rect.left() + margin + column as f32 * (TILE + GAP);
                    tile(ui, cx, album, Rect::from_min_size(pos2(left, row_rect.top() + GAP), vec2(TILE, TILE)));
                }
            }
        },
    );
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

fn tile(ui: &mut Ui, cx: &mut PanelContext, album: &Album, art: Rect) {
    let whole = Rect::from_min_max(art.min, art.max + vec2(0.0, CAPTION));
    let response = ui.interact(whole, ui.scope_id().with(("album", &album.key)), Sense::click());
    let selected = album.tracks.iter().all(|id| cx.view.selection.contains(*id));
    let texture = album.tracks.first().and_then(|id| cx.session.track(*id)).and_then(|t| cx.artwork.get(t));

    let painter = ui.painter();
    painter.rect_filled(art.translate(vec2(0.0, 2.0)).expand(1.0), 2.0, Color32::from_black_alpha(40));
    artwork::paint(painter, art, texture.as_ref());
    if selected {
        painter.rect_stroke(art.expand(3.0), 3.0, Stroke::new(3.0, theme::ROW_SELECTED), egui::StrokeKind::Middle);
    }
    let (artist, title) = &album.key;
    let title = if title.is_empty() { "Unknown Album" } else { title };
    painter.text(
        pos2(art.center().x, art.bottom() + 12.0),
        Align2::CENTER_CENTER,
        elide(title, 24),
        FontId::proportional(12.0),
        theme::TEXT,
    );
    painter.text(
        pos2(art.center().x, art.bottom() + 27.0),
        Align2::CENTER_CENTER,
        elide(artist, 28),
        FontId::proportional(11.0),
        theme::TEXT_MUTED,
    );

    if response.double_clicked() {
        cx.act(Action::PlayTracks { tracks: album.tracks.clone(), start: 0 });
    } else if response.clicked() {
        cx.view.selection.set(album.tracks.iter().copied());
    }
}

fn elide(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let mut cut: String = text.chars().take(max_chars - 1).collect();
        cut.push('…');
        cut
    }
}
