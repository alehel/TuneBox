//! The status display in the toolbar: what is playing and where, or what TuneBox is busy with.

use crate::actions::Action;
use crate::glyphs::{self, Glyph};
use crate::panels::PanelContext;
use crate::theme;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, Ui, vec2};
use std::time::Duration;
use tunebox_engine::model::format_clock;

pub fn show(ui: &mut Ui, rect: Rect, cx: &mut PanelContext) {
    let painter = ui.painter_at(rect.expand(1.0));
    theme::vertical_gradient(&painter, rect.shrink(1.0), theme::LCD_TOP, theme::LCD_BOTTOM);
    painter.rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0, theme::BORDER_DARK), egui::StrokeKind::Inside);
    painter.hline(rect.shrink(2.0).x_range(), rect.top() + 1.5, Stroke::new(1.0, Color32::from_white_alpha(200)));

    // Background disc polling is not worth announcing.
    if let Some(job) = cx.session.jobs().into_iter().find(|j| j.kind != tunebox_engine::JobKind::ReadDisc) {
        let lines = [job.title.as_str(), job.message.as_str()];
        text_lines(&painter, rect, &lines);
        let bar = progress_rect(rect);
        progress_bar(&painter, bar, job.progress.unwrap_or(0.0));
        let cancel = Rect::from_center_size(egui::pos2(rect.right() - 16.0, rect.center().y), vec2(14.0, 14.0));
        let response = ui.interact(cancel, ui.scope_id().with("cancel-job"), Sense::click());
        painter.circle_filled(cancel.center(), 7.0, if response.hovered() { theme::GLYPH } else { theme::LCD_TRACK });
        let x = Stroke::new(1.6, Color32::WHITE);
        painter.line_segment([cancel.center() + vec2(-3.0, -3.0), cancel.center() + vec2(3.0, 3.0)], x);
        painter.line_segment([cancel.center() + vec2(3.0, -3.0), cancel.center() + vec2(-3.0, 3.0)], x);
        if response.on_hover_text("Stop").clicked() {
            cx.act(Action::CancelJob(job.id));
        }
        return;
    }

    let Some(now) = cx.session.now_playing() else {
        glyphs::paint(&painter, glyphs::centered(rect, 26.0), Glyph::Note, Color32::from_rgb(0x8F, 0x93, 0x88));
        return;
    };
    let Some(track) = cx.session.track(now.track) else { return };
    // The second line alternates between artist and album, as on a car stereo.
    let seconds = ui.input(|i| i.time) as u64;
    let second_line = if (seconds / 4).is_multiple_of(2) || track.meta.album.is_empty() {
        &track.meta.artist
    } else {
        &track.meta.album
    };
    text_lines(&painter, rect, &[track.title(), second_line]);

    let bar = progress_rect(rect);
    let fraction = if now.duration.is_zero() {
        0.0
    } else {
        (now.position.as_secs_f32() / now.duration.as_secs_f32()).clamp(0.0, 1.0)
    };
    progress_bar(&painter, bar, fraction);
    let small = FontId::proportional(10.5);
    painter.text(
        bar.left_center() - vec2(6.0, 0.0),
        Align2::RIGHT_CENTER,
        format_clock(now.position),
        small.clone(),
        theme::LCD_TEXT,
    );
    let remaining = now.duration.saturating_sub(now.position);
    painter.text(
        bar.right_center() + vec2(6.0, 0.0),
        Align2::LEFT_CENTER,
        format!("-{}", format_clock(remaining)),
        small,
        theme::LCD_TEXT,
    );

    let response = ui.interact(bar.expand2(vec2(0.0, 4.0)), ui.scope_id().with("lcd-seek"), Sense::click_and_drag());
    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let fraction = ((pos.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
        cx.act(Action::Seek(Duration::from_secs_f32(now.duration.as_secs_f32() * fraction)));
    }
}

fn text_lines(painter: &egui::Painter, rect: Rect, lines: &[&str]) {
    let max_width = rect.width() - 120.0;
    for (i, line) in lines.iter().enumerate() {
        let size = if i == 0 { 12.5 } else { 11.5 };
        let y = rect.top() + 12.0 + i as f32 * 14.0;
        let galley = painter.layout_job({
            let mut job = egui::text::LayoutJob::simple_singleline(
                (*line).to_owned(),
                FontId::proportional(size),
                theme::LCD_TEXT,
            );
            job.wrap = egui::text::TextWrapping {
                max_width,
                max_rows: 1,
                break_anywhere: true,
                overflow_character: Some('…'),
            };
            job
        });
        painter.galley(
            egui::pos2(rect.center().x - galley.size().x / 2.0, y - galley.size().y / 2.0),
            galley,
            theme::LCD_TEXT,
        );
    }
}

fn progress_rect(rect: Rect) -> Rect {
    Rect::from_center_size(
        egui::pos2(rect.center().x, rect.bottom() - 10.0),
        vec2((rect.width() - 120.0).max(40.0), 7.0),
    )
}

fn progress_bar(painter: &egui::Painter, bar: Rect, fraction: f32) {
    painter.rect_filled(bar, 3.5, theme::LCD_TOP);
    painter.rect_stroke(bar, 3.5, Stroke::new(1.0, theme::LCD_TRACK), egui::StrokeKind::Inside);
    let filled = Rect::from_min_max(bar.min, egui::pos2(bar.left() + bar.width() * fraction, bar.bottom()));
    if fraction > 0.0 {
        painter.rect_filled(filled.shrink(1.5), 2.0, theme::LCD_FILL);
    }
    let knob = egui::pos2(filled.right().clamp(bar.left() + 3.5, bar.right() - 3.5), bar.center().y);
    painter.add(egui::Shape::convex_polygon(
        vec![knob + vec2(0.0, -5.0), knob + vec2(4.0, 0.0), knob + vec2(0.0, 5.0), knob + vec2(-4.0, 0.0)],
        theme::LCD_FILL,
        Stroke::NONE,
    ));
}
