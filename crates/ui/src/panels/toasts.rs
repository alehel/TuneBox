//! Notices from the engine, shown for a few seconds in the bottom-right corner.

use egui::{Align2, Color32, Frame, Margin, RichText};
use std::time::{Duration, Instant};
use tunebox_engine::{Notice, NoticeLevel};

const VISIBLE_FOR: Duration = Duration::from_secs(7);
const MAX_SHOWN: usize = 4;

#[derive(Default)]
pub struct Toasts {
    shown: Vec<(Notice, Instant)>,
}

impl Toasts {
    pub fn push(&mut self, notice: Notice) {
        self.shown.push((notice, Instant::now()));
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.shown.retain(|(_, since)| since.elapsed() < VISIBLE_FOR);
        let start = self.shown.len().saturating_sub(MAX_SHOWN);
        for (i, (notice, _)) in self.shown.iter().skip(start).enumerate() {
            let (fill, text) = match notice.level {
                NoticeLevel::Info => (Color32::from_rgb(0x30, 0x33, 0x38), Color32::WHITE),
                NoticeLevel::Warning => (Color32::from_rgb(0x8A, 0x5A, 0x0E), Color32::WHITE),
                NoticeLevel::Error => (Color32::from_rgb(0x9E, 0x24, 0x24), Color32::WHITE),
            };
            egui::Area::new(egui::Id::unique(("tunebox-toast", i)))
                .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -42.0 - 52.0 * i as f32))
                .interactable(false)
                .show(ctx, |ui| {
                    Frame::new()
                        .fill(fill.gamma_multiply(0.94))
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_max_width(380.0);
                            ui.label(RichText::new(&notice.text).color(text).size(12.0));
                        });
                });
        }
        if !self.shown.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}
