//! The status display in the toolbar: what is playing and where, or what TuneBox is busy with.
//! A soft, inset panel with the album art beside the text.

use crate::actions::Action;
use crate::glyphs::{self, Glyph};
use crate::panels::PanelContext;
use crate::theme;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::time::Duration;
use tunebox_engine::JobKind;
use tunebox_engine::model::format_clock;

const RADIUS: f32 = 9.0;

pub fn show(ui: &mut Ui, rect: Rect, cx: &mut PanelContext) {
    panel(ui, rect);
    // Background disc polling is not worth announcing.
    if let Some(job) = cx.session.jobs().into_iter().find(|j| j.kind != JobKind::ReadDisc) {
        let text = Rect::from_min_max(rect.min + vec2(48.0, 0.0), rect.max - vec2(48.0, 0.0));
        text_lines(ui, text, &[job.title.as_str(), job.message.as_str()]);
        progress_bar(ui, progress_rect(text), job.progress.unwrap_or(0.0));
        cancel_button(ui, rect, cx, job.id);
        return;
    }
    let Some(now) = cx.session.now_playing() else {
        idle(ui, rect);
        return;
    };
    let Some(track) = cx.session.track(now.track) else { return };

    let art = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(rect.height() - 12.0, rect.height() - 12.0));
    match cx.artwork.get(track) {
        Some(texture) => {
            egui::Image::from_texture(egui::load::SizedTexture::from_handle(&texture))
                .corner_radius(5)
                .paint_at(ui, art);
        }
        None => {
            ui.painter().rect_filled(art, 5.0, Color32::from_rgb(0xD9, 0xDC, 0xD2));
            glyphs::paint(ui.painter(), glyphs::centered(art, 18.0), Glyph::Note, theme::LCD_TEXT_MUTED);
        }
    }
    ui.painter().rect_stroke(art, 5.0, Stroke::new(1.0, Color32::from_black_alpha(30)), egui::StrokeKind::Inside);

    let text = Rect::from_min_max(pos2(art.right() + 10.0, rect.top()), rect.max - vec2(art.width() + 16.0, 0.0));
    // The second line alternates between artist and album, as on a car stereo.
    let seconds = ui.input(|i| i.time) as u64;
    let second_line = if (seconds / 4).is_multiple_of(2) || track.meta.album.is_empty() {
        &track.meta.artist
    } else {
        &track.meta.album
    };
    text_lines(ui, text, &[track.title(), second_line]);

    let bar = progress_rect(text);
    let fraction = if now.duration.is_zero() {
        0.0
    } else {
        (now.position.as_secs_f32() / now.duration.as_secs_f32()).clamp(0.0, 1.0)
    };
    progress_bar(ui, bar, fraction);
    let small = FontId::proportional(10.5);
    let painter = ui.painter();
    painter.text(
        bar.left_center() - vec2(7.0, 0.0),
        Align2::RIGHT_CENTER,
        format_clock(now.position),
        small.clone(),
        theme::LCD_TEXT_MUTED,
    );
    let remaining = now.duration.saturating_sub(now.position);
    painter.text(
        bar.right_center() + vec2(7.0, 0.0),
        Align2::LEFT_CENTER,
        format!("-{}", format_clock(remaining)),
        small,
        theme::LCD_TEXT_MUTED,
    );

    let response = ui.interact(bar.expand2(vec2(0.0, 5.0)), ui.scope_id().with("lcd-seek"), Sense::click_and_drag());
    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let fraction = ((pos.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
        cx.act(Action::Seek(Duration::from_secs_f32(now.duration.as_secs_f32() * fraction)));
    }
}

/// The inset panel: a soft gradient with a shadow along the inside of its top edge.
fn panel(ui: &Ui, rect: Rect) {
    let painter = ui.painter();
    // A one-pixel highlight below the panel makes it look pressed into the toolbar.
    painter.rect_filled(rect.translate(vec2(0.0, 1.0)), RADIUS, Color32::from_white_alpha(170));
    theme::rounded_gradient(painter, rect, RADIUS, theme::LCD_TOP, theme::LCD_BOTTOM);
    let inner_shadow = Rect::from_min_size(rect.min + vec2(2.0, 1.0), vec2(rect.width() - 4.0, 5.0));
    theme::rounded_gradient(painter, inner_shadow, 3.0, Color32::from_black_alpha(22), Color32::TRANSPARENT);
    painter.rect_stroke(
        rect,
        CornerRadius::from(RADIUS),
        Stroke::new(1.0, Color32::from_rgb(0x8E, 0x93, 0x88)),
        egui::StrokeKind::Inside,
    );
}

/// Nothing playing: the TuneBox mark, a note in a ring.
fn idle(ui: &Ui, rect: Rect) {
    let painter = ui.painter();
    let center = rect.center();
    painter.circle_stroke(center, 15.0, Stroke::new(1.5, Color32::from_rgb(0xA4, 0xA9, 0x9C)));
    glyphs::paint(
        painter,
        Rect::from_center_size(center, vec2(16.0, 16.0)),
        Glyph::Note,
        Color32::from_rgb(0x96, 0x9B, 0x8E),
    );
}

fn cancel_button(ui: &mut Ui, rect: Rect, cx: &mut PanelContext, job: tunebox_engine::JobId) {
    let cancel = Rect::from_center_size(pos2(rect.right() - 20.0, rect.center().y), vec2(16.0, 16.0));
    let response = ui.interact(cancel, ui.scope_id().with("cancel-job"), Sense::click());
    let painter = ui.painter();
    painter.circle_filled(cancel.center(), 8.0, if response.hovered() { theme::LCD_FILL } else { theme::LCD_TRACK });
    let x = Stroke::new(1.6, Color32::WHITE);
    painter.line_segment([cancel.center() + vec2(-3.0, -3.0), cancel.center() + vec2(3.0, 3.0)], x);
    painter.line_segment([cancel.center() + vec2(3.0, -3.0), cancel.center() + vec2(-3.0, 3.0)], x);
    if response.on_hover_text("Stop").clicked() {
        cx.act(Action::CancelJob(job));
    }
}

/// Two centred lines: the first a little larger.
fn text_lines(ui: &Ui, rect: Rect, lines: &[&str]) {
    let painter = ui.painter();
    let max_width = rect.width() - 80.0;
    for (i, line) in lines.iter().enumerate() {
        let (size, color) = if i == 0 { (13.0, theme::LCD_TEXT) } else { (11.5, theme::LCD_TEXT_MUTED) };
        let y = rect.top() + 14.0 + i as f32 * 15.0;
        let galley = painter.layout_job({
            let mut job =
                egui::text::LayoutJob::simple_singleline((*line).to_owned(), FontId::proportional(size), color);
            job.wrap = egui::text::TextWrapping {
                max_width,
                max_rows: 1,
                break_anywhere: true,
                overflow_character: Some('…'),
            };
            job
        });
        painter.galley(pos2(rect.center().x - galley.size().x / 2.0, y - galley.size().y / 2.0), galley, color);
    }
}

fn progress_rect(text: Rect) -> Rect {
    Rect::from_center_size(pos2(text.center().x, text.bottom() - 11.0), vec2((text.width() - 90.0).max(40.0), 4.0))
}

fn progress_bar(ui: &Ui, bar: Rect, fraction: f32) {
    let painter = ui.painter();
    painter.rect_filled(bar, 2.0, theme::LCD_TRACK);
    let filled = Rect::from_min_max(bar.min, pos2(bar.left() + bar.width() * fraction, bar.bottom()));
    if fraction > 0.0 {
        painter.rect_filled(filled, 2.0, theme::LCD_FILL);
    }
    let knob = pos2(filled.right().clamp(bar.left(), bar.right()), bar.center().y);
    painter.circle_filled(knob + vec2(0.0, 0.5), 4.5, Color32::from_black_alpha(40));
    painter.circle_filled(knob, 4.0, theme::LCD_FILL);
}
