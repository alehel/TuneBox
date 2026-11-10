//! Pulse: a ring of rays turning around a core that throbs with the bass.

use crate::paint::{background, hue, quad};
use crate::{Frame, Visualizer};
use egui::epaint::Mesh;
use egui::{Color32, Painter, Rect, Stroke, vec2};
use std::f32::consts::TAU;

/// The ring turns faster when the music is louder; `spin` is where it has got to.
#[derive(Default)]
pub struct Pulse {
    spin: f32,
}

const RAYS: usize = 128;

impl Visualizer for Pulse {
    fn id(&self) -> &'static str {
        "pulse"
    }

    fn name(&self) -> &'static str {
        "Pulse"
    }

    fn paint(&mut self, painter: &Painter, rect: Rect, frame: &Frame) {
        self.spin += frame.dt * (0.15 + frame.level * 0.6);
        background(painter, rect, Color32::from_rgb(14, 4, 26), Color32::from_rgb(2, 2, 10));
        let center = rect.center();
        let unit = rect.width().min(rect.height());
        let core = unit * (0.11 + frame.bass * 0.06);
        for ring in (1..=5).rev() {
            let radius = core * (1.0 + ring as f32 * 0.32) + frame.level * unit * 0.03 * ring as f32;
            painter.circle_stroke(
                center,
                radius,
                Stroke::new(1.5, hue(0.78 + frame.time * 0.03, 0.6, 1.0, 0.05 * (6 - ring) as f32)),
            );
        }
        let mut mesh = Mesh::default();
        let quarter = RAYS / 4;
        for i in 0..RAYS {
            // Mirror the spectrum into four quarters so the ring is symmetrical, bass at the seams.
            let q = i % (2 * quarter);
            let position = if q < quarter { q } else { 2 * quarter - 1 - q } as f32 / (quarter - 1) as f32;
            let band = frame.band_at(position);
            let angle = self.spin + i as f32 / RAYS as f32 * TAU;
            let dir = vec2(angle.cos(), angle.sin());
            let side = vec2(-dir.y, dir.x) * (TAU * core / RAYS as f32 * 0.42);
            let inner = center + dir * (core * 1.08);
            let outer = center + dir * (core * 1.1 + 6.0 + band * unit * 0.3);
            let color = hue(i as f32 / RAYS as f32 + frame.time * 0.05, 0.75, 1.0, 1.0);
            let dim = color.gamma_multiply(0.4);
            quad(&mut mesh, [inner - side, outer - side, outer + side, inner + side], [dim, color, color, dim]);
        }
        painter.add(mesh);
        painter.circle_filled(center, core, hue(0.85 + frame.time * 0.03, 0.55, 0.35 + frame.bass * 0.5, 1.0));
        painter.circle_filled(center, core * 0.55, Color32::from_white_alpha((60.0 + frame.bass * 160.0) as u8));
    }
}
