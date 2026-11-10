//! Oscilloscope: a glowing trace on a dark screen, the last few sweeps fading behind it.

use crate::paint::{background, hue};
use crate::{Frame, Visualizer};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

pub struct Scope;

impl Visualizer for Scope {
    fn id(&self) -> &'static str {
        "scope"
    }

    fn name(&self) -> &'static str {
        "Oscilloscope"
    }

    fn paint(&mut self, painter: &Painter, rect: Rect, frame: &Frame) {
        background(painter, rect, Color32::from_rgb(2, 14, 10), Color32::from_rgb(0, 6, 4));
        let grid = Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 255, 170, 14));
        for i in 1..10 {
            painter.vline(rect.left() + rect.width() * i as f32 / 10.0, rect.y_range(), grid);
        }
        for i in 1..8 {
            painter.hline(rect.x_range(), rect.top() + rect.height() * i as f32 / 8.0, grid);
        }
        let amplitude = rect.height() * (0.18 + frame.level * 0.2);
        let color = hue(0.42 + frame.level * 0.1, 0.7, 1.0, 1.0);
        let count = frame.waveforms.len();
        for (age, wave) in frame.waveforms.iter().enumerate() {
            let points = trace(rect, wave, amplitude);
            if age + 1 == count {
                // The newest sweep, with a soft glow around a crisp line.
                for (width, alpha) in [(9.0, 0.08), (4.5, 0.22), (1.8, 1.0)] {
                    painter.add(Shape::line(points.clone(), Stroke::new(width, color.gamma_multiply(alpha))));
                }
            } else {
                let freshness = (age + 1) as f32 / count as f32;
                painter.add(Shape::line(points, Stroke::new(1.2, color.gamma_multiply(0.18 * freshness))));
            }
        }
    }
}

fn trace(rect: Rect, wave: &[f32], amplitude: f32) -> Vec<Pos2> {
    let last = wave.len().max(2) - 1;
    wave.iter()
        .enumerate()
        .map(|(i, s)| pos2(rect.left() + rect.width() * i as f32 / last as f32, rect.center().y - s * amplitude))
        .collect()
}
