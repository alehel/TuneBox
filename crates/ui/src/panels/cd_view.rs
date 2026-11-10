//! The audio CD page: the disc's tracks, their names, and importing them.

use crate::actions::Action;
use crate::glyphs::{self, Glyph};
use crate::panels::{PanelContext, widgets};
use crate::theme;
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use tunebox_engine::JobKind;
use tunebox_engine::commands::CommandId;
use tunebox_engine::model::format_clock;

const HEADER_HEIGHT: f32 = 118.0;

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let Some(cd) = cx.session.cd() else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("There is no disc in the drive.").color(theme::TEXT_MUTED).size(16.0))
        });
        return;
    };
    let mut info = cd.info.clone();
    let numbers = cd.audio_numbers();
    let include = cd.include.clone();
    let imported = cd.imported.clone();
    let toc = cd.toc.clone();
    let rect = ui.max_rect();

    // Header: the disc and its names.
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_HEIGHT));
    theme::vertical_gradient(
        ui.painter(),
        header,
        Color32::from_rgb(0xF6, 0xF6, 0xF6),
        Color32::from_rgb(0xDE, 0xDE, 0xDE),
    );
    ui.painter().hline(header.x_range(), header.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));
    glyphs::paint(
        ui.painter(),
        Rect::from_min_size(header.min + vec2(20.0, 17.0), vec2(84.0, 84.0)),
        Glyph::Disc,
        Color32::from_rgb(0x9A, 0xA4, 0xB2),
    );
    let mut changed = false;
    let fields =
        Rect::from_min_max(header.min + vec2(124.0, 14.0), pos2(header.right() - 210.0, header.bottom() - 10.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(fields), |ui| {
        ui.spacing_mut().item_spacing.y = 7.0;
        let wide = (fields.width() - 70.0).clamp(160.0, 420.0);
        changed |= field(ui, "Album", &mut info.album, wide);
        changed |= field(ui, "Artist", &mut info.artist, wide);
        ui.horizontal(|ui| {
            ui.add_sized(vec2(LABEL_WIDTH, 20.0), egui::Label::new(RichText::new("Genre").color(theme::TEXT_MUTED)));
            changed |= ui.add(egui::TextEdit::singleline(&mut info.genre).desired_width(150.0)).changed();
            ui.label(RichText::new("Year").color(theme::TEXT_MUTED));
            let mut year = info.year.map(|y| y.to_string()).unwrap_or_default();
            if ui.add(egui::TextEdit::singleline(&mut year).desired_width(50.0)).changed() {
                info.year = year.trim().parse().ok();
                changed = true;
            }
            changed |= ui.checkbox(&mut info.compilation, "Compilation").changed();
        });
    });

    let buttons = Rect::from_min_max(
        pos2(header.right() - 196.0, header.top() + 18.0),
        pos2(header.right() - 16.0, header.bottom() - 14.0),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(buttons).layout(egui::Layout::top_down(egui::Align::Max)), |ui| {
        let busy = cx.session.is_busy(JobKind::Rip);
        let import = egui::Button::new(RichText::new(if busy { "Importing…" } else { "Import CD" }).strong())
            .min_size(vec2(150.0, 28.0));
        if ui.add_enabled(!busy, import).clicked() {
            cx.act(Action::Run(CommandId::ImportCd));
        }
        let looking = cx.session.is_busy(JobKind::LookupNames);
        if ui.add_enabled(!looking, egui::Button::new("Get Track Names").min_size(vec2(150.0, 24.0))).clicked() {
            cx.act(Action::Run(CommandId::GetTrackNames));
        }
        let format = cx.session.preferences().rip_format.encoder_name();
        ui.label(RichText::new(format!("Imports with the {format}")).size(10.5).color(theme::TEXT_MUTED));
    });

    // Tracks.
    let table = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
    ui.painter().rect_filled(table, 0.0, theme::LIST_BG);
    let columns = [("", 26.0), ("#", 34.0), ("Name", 0.5), ("Time", 60.0), ("Artist", 0.5), ("", 40.0)];
    let widths = resolve(&columns, table.width());
    let head = Rect::from_min_size(table.min, vec2(table.width(), 20.0));
    theme::vertical_gradient(ui.painter(), head, theme::HEADER_TOP, theme::HEADER_BOTTOM);
    ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));
    let mut x = head.left();
    for ((title, _), width) in columns.iter().zip(&widths) {
        theme::text_in_row(ui.painter(), pos2(x + 6.0, head.center().y), title, theme::SMALL, theme::TEXT, width - 8.0);
        ui.painter().vline(x + width - 0.5, head.shrink2(vec2(0.0, 3.0)).y_range(), Stroke::new(1.0, theme::BORDER));
        x += width;
    }
    let body = Rect::from_min_max(pos2(table.left(), head.bottom()), table.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, number) in numbers.iter().enumerate() {
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
                if index % 2 == 1 {
                    ui.painter().rect_filled(row, 0.0, theme::ROW_ALT);
                }
                let mut x = row.left();
                let cell = |x: f32, i: usize| Rect::from_min_size(pos2(x, row.top()), vec2(widths[i], row.height()));
                if let Some(value) = widgets::row_checkbox(ui, cell(x, 0), include.get(index).copied().unwrap_or(true))
                {
                    cx.act(Action::SetCdTrackIncluded(index, value));
                }
                x += widths[0];
                ui.painter().text(
                    cell(x, 1).right_center() - vec2(8.0, 0.0),
                    Align2::RIGHT_CENTER,
                    number.to_string(),
                    FontId::proportional(12.0),
                    theme::TEXT,
                );
                x += widths[1];
                if let Some((title, artist)) = info.tracks.get_mut(index) {
                    changed |= ui
                        .put(
                            cell(x, 2).shrink2(vec2(4.0, 3.0)),
                            egui::TextEdit::singleline(title).frame(egui::Frame::NONE),
                        )
                        .changed();
                    x += widths[2];
                    let time = toc.duration(*number).map(format_clock).unwrap_or_default();
                    ui.painter().text(
                        cell(x, 3).right_center() - vec2(8.0, 0.0),
                        Align2::RIGHT_CENTER,
                        time,
                        FontId::proportional(12.0),
                        theme::TEXT,
                    );
                    x += widths[3];
                    let hint =
                        egui::TextEdit::singleline(artist).hint_text(info.artist.clone()).frame(egui::Frame::NONE);
                    changed |= ui.put(cell(x, 4).shrink2(vec2(4.0, 3.0)), hint).changed();
                    x += widths[4];
                }
                if imported.contains(number) {
                    let check = Rect::from_center_size(cell(x, 5).center(), vec2(12.0, 12.0));
                    ui.painter().circle_filled(check.center(), 7.0, Color32::from_rgb(0x3E, 0xA0, 0x4B));
                    let stroke = Stroke::new(1.8, Color32::WHITE);
                    ui.painter()
                        .line_segment([check.center() + vec2(-3.5, 0.0), check.center() + vec2(-1.0, 2.5)], stroke);
                    ui.painter()
                        .line_segment([check.center() + vec2(-1.0, 2.5), check.center() + vec2(3.5, -2.5)], stroke);
                }
            }
        });
    });
    if changed {
        cx.act(Action::SetDiscInfo(Box::new(info)));
    }
}

const LABEL_WIDTH: f32 = 50.0;

fn field(ui: &mut Ui, label: &str, value: &mut String, width: f32) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized(vec2(LABEL_WIDTH, 20.0), egui::Label::new(RichText::new(label).color(theme::TEXT_MUTED)));
        ui.add(egui::TextEdit::singleline(value).desired_width(width)).changed()
    })
    .inner
}

/// Turns fixed widths (> 1) and shares (≤ 1) into pixel widths.
fn resolve(columns: &[(&str, f32)], total: f32) -> Vec<f32> {
    let fixed: f32 = columns.iter().map(|c| c.1).filter(|w| *w > 1.0).sum();
    let rest = (total - fixed).max(0.0);
    columns.iter().map(|c| if c.1 > 1.0 { c.1 } else { rest * c.1 }).collect()
}
