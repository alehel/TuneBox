//! The audiobook page: the header (`book_header`) over the book's tracks, grouped under their
//! chapters, and a strip of the common keys along the bottom.
//!
//! It works like the CD page's table: the check box means "in the book", names are edited in
//! place, and the last column says what is happening to each track. Everything else is done
//! from the keyboard (the Audiobook menu's keys) or the right-click menu.

use crate::actions::Action;
use crate::book_state::{BookRow, Renaming};
use crate::glyphs::{self, Glyph};
use crate::panels::{PanelContext, book_header, widgets};
use crate::theme;
use egui::{Align2, Color32, FontId, Key, Modifiers, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::collections::HashMap;
use tunebox_engine::JobKind;
use tunebox_engine::commands::{COMMANDS, CommandId, Context, Scope, spec_of};
use tunebox_engine::model::audiobook::{Book, ChapterView, DiscCheck, Suggestion, TrackRef, seconds};

const ROW: f32 = 26.0;
const KEY_STRIP: f32 = 28.0;
const COLUMNS: [(&str, f32); 6] =
    [("", 30.0), ("Disc", 46.0), ("#", 34.0), ("Name", 0.62), ("Time", 70.0), ("Status", 0.38)];
const CHAPTER_BG: Color32 = Color32::from_rgb(0xE9, 0xED, 0xF3);

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let Some(book) = cx.session.audiobook() else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("There is no audiobook in progress.").color(theme::TEXT_MUTED).size(16.0))
        });
        return;
    };
    let chapters = book.chapters();
    cx.view.book.keep_only(&chapters);
    let rect = ui.max_rect();
    let header_bottom = book_header::show(ui, cx, rect);
    let table = Rect::from_min_max(pos2(rect.left(), header_bottom), pos2(rect.right(), rect.bottom() - KEY_STRIP));
    arrow_keys(ui, cx, &chapters);
    let widths = resolve(table.width());
    draw_heading(ui, table, &widths);
    let body = Rect::from_min_max(pos2(table.left(), table.top() + 24.0), table.max);
    ui.painter().rect_filled(body, 0.0, theme::LIST_BG);
    let statuses = Statuses::new(cx, book);
    ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let rows = cx.view.book.rows(&chapters);
            for (index, row) in rows.iter().enumerate() {
                match row {
                    BookRow::Chapter(start) => {
                        if let Some(chapter) = chapters.iter().find(|c| c.start == *start) {
                            chapter_row(ui, cx, &chapters, chapter, &widths);
                        }
                    }
                    BookRow::Track(track) => track_row(ui, cx, book, *track, index, &widths, &statuses),
                }
            }
        });
    });
    key_strip(ui, Rect::from_min_max(pos2(rect.left(), rect.bottom() - KEY_STRIP), rect.max));
}

/// ↑ ↓ (⇧ to extend): the arrow keys the registry leaves to the page, since they move within it.
fn arrow_keys(ui: &mut Ui, cx: &mut PanelContext, chapters: &[ChapterView]) {
    if ui.ctx().egui_wants_keyboard_input() || cx.view.book.renaming.is_some() {
        return;
    }
    for (key, forward) in [(Key::ArrowUp, false), (Key::ArrowDown, true)] {
        if ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, key)) {
            cx.view.book.step(chapters, forward, true);
        } else if ui.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
            cx.view.book.step(chapters, forward, false);
        }
    }
}

fn draw_heading(ui: &Ui, table: Rect, widths: &[f32]) {
    let head = Rect::from_min_size(table.min, vec2(table.width(), 24.0));
    theme::vertical_gradient(ui.painter(), head, theme::HEADER_TOP, theme::HEADER_BOTTOM);
    ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));
    let mut x = head.left();
    for ((title, _), width) in COLUMNS.iter().zip(widths) {
        theme::text_in_row(
            ui.painter(),
            pos2(x + 8.0, head.center().y),
            title,
            theme::SMALL + 0.5,
            theme::TEXT_MUTED,
            width - 8.0,
        );
        ui.painter().vline(x + width - 0.5, head.shrink2(vec2(0.0, 5.0)).y_range(), Stroke::new(1.0, theme::GRID_LINE));
        x += width;
    }
}

/// A chapter's heading: fold triangle, name (a field while renamed), the discs it spans, its
/// length.
fn chapter_row(ui: &mut Ui, cx: &mut PanelContext, chapters: &[ChapterView], chapter: &ChapterView, widths: &[f32]) {
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    let selected = cx.view.book.is_chapter_selected(chapter);
    ui.painter().rect_filled(row, 0.0, if selected { theme::ROW_HOVER } else { CHAPTER_BG });
    ui.painter().hline(row.x_range(), row.top() + 0.5, Stroke::new(1.0, theme::GRID_LINE));
    let folded = cx.view.book.is_folded(chapter.start);
    let triangle = Rect::from_center_size(pos2(row.left() + 15.0, row.center().y), vec2(10.0, 10.0));
    let toggle = ui.interact(triangle.expand(5.0), ui.scope_id().with(("fold", chapter.start)), Sense::click());
    let points = if folded {
        vec![triangle.left_top(), triangle.right_center(), triangle.left_bottom()]
    } else {
        vec![triangle.left_top(), triangle.right_top(), triangle.center_bottom()]
    };
    ui.painter().add(egui::Shape::convex_polygon(points, theme::GLYPH, Stroke::NONE));
    if toggle.clicked() {
        cx.view.book.set_folded(chapter.start, !folded);
    }
    let name_cell = Rect::from_min_max(
        pos2(row.left() + widths[0], row.top()),
        pos2(row.left() + widths[0] + widths[1] + widths[2] + widths[3], row.bottom()),
    );
    let renaming = cx.view.book.renaming.as_ref().is_some_and(|r| r.start == chapter.start);
    if renaming {
        rename_field(ui, cx, chapters, name_cell.shrink2(vec2(6.0, 2.0)));
    } else {
        let font = if chapter.number.is_some() { crate::fonts::bold(13.0) } else { FontId::proportional(13.0) };
        let color = if chapter.number.is_some() { theme::TEXT } else { theme::TEXT_MUTED };
        let galley = ui.painter().layout_no_wrap(chapter.name.clone(), font, color);
        let name_width = galley.size().x;
        ui.painter().galley(pos2(name_cell.left() + 8.0, row.center().y - galley.size().y / 2.0), galley, color);
        let discs = match chapter.discs {
            (a, b) if a == b => format!("disc {a}"),
            (a, b) => format!("discs {a}–{b}"),
        };
        ui.painter().text(
            pos2(name_cell.left() + 18.0 + name_width, row.center().y),
            Align2::LEFT_CENTER,
            discs,
            FontId::proportional(theme::SMALL + 1.0),
            theme::TEXT_MUTED,
        );
    }
    let time_cell = Rect::from_min_size(pos2(name_cell.right(), row.top()), vec2(widths[4], ROW));
    ui.painter().text(
        time_cell.right_center() - vec2(8.0, 0.0),
        Align2::RIGHT_CENTER,
        clock(chapter.samples),
        FontId::proportional(12.0),
        theme::TEXT_MUTED,
    );
    if response.double_clicked() {
        cx.view.book.select_chapter(chapter);
        cx.view.book.renaming = Some(Renaming { start: chapter.start, text: chapter.name.clone() });
    } else if response.clicked() || response.secondary_clicked() {
        cx.view.book.select_chapter(chapter);
    }
    book_menu(&response, cx);
}

/// The chapter name being typed: Return saves, Tab saves and goes on to the next name (⇧Tab
/// the one before), Esc gives up.
fn rename_field(ui: &mut Ui, cx: &mut PanelContext, chapters: &[ChapterView], rect: Rect) {
    let tab = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
    let back = ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab));
    let enter = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    let escape = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    if escape {
        cx.view.book.renaming = None;
        return;
    }
    if tab || back {
        if let Some(saved) = cx.view.book.rename_next(chapters, tab) {
            cx.act(Action::RenameChapter(saved.start, saved.text));
        }
        return;
    }
    if enter {
        if let Some(saved) = cx.view.book.renaming.take() {
            cx.act(Action::RenameChapter(saved.start, saved.text));
        }
        return;
    }
    if let Some(renaming) = cx.view.book.renaming.as_mut() {
        let edit = egui::TextEdit::singleline(&mut renaming.text).font(crate::fonts::bold(13.0));
        let response = ui.put(rect, edit);
        response.request_focus();
    }
}

/// One track: in the book or not, where it is, its name and length, and what is happening to it.
fn track_row(
    ui: &mut Ui,
    cx: &mut PanelContext,
    book: &Book,
    track: TrackRef,
    index: usize,
    widths: &[f32],
    statuses: &Statuses,
) {
    let Some(data) = book.track(track) else { return };
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    let selected = cx.view.book.is_selected(track);
    if selected {
        theme::selection_background(ui.painter(), row);
    } else if index % 2 == 1 {
        ui.painter().rect_filled(row, 0.0, theme::ROW_ALT);
    }
    let kept = book.is_kept(track);
    let text = match (selected, kept) {
        (true, _) => Color32::WHITE,
        (false, true) => theme::TEXT,
        (false, false) => theme::GLYPH_DISABLED,
    };
    let muted = if selected { Color32::from_white_alpha(210) } else { theme::TEXT_MUTED };
    let mut x = row.left();
    let cell = |x: f32, i: usize| Rect::from_min_size(pos2(x, row.top()), vec2(widths[i], ROW));
    if let Some(keep) = widgets::row_checkbox(ui, cell(x, 0), kept) {
        cx.act(Action::SetBookTracksRemoved(vec![track], !keep));
    }
    x += widths[0];
    ui.painter().text(
        cell(x, 1).left_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        track.disc.to_string(),
        FontId::proportional(12.0),
        muted,
    );
    x += widths[1];
    let playing = cx.session.audiobook_preview().is_some_and(|p| p.track == track);
    if playing {
        let icon = Rect::from_center_size(cell(x, 2).center(), vec2(13.0, 13.0));
        glyphs::paint(ui.painter(), icon, Glyph::SpeakerLoud, if selected { Color32::WHITE } else { theme::ACCENT });
    } else {
        ui.painter().text(
            cell(x, 2).right_center() - vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            track.track.to_string(),
            FontId::proportional(12.0),
            muted,
        );
    }
    x += widths[2];
    let name = cell(x, 3);
    theme::text_in_row(
        ui.painter(),
        name.left_center() + vec2(8.0, 0.0),
        &data.title,
        theme::BODY,
        text,
        name.width() - 12.0,
    );
    if !kept {
        let width = ui.painter().layout_no_wrap(data.title.clone(), FontId::proportional(theme::BODY), text).size().x;
        let y = row.center().y;
        ui.painter().hline(
            name.left() + 8.0..=(name.left() + 8.0 + width).min(name.right() - 4.0),
            y,
            Stroke::new(1.0, text),
        );
    }
    x += widths[3];
    let trim = book.trim(track);
    let length = data.samples.saturating_sub(trim.start + trim.end);
    ui.painter().text(
        cell(x, 4).right_center() - vec2(8.0, 0.0),
        Align2::RIGHT_CENTER,
        clock(length),
        FontId::proportional(12.0),
        text,
    );
    x += widths[4];
    statuses.paint(ui, cx, book, track, cell(x, 5), selected);

    if response.double_clicked() && data.staged {
        cx.act(Action::PreviewBook { track, from: trim.start, to: data.samples - trim.end });
    } else if response.clicked() {
        let modifiers = ui.input(|i| i.modifiers);
        let chapters = book.chapters();
        if modifiers.shift {
            cx.view.book.extend_to(&chapters, track);
        } else if modifiers.command {
            cx.view.book.toggle(track);
        } else {
            cx.view.book.select_only(track);
        }
    } else if response.secondary_clicked() && !selected {
        cx.view.book.select_only(track);
    }
    book_menu(&response, cx);
}

/// The right-click menu: the Audiobook menu's commands that act on the selection.
fn book_menu(response: &egui::Response, cx: &mut PanelContext) {
    let context = Context { book_page: true, book_selection: cx.view.book.selected().len(), ..Default::default() };
    response.context_menu(|ui| {
        ui.set_min_width(230.0);
        let ids = [
            CommandId::SplitChapter,
            CommandId::JoinChapters,
            CommandId::RenameChapter,
            CommandId::RemoveFromBook,
            CommandId::TrimTrack,
            CommandId::AcceptSuggestion,
            CommandId::PlayAcrossStart,
            CommandId::PlayEnd,
        ];
        for id in ids {
            let Some(spec) = spec_of(id) else { continue };
            if spec.separator_before {
                ui.separator();
            }
            let shortcut = spec.shortcut.map(|s| s.label(cfg!(target_os = "macos"))).unwrap_or_default();
            let enabled = cx.session.is_enabled(id, context);
            if widgets::menu_item(ui, spec.label, &shortcut, false, enabled).clicked() {
                cx.act(Action::Run(id));
                ui.close();
            }
        }
    });
}

/// What each track's last column says.
struct Statuses {
    suggestions: HashMap<TrackRef, Suggestion>,
    /// The disc being ripped, its tracks being ripped in order, and how far the rip has got.
    ripping: Option<(u16, Vec<TrackRef>, f32)>,
}

impl Statuses {
    fn new(cx: &PanelContext, book: &Book) -> Self {
        let suggestions = cx.session.audiobook_suggestions().into_iter().map(|s| (s.track(), s)).collect();
        let progress =
            cx.session.jobs().into_iter().find(|j| j.kind == JobKind::RipBookDisc).map(|j| j.progress.unwrap_or(0.0));
        let ripping = progress.and_then(|p| {
            let cd = cx.session.cd()?;
            match book.check_disc(&cd.disc_id) {
                DiscCheck::InBook { disc, missing } => Some((disc, missing, p)),
                _ => None,
            }
        });
        Self { suggestions, ripping }
    }

    fn paint(&self, ui: &mut Ui, cx: &mut PanelContext, book: &Book, track: TrackRef, cell: Rect, selected: bool) {
        let Some(data) = book.track(track) else { return };
        let painter = ui.painter();
        let at = cell.left_center() + vec2(8.0, 0.0);
        let muted = if selected { Color32::from_white_alpha(210) } else { theme::TEXT_MUTED };
        let small = FontId::proportional(theme::SMALL + 1.0);
        if !data.staged {
            if let Some((disc, tracks, progress)) = &self.ripping
                && *disc == track.disc
                && let Some(position) = tracks.iter().position(|t| *t == track)
            {
                let done = progress * tracks.len() as f32 - position as f32;
                if done >= 1.0 {
                    painter.text(at, Align2::LEFT_CENTER, "✓ Ripped", small, ripped_color(selected));
                } else if done > 0.0 {
                    let bar = Rect::from_min_size(at - vec2(0.0, 3.0), vec2(110.0, 6.0));
                    painter.rect_filled(bar, 3.0, theme::GRID_LINE);
                    painter.rect_filled(
                        Rect::from_min_size(bar.min, vec2(bar.width() * done, 6.0)),
                        3.0,
                        theme::CAPACITY_AUDIO,
                    );
                    painter.text(
                        bar.right_center() + vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("Ripping {:.0}%", done * 100.0),
                        small,
                        muted,
                    );
                } else {
                    painter.text(at, Align2::LEFT_CENTER, "Waiting", small, muted);
                }
                return;
            }
            let text = data
                .failure
                .as_ref()
                .map_or_else(|| "Not ripped yet".to_owned(), |f| format!("Could not be ripped: {f}"));
            let color = if data.failure.is_some() && !selected { theme::WARNING } else { muted };
            theme::text_in_row(painter, at, &text, theme::SMALL + 1.0, color, cell.width() - 12.0);
            return;
        }
        if !book.is_kept(track) {
            painter.text(at, Align2::LEFT_CENTER, "Removed", small, muted);
            return;
        }
        let trim = book.trim(track);
        let mut x = at.x;
        if !trim.is_none() {
            let mut parts = Vec::new();
            if trim.start > 0 {
                parts.push(format!("−{:.1} s at the start", seconds(trim.start)));
            }
            if trim.end > 0 {
                parts.push(format!("−{:.1} s at the end", seconds(trim.end)));
            }
            x = pill(ui, pos2(x, at.y), &parts.join(", "), theme::ACCENT, Color32::from_rgb(0xE4, 0xEE, 0xFC)) + 8.0;
        }
        match self.suggestions.get(&track) {
            Some(suggestion) => {
                let label = match suggestion {
                    Suggestion::Remove(_) => "Looks like an announcement".to_owned(),
                    Suggestion::Trim { trim, .. } if trim.end > 0 => {
                        format!("Possible announcement in the last {:.1} s", seconds(trim.end))
                    }
                    Suggestion::Trim { trim, .. } => {
                        format!("Possible announcement in the first {:.1} s", seconds(trim.start))
                    }
                };
                let right = pill(
                    ui,
                    pos2(x, at.y),
                    &label,
                    Color32::from_rgb(0x8A, 0x54, 0x10),
                    Color32::from_rgb(0xFC, 0xEF, 0xD9),
                );
                let action = match suggestion {
                    Suggestion::Remove(_) => "Remove",
                    Suggestion::Trim { .. } => "Cut",
                };
                let link = Rect::from_min_size(pos2(right + 6.0, at.y - 8.0), vec2(44.0, 16.0));
                let response = ui.interact(link, ui.scope_id().with(("accept", track)), Sense::click());
                let color = if selected { Color32::WHITE } else { theme::ACCENT };
                ui.painter().text(link.left_center(), Align2::LEFT_CENTER, action, small, color);
                if response.on_hover_text("Accept the suggestion (A)").clicked() {
                    cx.act(Action::AcceptBookSuggestions(vec![track]));
                }
            }
            None if trim.is_none() => {
                painter.text(at, Align2::LEFT_CENTER, "✓", FontId::proportional(13.0), ripped_color(selected));
            }
            None => {}
        }
    }
}

fn ripped_color(selected: bool) -> Color32 {
    if selected { Color32::WHITE } else { theme::ICON_DISC }
}

/// A small rounded label starting at `at` (vertically centred on it); returns its right edge.
fn pill(ui: &Ui, at: egui::Pos2, text: &str, color: Color32, fill: Color32) -> f32 {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(theme::SMALL), color);
    let rect = Rect::from_min_size(pos2(at.x, at.y - 8.5), vec2(galley.size().x + 14.0, 17.0));
    ui.painter().rect_filled(rect, 8.5, fill);
    ui.painter().galley(pos2(rect.left() + 7.0, at.y - galley.size().y / 2.0), galley, color);
    rect.right()
}

/// The common keys, along the bottom.
fn key_strip(ui: &Ui, rect: Rect) {
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme::BAR_TOP);
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, theme::GRID_LINE));
    let mac = cfg!(target_os = "macos");
    let shown = [
        (CommandId::SplitChapter, "split"),
        (CommandId::JoinChapters, "join"),
        (CommandId::RenameChapter, "rename"),
        (CommandId::RemoveFromBook, "remove"),
        (CommandId::TrimTrack, "trim"),
        (CommandId::AcceptSuggestion, "accept suggestion"),
        (CommandId::PlayAcrossStart, "play across start"),
        (CommandId::PlayEnd, "play end"),
        (CommandId::FoldAllChapters, "fold all"),
        (CommandId::AudiobookShortcuts, "all shortcuts"),
    ];
    let mut x = rect.left() + 14.0;
    for (id, what) in shown {
        let Some(shortcut) =
            COMMANDS.iter().find(|c| c.id == id && c.scope == Scope::BookPage).and_then(|c| c.shortcut)
        else {
            continue;
        };
        let key = shortcut.label(mac);
        let galley = painter.layout_no_wrap(key, FontId::proportional(10.5), theme::TEXT);
        let cap = Rect::from_min_size(pos2(x, rect.center().y - 8.0), vec2(galley.size().x + 10.0, 16.0));
        if cap.right() > rect.right() - 10.0 {
            break;
        }
        painter.rect_filled(cap, 3.0, Color32::WHITE);
        painter.rect_stroke(cap, 3.0, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
        painter.galley(pos2(cap.left() + 5.0, rect.center().y - galley.size().y / 2.0), galley, theme::TEXT);
        let label = painter.layout_no_wrap(what.to_owned(), FontId::proportional(11.5), theme::TEXT_MUTED);
        let width = label.size().x;
        painter.galley(pos2(cap.right() + 5.0, rect.center().y - label.size().y / 2.0), label, theme::TEXT_MUTED);
        x = cap.right() + 5.0 + width + 18.0;
    }
}

fn clock(samples: u64) -> String {
    tunebox_engine::model::format_clock(std::time::Duration::from_secs_f64(seconds(samples)))
}

/// Fixed widths (> 1) and shares (≤ 1) as pixel widths.
fn resolve(total: f32) -> Vec<f32> {
    let fixed: f32 = COLUMNS.iter().map(|c| c.1).filter(|w| *w > 1.0).sum();
    let rest = (total - fixed).max(0.0);
    COLUMNS.iter().map(|c| if c.1 > 1.0 { c.1 } else { rest * c.1 }).collect()
}
