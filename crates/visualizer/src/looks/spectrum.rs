//! Spectrum: bars rising from a mirrored floor, teal to magenta, with falling peak caps.

use crate::paint::{background, hue, quad};
use crate::{Frame, Visualizer};
use egui::epaint::Mesh;
use egui::{Color32, Painter, Rect, Stroke, pos2, vec2};

pub struct Spectrum;

impl Visualizer for Spectrum {
    fn id(&self) -> &'static str {
        "spectrum"
    }

    fn name(&self) -> &'static str {
        "Spectrum"
    }

    fn paint(&mut self, painter: &Painter, rect: Rect, frame: &Frame) {
        background(painter, rect, Color32::from_rgb(6, 8, 22), Color32::from_rgb(18, 10, 36));
        let count = frame.bands.len().max(1);
        let floor = rect.top() + rect.height() * 0.66;
        let usable = rect.width() * 0.86;
        let left = rect.center().x - usable / 2.0;
        let slot = usable / count as f32;
        let height = rect.height() * 0.55;
        let mut mesh = Mesh::default();
        for (i, (band, peak)) in frame.bands.iter().zip(frame.peaks).enumerate() {
            let (x, w) = (left + i as f32 * slot + slot * 0.12, slot * 0.76);
            let h = band.max(0.012) * height;
            let shade = i as f32 / count as f32;
            let bottom = hue(0.48 + shade * 0.12, 0.85, 0.75, 1.0);
            let top = hue(0.83 - shade * 0.05 + frame.time * 0.02, 0.75, 1.0, 1.0);
            quad(
                &mut mesh,
                [pos2(x, floor - h), pos2(x + w, floor - h), pos2(x + w, floor), pos2(x, floor)],
                [top, top, bottom, bottom],
            );
            // The reflection, fading into the floor.
            let (y, r) = (floor + 2.0, h * 0.45);
            let faint = bottom.gamma_multiply(0.35);
            quad(
                &mut mesh,
                [pos2(x, y), pos2(x + w, y), pos2(x + w, y + r), pos2(x, y + r)],
                [faint, faint, Color32::TRANSPARENT, Color32::TRANSPARENT],
            );
            let cap = floor - peak.max(0.012) * height - 4.0;
            painter.rect_filled(Rect::from_min_size(pos2(x, cap), vec2(w, 2.5)), 1.0, Color32::from_white_alpha(220));
        }
        painter.add(mesh);
        painter.hline(rect.x_range(), floor + 1.0, Stroke::new(1.0, Color32::from_white_alpha(28)));
    }
}
