//! The column browser above the track list: Genres, Artists, Albums.

use crate::panels::PanelContext;
use crate::theme;
use egui::{Color32, Rect, Sense, Ui, pos2, vec2};

pub const HEIGHT: f32 = 150.0;

/// Which browser column.
#[derive(Clone, Copy)]
enum Kind {
    Genre,
    Artist,
    Album,
}

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let rect = ui.max_rect();
    let width = rect.width() / 3.0;
    let columns = [(Kind::Genre, "Genres"), (Kind::Artist, "Artists"), (Kind::Album, "Albums")];
    for (i, (kind, title)) in columns.into_iter().enumerate() {
        let column = Rect::from_min_size(pos2(rect.left() + width * i as f32, rect.top()), vec2(width, rect.height()));
        ui.scope_builder(egui::UiBuilder::new().max_rect(column).id_salt(title), |ui| column_ui(ui, cx, kind, title));
        if i > 0 {
            ui.painter().vline(column.left(), column.y_range(), egui::Stroke::new(1.0, theme::BORDER));
        }
    }
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, theme::BORDER_DARK));
}

fn column_ui(ui: &mut Ui, cx: &mut PanelContext, kind: Kind, title: &str) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, theme::LIST_BG);
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 18.0));
    theme::vertical_gradient(ui.painter(), header, theme::HEADER_TOP, theme::HEADER_BOTTOM);
    ui.painter().hline(header.x_range(), header.bottom() - 0.5, egui::Stroke::new(1.0, theme::BORDER));
    theme::text_in_row(
        ui.painter(),
        pos2(header.left() + 8.0, header.center().y),
        title,
        theme::SMALL,
        theme::TEXT,
        header.width() - 12.0,
    );

    let values = match kind {
        Kind::Genre => cx.view.browser().genres.clone(),
        Kind::Artist => cx.view.browser().artists.clone(),
        Kind::Album => cx.view.browser().albums.clone(),
    };
    let selected = match kind {
        Kind::Genre => cx.view.browse.genre.clone(),
        Kind::Artist => cx.view.browse.artist.clone(),
        Kind::Album => cx.view.browse.album.clone(),
    };
    let all_label = format!("All ({} {})", values.len(), title);
    let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 17.0, values.len() + 1, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in range {
                let value = if row == 0 { None } else { values.get(row - 1).cloned() };
                let label = value.clone().unwrap_or_else(|| all_label.clone());
                let is_selected = value == selected;
                let (row_rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 17.0), Sense::click());
                if is_selected {
                    ui.painter().rect_filled(row_rect, 0.0, theme::ROW_SELECTED);
                }
                let color = if is_selected { Color32::WHITE } else { theme::TEXT };
                let font = if row == 0 { theme::SMALL } else { theme::BODY - 0.5 };
                theme::text_in_row(
                    ui.painter(),
                    pos2(row_rect.left() + 8.0, row_rect.center().y),
                    &label,
                    font,
                    color,
                    row_rect.width() - 12.0,
                );
                if response.clicked() {
                    select(cx, kind, value);
                }
            }
        });
    });
}

/// Choosing a genre resets the artist and album below it; choosing an artist resets the album.
fn select(cx: &mut PanelContext, kind: Kind, value: Option<String>) {
    let browse = &mut cx.view.browse;
    match kind {
        Kind::Genre => {
            browse.genre = value;
            browse.artist = None;
            browse.album = None;
        }
        Kind::Artist => {
            browse.artist = value;
            browse.album = None;
        }
        Kind::Album => browse.album = value,
    }
    cx.view.selection.clear();
}
