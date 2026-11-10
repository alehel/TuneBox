//! Hyperspace: flying through stars that streak faster as the music gets louder.

use crate::paint::{background, hue};
use crate::{Frame, Visualizer};
use egui::{Color32, Painter, Rect, Stroke, vec2};

/// A star in a unit cube in front of the viewer; `z` is its distance.
#[derive(Clone, Copy)]
struct Star {
    x: f32,
    y: f32,
    z: f32,
}

pub struct Hyperspace {
    stars: Vec<Star>,
    seed: u64,
}

const STARS: usize = 320;

impl Default for Hyperspace {
    fn default() -> Self {
        let mut look = Self { stars: Vec::with_capacity(STARS), seed: 0x2545_F491_4F6C_DD1D };
        for _ in 0..STARS {
            let z = 0.05 + look.random() * 0.95;
            let star = look.star(z);
            look.stars.push(star);
        }
        look
    }
}

impl Hyperspace {
    /// A pseudo-random number in 0–1 (xorshift: variety, not security).
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 40) as f32 / (1u64 << 24) as f32
    }

    fn star(&mut self, z: f32) -> Star {
        Star { x: self.random() * 2.0 - 1.0, y: self.random() * 2.0 - 1.0, z }
    }

    /// Flies forward; stars that pass the viewer reappear in the distance.
    fn fly(&mut self, frame: &Frame) {
        let speed = 0.08 + frame.level * 0.9 + frame.bass * 0.8;
        for i in 0..self.stars.len() {
            let Some(mut star) = self.stars.get(i).copied() else { continue };
            star.z -= speed * frame.dt;
            if star.z <= 0.02 {
                star = self.star(1.0);
            }
            if let Some(slot) = self.stars.get_mut(i) {
                *slot = star;
            }
        }
    }
}

impl Visualizer for Hyperspace {
    fn id(&self) -> &'static str {
        "hyperspace"
    }

    fn name(&self) -> &'static str {
        "Hyperspace"
    }

    fn paint(&mut self, painter: &Painter, rect: Rect, frame: &Frame) {
        self.fly(frame);
        background(painter, rect, Color32::from_rgb(0, 0, 6), Color32::from_rgb(6, 2, 18));
        let center = rect.center();
        let scale = rect.width().max(rect.height()) * 0.175;
        let streak = 0.012 + frame.level * 0.05 + frame.bass * 0.04;
        for (i, star) in self.stars.iter().enumerate() {
            let project = |z: f32| center + vec2(star.x / z, star.y / z) * scale;
            let head = project(star.z);
            if !rect.contains(head) {
                continue;
            }
            let tail = project(star.z + streak);
            let position = i as f32 / STARS as f32;
            let band = frame.band_at(position);
            let brightness = ((1.0 - star.z) * 1.3).clamp(0.1, 1.0);
            let color = hue(0.55 + position * 0.45, 0.25 + band * 0.6, 1.0, brightness);
            painter.line_segment([tail, head], Stroke::new(0.6 + (1.0 - star.z) * 2.2, color));
        }
        painter.circle_filled(center, 2.0 + frame.bass * 30.0, Color32::from_white_alpha((frame.bass * 40.0) as u8));
    }
}
