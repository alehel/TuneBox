//! Trim…: cutting a stretch off the start or end of a ripped track ("End of disc one. Please
//! insert disc two.").
//!
//! The first and last 20 seconds are drawn as waveforms, with the track's pauses shaded and a
//! handle for each cut. A handle dragged near a pause snaps to its middle, so a cut never clips
//! a word; Alt (Option) places it anywhere. Keys: Tab picks the cut, ← → move it 0.1 s (1 s with
//! Shift), ⌥← ⌥→ go to the previous or next pause, P plays across the cut, X plays what is cut,
//! 0 takes the cut away, Return trims, Esc cancels.

use crate::actions::Action;
use crate::dialogs::{layout, modal, title_heading};
use crate::panels::widgets;
use crate::theme;
use egui::{Align2, Color32, FontId, Key, Modifiers, Rect, Sense, Stroke, Ui, pos2, vec2};
use tunebox_engine::Session;
use tunebox_engine::model::audiobook::{Pause, Suggestion, TrackRef, Trim, samples, seconds};

/// How much of each end is shown.
const WINDOW_SECONDS: f64 = 20.0;
/// How far a handle reaches for a pause to snap to, in points.
const SNAP_POINTS: f32 = 12.0;
/// Columns of each waveform.
const COLUMNS: usize = 360;
/// What Play from Cut and Play to Cut play.
const LISTEN_SECONDS: f64 = 4.0;

/// Which cut the keyboard moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cut {
    Start,
    End,
}

#[derive(Clone, Debug)]
pub struct TrimDialog {
    track: TrackRef,
    title: String,
    length: u64,
    /// Samples cut off the start and the end.
    start: u64,
    end: u64,
    pauses: Vec<Pause>,
    /// The suggested cut, if any, to label it.
    proposed: Option<Trim>,
    start_wave: Vec<f32>,
    end_wave: Vec<f32>,
    focus: Cut,
    error: Option<String>,
}

impl TrimDialog {
    /// The editor for `track`, opened on its current trim, or the suggested one if it has none.
    /// `None` when the track is not ripped.
    pub fn new(session: &Session, track: TrackRef) -> Option<Self> {
        let book = session.audiobook()?;
        let data = book.track(track).filter(|t| t.staged)?;
        let length = data.samples;
        let proposed = session.audiobook_suggestions().into_iter().find_map(|s| match s {
            Suggestion::Trim { track: t, trim } if t == track => Some(trim),
            _ => None,
        });
        let current = book.trim(track);
        let trim = if current.is_none() { proposed.unwrap_or_default() } else { current };
        let window = samples(WINDOW_SECONDS).min(length);
        let wave = |from, to| session.audiobook_waveform(track, from, to, COLUMNS).unwrap_or_default();
        let focus = if trim.end > 0 || trim.start == 0 { Cut::End } else { Cut::Start };
        Some(Self {
            track,
            title: data.title.clone(),
            length,
            start: trim.start,
            end: trim.end,
            pauses: data.pauses.clone(),
            proposed,
            start_wave: wave(0, window),
            end_wave: wave(length - window, length),
            focus,
            error: None,
        })
    }

    pub fn show(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let mut result = None;
        self.keys(ctx, actions, &mut result);
        modal(ctx, "trim", |ui| {
            ui.set_width(760.0);
            title_heading(ui, &format!("Trim “{}”", self.title));
            ui.label(layout::muted(&format!(
                "Track {} · {}. Drag a handle to cut. Handles snap to quiet moments; hold Alt to place one anywhere.",
                self.track,
                clock(self.length)
            )));
            ui.add_space(8.0);
            self.side(ui, Cut::Start, actions);
            ui.add_space(14.0);
            self.side(ui, Cut::End, actions);
            ui.add_space(10.0);
            let left = self.length.saturating_sub(self.start + self.end);
            ui.label(format!("New length {} (was {}).", clock(left), clock(self.length)));
            let (option, shift) = if cfg!(target_os = "macos") { ("⌥", "⇧") } else { ("Alt+", "Shift+") };
            ui.label(layout::muted(&format!(
                "Tab: start or end cut · ← → 0.1 s ({shift}→ 1 s) · {option}← {option}→ previous, next pause · P play \
                 across the cut · X play what is cut · 0 no cut"
            )));
            if let Some(error) = &self.error {
                ui.colored_label(theme::WARNING, error);
            }
            ui.add_space(12.0);
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 28.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if widgets::primary_button(ui, "Trim", true, vec2(88.0, 28.0)).clicked() {
                        result = Some(true);
                    }
                    if ui.add(egui::Button::new("Cancel").min_size(vec2(88.0, 28.0))).clicked() {
                        result = Some(false);
                    }
                    if ui.add(egui::Button::new("Remove Trim").min_size(vec2(110.0, 28.0))).clicked() {
                        self.start = 0;
                        self.end = 0;
                        result = Some(true);
                    }
                },
            );
        });
        match result {
            Some(true) => {
                if self.start + self.end >= self.length {
                    self.error =
                        Some("That would cut the whole track. To leave it out, remove it from the book.".into());
                    return true;
                }
                actions.push(Action::StopBookPreview);
                actions.push(Action::TrimBookTrack(self.track, Trim { start: self.start, end: self.end }));
                false
            }
            Some(false) => {
                actions.push(Action::StopBookPreview);
                false
            }
            None => true,
        }
    }

    /// One end of the track: its heading, waveform with handle, and controls.
    fn side(&mut self, ui: &mut Ui, cut: Cut, actions: &mut Vec<Action>) {
        let window = samples(WINDOW_SECONDS).min(self.length);
        let (offset, heading) = match cut {
            Cut::Start => (0, "Start · first 20 seconds"),
            Cut::End => (self.length - window, "End · last 20 seconds"),
        };
        let focused = self.focus == cut;
        let heading = if focused { format!("{heading}   (the keys move this cut)") } else { heading.to_owned() };
        ui.label(egui::RichText::new(heading).strong());
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 104.0), Sense::click_and_drag());
        let span = window.max(1) as f32;
        let x_of = |sample: u64| rect.left() + (sample.saturating_sub(offset)) as f32 / span * rect.width();
        let sample_of = |x: f32| offset + (((x - rect.left()) / rect.width()).clamp(0.0, 1.0) * span) as u64;
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::WHITE);
        // Pauses, shaded.
        for pause in self.pauses.iter().filter(|p| p.end > offset && p.start < offset + window) {
            let shade = Rect::from_x_y_ranges(
                x_of(pause.start.max(offset))..=x_of(pause.end.min(offset + window)),
                rect.y_range(),
            );
            painter.rect_filled(shade, 0.0, Color32::from_rgb(0xEA, 0xF4, 0xE8));
        }
        // The waveform, scaled to the loudest moment shown.
        let wave = if cut == Cut::Start { &self.start_wave } else { &self.end_wave };
        let loudest = wave.iter().fold(0.05f32, |m, p| m.max(*p));
        let position = match cut {
            Cut::Start => self.start,
            Cut::End => self.length - self.end,
        };
        // Kept inside the frame, so a cut of nothing still shows its handle.
        let cut_x = x_of(position).clamp(rect.left() + 1.0, rect.right() - 1.0);
        for (i, peak) in wave.iter().enumerate() {
            let x = rect.left() + (i as f32 + 0.5) / wave.len().max(1) as f32 * rect.width();
            let height = (peak / loudest * rect.height() * 0.42).max(0.5);
            let away = match cut {
                Cut::Start => x < cut_x,
                Cut::End => x > cut_x,
            };
            let color = if away { Color32::from_rgb(0xC8, 0xCB, 0xD0) } else { theme::CAPACITY_AUDIO };
            painter.line_segment(
                [pos2(x, rect.center().y - height), pos2(x, rect.center().y + height)],
                Stroke::new(1.4, color),
            );
        }
        // Seconds along the bottom.
        for s in (0..=WINDOW_SECONDS as u64).step_by(5) {
            let label = match cut {
                Cut::Start => format!("{s} s"),
                Cut::End => format!("−{} s", WINDOW_SECONDS as u64 - s),
            };
            let x = rect.left() + s as f32 / WINDOW_SECONDS as f32 * rect.width();
            painter.text(
                pos2(x + 3.0, rect.bottom() - 3.0),
                Align2::LEFT_BOTTOM,
                label,
                FontId::proportional(10.0),
                theme::TEXT_MUTED,
            );
        }
        // The cut: a shaded stretch and a handle.
        let gone = match cut {
            Cut::Start => Rect::from_x_y_ranges(rect.left()..=cut_x, rect.y_range()),
            Cut::End => Rect::from_x_y_ranges(cut_x..=rect.right(), rect.y_range()),
        };
        painter.rect_filled(gone, 0.0, Color32::from_black_alpha(14));
        let handle_color = if focused { theme::CAUTION } else { theme::BORDER_DARK };
        painter.line_segment([pos2(cut_x, rect.top()), pos2(cut_x, rect.bottom())], Stroke::new(2.0, handle_color));
        painter.rect_filled(Rect::from_center_size(pos2(cut_x, rect.top() + 7.0), vec2(12.0, 14.0)), 2.0, handle_color);
        let proposed = self.proposed.is_some_and(|p| match cut {
            Cut::Start => p.start > 0 && p.start == self.start,
            Cut::End => p.end > 0 && p.end == self.end,
        });
        if proposed {
            let note = "Possible announcement after this point";
            let at = if cut == Cut::End {
                pos2(cut_x + 10.0, rect.top() + 6.0)
            } else {
                pos2(cut_x - 10.0, rect.top() + 6.0)
            };
            let align = if cut == Cut::End { Align2::LEFT_TOP } else { Align2::RIGHT_TOP };
            painter.text(at, align, note, FontId::proportional(11.0), Color32::from_rgb(0x8A, 0x54, 0x10));
        }
        painter.rect_stroke(rect, 4.0, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
        // Dragging, or clicking, places the cut; near a pause it snaps unless Alt is held.
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked())
        {
            self.focus = cut;
            let free = ui.input(|i| i.modifiers.alt);
            let mut sample = sample_of(pointer.x);
            if !free
                && let Some(pause) = self.pauses.iter().map(Pause::middle).min_by_key(|m| m.abs_diff(sample))
                && (x_of(pause) - pointer.x).abs() <= SNAP_POINTS
            {
                sample = pause;
            }
            self.place(cut, sample);
        }
        ui.horizontal(|ui| {
            let (label, amount) = match cut {
                Cut::Start => ("Cut from the start", self.start),
                Cut::End => ("Cut from the end", self.end),
            };
            ui.label(label);
            let mut text = format!("{:.1}", seconds(amount));
            let field = ui.add(egui::TextEdit::singleline(&mut text).desired_width(60.0));
            if field.changed()
                && let Ok(value) = text.trim().trim_end_matches('s').trim().parse::<f64>()
            {
                match cut {
                    Cut::Start => self.start = samples(value).min(self.length),
                    Cut::End => self.end = samples(value).min(self.length),
                }
            }
            ui.label("s");
            let (play, what) = match cut {
                Cut::Start => ("▶ Play from Cut", "▶ Play What Is Cut"),
                Cut::End => ("▶ Play to Cut", "▶ Play What Is Cut"),
            };
            if ui.button(play).clicked() {
                actions.push(self.listen_across(cut));
            }
            if ui.button(what).clicked() {
                actions.push(self.listen_cut(cut));
            }
        });
    }

    /// Puts `cut` at `sample` of the track.
    fn place(&mut self, cut: Cut, sample: u64) {
        match cut {
            Cut::Start => self.start = sample.min(self.length),
            Cut::End => self.end = self.length.saturating_sub(sample),
        }
        self.error = None;
    }

    /// The editor's keys.
    fn keys(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>, result: &mut Option<bool>) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let pressed = |modifiers: Modifiers, key: Key| ctx.input_mut(|i| i.consume_key(modifiers, key));
        if pressed(Modifiers::NONE, Key::Tab) || pressed(Modifiers::SHIFT, Key::Tab) {
            self.focus = if self.focus == Cut::Start { Cut::End } else { Cut::Start };
        }
        let position = match self.focus {
            Cut::Start => self.start,
            Cut::End => self.length - self.end,
        };
        // Option first: egui would match a plain arrow when Option is held too.
        if pressed(Modifiers::ALT, Key::ArrowLeft) {
            let before = self.pauses.iter().map(Pause::middle).filter(|m| *m < position).max();
            if let Some(sample) = before {
                self.place(self.focus, sample);
            }
        } else if pressed(Modifiers::ALT, Key::ArrowRight) {
            let after = self.pauses.iter().map(Pause::middle).filter(|m| *m > position).min();
            if let Some(sample) = after {
                self.place(self.focus, sample);
            }
        } else {
            for (modifiers, step) in [(Modifiers::SHIFT, 1.0), (Modifiers::NONE, 0.1)] {
                if pressed(modifiers, Key::ArrowLeft) {
                    self.place(self.focus, position.saturating_sub(samples(step)));
                }
                if pressed(modifiers, Key::ArrowRight) {
                    self.place(self.focus, (position + samples(step)).min(self.length));
                }
            }
        }
        if pressed(Modifiers::NONE, Key::Num0) {
            match self.focus {
                Cut::Start => self.start = 0,
                Cut::End => self.end = 0,
            }
        }
        if pressed(Modifiers::NONE, Key::P) {
            actions.push(self.listen_across(self.focus));
        }
        if pressed(Modifiers::NONE, Key::X) {
            actions.push(self.listen_cut(self.focus));
        }
        if pressed(Modifiers::NONE, Key::Enter) {
            *result = Some(true);
        }
        if pressed(Modifiers::NONE, Key::Escape) {
            *result = Some(false);
        }
    }

    /// What the book will contain around the cut: after the start cut, before the end cut.
    fn listen_across(&self, cut: Cut) -> Action {
        let span = samples(LISTEN_SECONDS);
        let (from, to) = match cut {
            Cut::Start => (self.start, self.start + span),
            Cut::End => {
                let at = self.length - self.end;
                (at.saturating_sub(span), at)
            }
        };
        Action::PreviewBook { track: self.track, from, to }
    }

    /// What the cut takes away.
    fn listen_cut(&self, cut: Cut) -> Action {
        let (from, to) = match cut {
            Cut::Start => (0, self.start),
            Cut::End => (self.length - self.end, self.length),
        };
        Action::PreviewBook { track: self.track, from, to }
    }
}

fn clock(samples: u64) -> String {
    let total = seconds(samples);
    format!("{}:{:04.1}", (total / 60.0) as u64, total % 60.0)
}
