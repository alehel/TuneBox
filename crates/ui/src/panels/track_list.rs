//! The track list: sortable columns, alternating rows, selection, double-click to play, drag
//! to a playlist, star ratings in place and a context menu.

use crate::actions::Action;
use crate::glyphs::{self, Glyph};
use crate::panels::{PanelContext, widgets};
use crate::state::SourceItem;
use crate::theme;
use egui::{Align2, Color32, FontId, Modifiers, Rect, Sense, Stroke, Ui, pos2, vec2};
use tunebox_engine::commands::CommandId;
use tunebox_engine::model::{Column, PlaylistKind, Rating, SortOrder, TrackId};

const HEADER_HEIGHT: f32 = 20.0;
const CHECK_WIDTH: f32 = 22.0;
const PLAYING_WIDTH: f32 = 18.0;

/// A column's place in the row.
struct Layout {
    column: Column,
    left: f32,
    width: f32,
}

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, theme::LIST_BG);
    let columns = cx.session.preferences().visible_columns.clone();
    let layout =
        layout(&columns, rect.left() + CHECK_WIDTH + PLAYING_WIDTH, rect.width() - CHECK_WIDTH - PLAYING_WIDTH);

    let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_HEIGHT));
    header_row(ui, header, &layout, cx);

    let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
    if cx.view.rows().is_empty() {
        empty_state(ui, body, cx);
        return;
    }
    // Zebra stripes fill the list down to the bottom, behind the rows, as in a real list.
    paint_stripes(ui, body, 0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
        let rows = cx.view.rows().to_vec();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(
            ui,
            theme::ROW_HEIGHT,
            rows.len(),
            |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for index in range {
                    row(ui, cx, &rows, index, &layout);
                }
            },
        );
    });
    keyboard(ui, cx);
}

/// Fixed widths for short columns; the rest share what is left by weight.
fn layout(columns: &[Column], left: f32, width: f32) -> Vec<Layout> {
    let fixed = |c: Column| -> Option<f32> {
        Some(match c {
            Column::Time => 52.0,
            Column::Rating => 70.0,
            Column::Plays | Column::Skips | Column::Year => 48.0,
            Column::TrackNumber => 62.0,
            Column::DiscNumber => 56.0,
            Column::BitRate => 76.0,
            Column::SampleRate => 86.0,
            Column::Size => 72.0,
            Column::Kind => 136.0,
            Column::DateAdded | Column::LastPlayed => 118.0,
            _ => return None,
        })
    };
    let weight = |c: Column| match c {
        Column::Name => 3.0,
        Column::Genre => 1.1,
        _ => 2.0,
    };
    let fixed_total: f32 = columns.iter().filter_map(|c| fixed(*c)).sum();
    let weights: f32 = columns.iter().filter(|c| fixed(**c).is_none()).map(|c| weight(*c)).sum();
    let flexible = (width - fixed_total).max(0.0);
    let mut x = left;
    columns
        .iter()
        .map(|&column| {
            let w = fixed(column).unwrap_or_else(|| (flexible * weight(column) / weights.max(1.0)).max(60.0));
            let placed = Layout { column, left: x, width: w };
            x += w;
            placed
        })
        .collect()
}

fn header_row(ui: &mut Ui, rect: Rect, layout: &[Layout], cx: &mut PanelContext) {
    let painter = ui.painter();
    theme::vertical_gradient(painter, rect, theme::HEADER_TOP, theme::HEADER_BOTTOM);
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));
    for cell in layout {
        let cell_rect = Rect::from_min_size(pos2(cell.left, rect.top()), vec2(cell.width, rect.height()));
        let sorted = cx.view.sort.filter(|(c, _)| *c == cell.column).map(|(_, o)| o);
        if sorted.is_some() {
            theme::vertical_gradient(ui.painter(), cell_rect, theme::HEADER_SORTED_TOP, theme::HEADER_SORTED_BOTTOM);
        }
        let painter = ui.painter();
        painter.vline(
            cell_rect.right() - 0.5,
            cell_rect.shrink2(vec2(0.0, 3.0)).y_range(),
            Stroke::new(1.0, theme::BORDER),
        );
        theme::text_in_row(
            painter,
            pos2(cell_rect.left() + 6.0, cell_rect.center().y),
            cell.column.title(),
            theme::SMALL,
            theme::TEXT,
            cell.width - 20.0,
        );
        if let Some(order) = sorted {
            let arrow = if order == SortOrder::Ascending { "▲" } else { "▼" };
            painter.text(
                pos2(cell_rect.right() - 9.0, cell_rect.center().y),
                Align2::CENTER_CENTER,
                arrow,
                FontId::proportional(8.0),
                theme::TEXT_MUTED,
            );
        }
        let response = ui.interact(cell_rect, ui.scope_id().with(("header", cell.column.title())), Sense::click());
        if response.clicked() {
            cx.act(Action::SortBy(cell.column));
        }
    }
}

fn row(ui: &mut Ui, cx: &mut PanelContext, rows: &[TrackId], index: usize, layout: &[Layout]) {
    let Some(&id) = rows.get(index) else { return };
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), theme::ROW_HEIGHT), Sense::click_and_drag());
    let Some(track) = cx.session.track(id) else { return };
    let selected = cx.view.selection.contains(id);
    let background = if selected {
        theme::ROW_SELECTED
    } else if index % 2 == 1 {
        theme::ROW_ALT
    } else {
        theme::LIST_BG
    };
    ui.painter().rect_filled(rect, 0.0, background);
    let text_color = if selected {
        Color32::WHITE
    } else if track.checked {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };

    let check = Rect::from_min_size(rect.min, vec2(CHECK_WIDTH, rect.height()));
    if let Some(checked) = widgets::row_checkbox(ui, check, track.checked) {
        let ids = if selected { cx.view.selected_tracks() } else { vec![id] };
        cx.act(Action::SetChecked(ids, checked));
    }
    if cx.session.now_playing().is_some_and(|n| n.track == id) {
        let icon = Rect::from_center_size(
            pos2(rect.left() + CHECK_WIDTH + PLAYING_WIDTH / 2.0, rect.center().y),
            vec2(11.0, 11.0),
        );
        glyphs::paint(ui.painter(), icon, Glyph::NowPlaying, if selected { Color32::WHITE } else { theme::ACCENT });
    }

    for cell in layout {
        let cell_rect = Rect::from_min_size(pos2(cell.left, rect.top()), vec2(cell.width, rect.height()));
        if cell.column == Column::Rating {
            let stars = Rect::from_min_size(cell_rect.min + vec2(6.0, 0.0), vec2(56.0, rect.height()));
            let interactive = selected || response.hovered();
            if let Some(rating) = widgets::rating(ui, stars, track.stats.rating, interactive, selected) {
                let ids = if selected { cx.view.selected_tracks() } else { vec![id] };
                cx.act(Action::SetRating(ids, rating));
            }
            continue;
        }
        let text = cell.column.text(track);
        let painter = ui.painter();
        if cell.column.is_numeric() {
            let galley_pos = pos2(cell_rect.right() - 8.0, cell_rect.center().y);
            painter.text(galley_pos, Align2::RIGHT_CENTER, text, FontId::proportional(theme::BODY - 0.5), text_color);
        } else {
            theme::text_in_row(
                painter,
                pos2(cell_rect.left() + 6.0, cell_rect.center().y),
                &text,
                theme::BODY - 0.5,
                text_color,
                cell.width - 10.0,
            );
        }
    }

    handle_clicks(ui, cx, &response, rows, index, id);
    context_menu(&response, cx, id);
}

fn handle_clicks(
    ui: &Ui,
    cx: &mut PanelContext,
    response: &egui::Response,
    rows: &[TrackId],
    index: usize,
    id: TrackId,
) {
    let modifiers = ui.input(|i| i.modifiers);
    if response.double_clicked() {
        cx.act(Action::PlayTracks { tracks: rows.to_vec(), start: index });
    } else if response.clicked() || (response.secondary_clicked() && !cx.view.selection.contains(id)) {
        select(cx, modifiers, rows, index, id);
    }
    if response.drag_started() {
        if !cx.view.selection.contains(id) {
            cx.view.selection.select_only(index, id);
        }
        response.dnd_set_drag_payload(cx.view.selected_tracks());
    }
}

fn select(cx: &mut PanelContext, modifiers: Modifiers, rows: &[TrackId], index: usize, id: TrackId) {
    let selection = &mut cx.view.selection;
    if modifiers.shift {
        selection.extend_to(index, rows);
    } else if modifiers.command {
        selection.toggle(index, id);
    } else {
        selection.select_only(index, id);
    }
}

fn context_menu(response: &egui::Response, cx: &mut PanelContext, id: TrackId) {
    response.context_menu(|ui| {
        let selected = cx.view.selected_tracks();
        let tracks = if selected.is_empty() { vec![id] } else { selected };
        if ui.button("Play").clicked() {
            let rows = cx.view.rows().to_vec();
            let start = rows.iter().position(|r| *r == id).unwrap_or(0);
            cx.act(Action::PlayTracks { tracks: rows, start });
            ui.close();
        }
        if ui.button("Get Info").clicked() {
            cx.act(Action::Run(CommandId::GetInfo));
            ui.close();
        }
        ui.menu_button("Rating", |ui| {
            for stars in (0..=Rating::MAX_STARS).rev() {
                let label = if stars == 0 { "None".to_owned() } else { "★".repeat(usize::from(stars)) };
                if ui.button(label).clicked() {
                    cx.act(Action::SetRating(tracks.clone(), Rating::from_stars(stars)));
                    ui.close();
                }
            }
        });
        ui.separator();
        ui.menu_button("Add to Playlist", |ui| {
            let playlists: Vec<_> = cx
                .session
                .library()
                .playlists()
                .iter()
                .filter(|p| matches!(p.kind, PlaylistKind::Manual { .. }))
                .map(|p| (p.id, p.name.clone()))
                .collect();
            for (playlist, name) in playlists {
                if ui.button(name).clicked() {
                    cx.act(Action::AddToPlaylist(playlist, tracks.clone()));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("New Playlist…").clicked() {
                cx.act(Action::NewPlaylistWith(tracks.clone()));
                ui.close();
            }
        });
        if ui.button("Show in Folder").clicked() {
            cx.act(Action::ShowInFolder(id));
            ui.close();
        }
        ui.separator();
        let label = if matches!(cx.view.source, SourceItem::Playlist(_)) {
            "Remove from Playlist"
        } else {
            "Delete from Library…"
        };
        if ui.button(label).clicked() {
            cx.act(Action::Run(CommandId::Delete));
            ui.close();
        }
    });
}

/// Arrow keys move the selection; Return plays it.
fn keyboard(ui: &Ui, cx: &mut PanelContext) {
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let (up, down, enter) = ui.input(|i| {
        (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::Enter))
    });
    let rows = cx.view.rows().to_vec();
    let current = cx.view.selection.positions(&rows).first().copied();
    if enter && let Some(start) = current {
        cx.act(Action::PlayTracks { tracks: rows, start });
        return;
    }
    let target = match (up, down, current) {
        (true, _, Some(i)) => Some(i.saturating_sub(1)),
        (_, true, Some(i)) => Some((i + 1).min(rows.len().saturating_sub(1))),
        (true, _, None) | (_, true, None) if !rows.is_empty() => Some(0),
        _ => None,
    };
    if let Some(index) = target
        && let Some(&id) = rows.get(index)
    {
        cx.view.selection.select_only(index, id);
    }
}

fn paint_stripes(ui: &Ui, rect: Rect, first_index: usize) {
    let mut y = rect.top();
    let mut index = first_index;
    while y < rect.bottom() {
        if index % 2 == 1 {
            let stripe = Rect::from_min_max(
                pos2(rect.left(), y),
                pos2(rect.right(), (y + theme::ROW_HEIGHT).min(rect.bottom())),
            );
            ui.painter().rect_filled(stripe, 0.0, theme::ROW_ALT);
        }
        y += theme::ROW_HEIGHT;
        index += 1;
    }
}

fn empty_state(ui: &mut Ui, rect: Rect, cx: &mut PanelContext) {
    paint_stripes(ui, rect, 0);
    let (title, detail) = if cx.session.library().is_empty() {
        ("Your music library is empty", "Add music from your computer, or insert a CD to import it.")
    } else if !cx.view.search.is_empty() {
        ("No matches", "No songs match your search.")
    } else {
        ("No songs", "Drag songs here from your library to add them.")
    };
    let center = rect.center() - vec2(0.0, 40.0);
    ui.painter().text(center, Align2::CENTER_CENTER, title, FontId::proportional(18.0), theme::TEXT_MUTED);
    ui.painter().text(
        center + vec2(0.0, 26.0),
        Align2::CENTER_CENTER,
        detail,
        FontId::proportional(12.0),
        theme::TEXT_MUTED,
    );
    if cx.session.library().is_empty() {
        let button = Rect::from_center_size(center + vec2(0.0, 64.0), vec2(140.0, 26.0));
        if ui.put(button, egui::Button::new("Add to Library…")).clicked() {
            cx.act(Action::Run(CommandId::AddToLibrary));
        }
    }
}
