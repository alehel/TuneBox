//! What sits over the visualizer: the playing track's details, and (while the pointer moves)
//! the controls for choosing a look, showing track info and leaving.

use crate::actions::Action;
use crate::artwork;
use crate::panels::PanelContext;
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};
use tunebox_engine::commands::CommandId;
use tunebox_engine::model::format_clock;
use tunebox_visualizer::LookEntry;

/// `looks` are in picker order; `current` is the id of the look showing.
pub fn show(ui: &mut Ui, rect: Rect, cx: &mut PanelContext, looks: &[LookEntry], current: &str, show_controls: bool) {
    let show_info = cx.session.preferences().visualizer_track_info;
    if show_info {
        track_info(ui, rect, cx);
    }
    if show_controls || cx.session.now_playing().is_none() {
        controls(ui, rect, cx, looks, current, show_info);
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        cx.act(Action::Run(CommandId::Visualizer));
    }
}

/// The card in the lower left: artwork, title, artist, album and progress.
fn track_info(ui: &mut Ui, rect: Rect, cx: &mut PanelContext) {
    let Some(now) = cx.session.now_playing() else {
        let hint = "Play something to see it here";
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            hint,
            FontId::proportional(18.0),
            Color32::from_white_alpha(120),
        );
        return;
    };
    let Some(track) = cx.session.track(now.track) else { return };
    let card = Rect::from_min_size(
        pos2(rect.left() + 28.0, rect.bottom() - 170.0),
        vec2(430.0_f32.min(rect.width() - 56.0), 128.0),
    );
    let painter = ui.painter();
    painter.rect_filled(card, 10.0, Color32::from_black_alpha(150));
    painter.rect_stroke(card, 10.0, Stroke::new(1.0, Color32::from_white_alpha(30)), egui::StrokeKind::Inside);
    let art = Rect::from_min_size(card.min + vec2(14.0, 14.0), vec2(100.0, 100.0));
    let texture = cx.artwork.cover(cx.session, track);
    artwork::paint(ui, art, texture.as_ref());

    let left = art.right() + 16.0;
    let width = card.right() - left - 14.0;
    let text = |y: f32, text: &str, size: f32, color: Color32| {
        crate::theme::text_in_row(ui.painter(), pos2(left, card.top() + y), text, size, color, width);
    };
    let title_left = pos2(left, card.top() + 28.0);
    crate::theme::font_in_row(ui.painter(), title_left, track.title(), crate::fonts::bold(19.0), Color32::WHITE, width);
    text(54.0, &track.meta.artist, 14.0, Color32::from_white_alpha(210));
    text(74.0, &track.meta.album, 13.0, Color32::from_white_alpha(150));

    let bar = Rect::from_min_size(pos2(left, card.top() + 98.0), vec2(width, 3.0));
    let fraction = if now.duration.is_zero() {
        0.0
    } else {
        (now.position.as_secs_f32() / now.duration.as_secs_f32()).clamp(0.0, 1.0)
    };
    let painter = ui.painter();
    painter.rect_filled(bar, 1.5, Color32::from_white_alpha(50));
    painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * fraction, bar.height())), 1.5, Color32::WHITE);
    let small = FontId::proportional(10.5);
    painter.text(
        bar.left_bottom() + vec2(0.0, 11.0),
        Align2::LEFT_CENTER,
        format_clock(now.position),
        small.clone(),
        Color32::from_white_alpha(170),
    );
    let remaining = format!("-{}", format_clock(now.duration.saturating_sub(now.position)));
    painter.text(
        bar.right_bottom() + vec2(0.0, 11.0),
        Align2::RIGHT_CENTER,
        remaining,
        small,
        Color32::from_white_alpha(170),
    );
}

/// The bar along the bottom: one chip per look, the Track Info switch and Done.
fn controls(ui: &mut Ui, rect: Rect, cx: &mut PanelContext, looks: &[LookEntry], current: &str, show_info: bool) {
    let chips: Vec<(String, bool)> = looks
        .iter()
        .map(|look| (look.name.clone(), look.id == current))
        .chain([("Track Info".to_owned(), show_info), ("Done".to_owned(), false)])
        .collect();
    let font = FontId::proportional(12.5);
    let widths: Vec<f32> = chips
        .iter()
        .map(|(label, _)| ui.painter().layout_no_wrap(label.clone(), font.clone(), Color32::WHITE).size().x + 26.0)
        .collect();
    let gap = 8.0;
    let first_plugin = looks.iter().position(|look| look.plugin);
    let rules = if first_plugin.is_some() { 2.0 } else { 1.0 };
    let total: f32 = widths.iter().sum::<f32>() + gap * (chips.len() as f32 + 1.0) + 7.0 + 7.0 * rules;
    let bar = Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 32.0), vec2(total, 38.0));
    ui.painter().rect_filled(bar, 19.0, Color32::from_black_alpha(170));
    let mut x = bar.left() + gap;
    for (index, ((label, on), width)) in chips.iter().zip(&widths).enumerate() {
        // A rule between the built-in looks and the plug-ins, and between the looks and the switches.
        if index == looks.len() || Some(index) == first_plugin {
            x += 7.0;
            ui.painter().vline(
                x - 4.0 - gap / 2.0,
                bar.shrink(9.0).y_range(),
                Stroke::new(1.0, Color32::from_white_alpha(60)),
            );
        }
        let chip = Rect::from_min_size(pos2(x, bar.top() + 6.0), vec2(*width, bar.height() - 12.0));
        let mut response = ui.interact(chip, ui.scope_id().with(("visualizer-chip", index)), Sense::click());
        if looks.get(index).is_some_and(|look| look.plugin) {
            response = response.on_hover_text("A plug-in from the Visualizers folder");
        }
        let fill = if *on {
            Color32::from_white_alpha(230)
        } else if response.hovered() {
            Color32::from_white_alpha(50)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(chip, 13.0, fill);
        let color = if *on { Color32::BLACK } else { Color32::WHITE };
        ui.painter().text(chip.center(), Align2::CENTER_CENTER, label, font.clone(), color);
        if response.clicked() {
            choose(cx, looks, index);
        }
        x += width + gap;
    }
}

/// A chip was clicked: a look, then Track Info, then Done.
fn choose(cx: &mut PanelContext, looks: &[LookEntry], index: usize) {
    let mut prefs = cx.session.preferences().clone();
    match looks.get(index) {
        Some(look) => prefs.visualizer_style = look.id.clone(),
        None if index == looks.len() => prefs.visualizer_track_info = !prefs.visualizer_track_info,
        None => {
            cx.act(Action::Run(CommandId::Visualizer));
            return;
        }
    }
    cx.act(Action::SetPreferences(Box::new(prefs)));
}
