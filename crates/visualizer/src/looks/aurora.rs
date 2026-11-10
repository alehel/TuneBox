//! Aurora: translucent ribbons of light swaying across a starry sky, swelling with the music.

use crate::paint::{background, hue, quad};
use crate::{Frame, Visualizer};
use egui::epaint::Mesh;
use egui::{Color32, Painter, Rect, pos2};

pub struct Aurora;

const RIBBONS: usize = 4;
const STEPS: usize = 140;

impl Visualizer for Aurora {
    fn id(&self) -> &str {
        "aurora"
    }

    fn name(&self) -> &str {
        "Aurora"
    }

    fn paint(&mut self, painter: &Painter, rect: Rect, frame: &Frame) {
        background(painter, rect, Color32::from_rgb(2, 6, 18), Color32::from_rgb(8, 26, 40));
        stars(painter, rect, frame.time);
        for ribbon in 0..RIBBONS {
            painter.add(ribbon_mesh(rect, frame, ribbon as f32));
        }
    }
}

/// Fixed stars that twinkle.
fn stars(painter: &Painter, rect: Rect, time: f32) {
    for i in 0..90 {
        let fx = ((i as f32 * 12.9898).sin() * 43758.547).fract().abs();
        let fy = ((i as f32 * 78.233).sin() * 12345.678).fract().abs();
        let twinkle = 0.4 + 0.6 * ((time * 1.7 + i as f32).sin() * 0.5 + 0.5);
        let at = pos2(rect.left() + fx * rect.width(), rect.top() + fy * rect.height() * 0.7);
        painter.circle_filled(at, 1.1, Color32::from_white_alpha((twinkle * 160.0) as u8));
    }
}

/// One ribbon: bright along its centre line, fading to nothing above and below.
fn ribbon_mesh(rect: Rect, frame: &Frame, r: f32) -> Mesh {
    let point = |t: f32| {
        let band = frame.band_at(t);
        let wave = (t * (3.0 + r) + frame.time * (0.35 + r * 0.12) + r * 1.7).sin() * 0.09
            + (t * 9.0 - frame.time * 0.6 + r).sin() * 0.03 * (1.0 + frame.level * 2.0);
        let y = rect.top() + rect.height() * (0.34 + r * 0.09 + wave);
        let thickness = rect.height() * (0.04 + band * 0.22 + frame.bass * 0.05);
        (pos2(rect.left() + t * rect.width(), y), thickness, band)
    };
    let color = |t: f32, band: f32| hue(0.36 + r * 0.09 + t * 0.18 + frame.time * 0.01, 0.8, 1.0, 0.18 + band * 0.4);
    let mut mesh = Mesh::default();
    for step in 0..STEPS {
        let (a, b) = (step as f32 / STEPS as f32, (step + 1) as f32 / STEPS as f32);
        let ((p0, t0, b0), (p1, t1, b1)) = (point(a), point(b));
        let (c0, c1) = (color(a, b0), color(b, b1));
        let clear = Color32::TRANSPARENT;
        quad(&mut mesh, [pos2(p0.x, p0.y - t0), pos2(p1.x, p1.y - t1), p1, p0], [clear, clear, c1, c0]);
        quad(&mut mesh, [p0, p1, pos2(p1.x, p1.y + t1 * 0.35), pos2(p0.x, p0.y + t0 * 0.35)], [c0, c1, clear, clear]);
    }
    mesh
}
