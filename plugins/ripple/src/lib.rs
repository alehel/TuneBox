//! Ripple: an example visualizer plug-in. Rings ripple out from the centre on each beat, over a
//! circle of spectrum bars and the waveform.
//!
//! Build it with `cargo xtask plugins` and copy `target/visualizer-plugins/ripple.wasm` into
//! TuneBox's `Visualizers` folder (see `docs/visualizer-plugins.md`).

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::f32::consts::TAU;
use tunebox_visualizer_sdk::{Canvas, Color, Frame, Look, Vertex, export};

/// A ring on its way out: when it started and how hard the beat was.
#[derive(Clone, Copy, Debug)]
struct Ring {
    born: f32,
    strength: f32,
}

/// How long a ring takes to fade, in seconds.
const RING_LIFE: f32 = 2.5;
const BARS: usize = 96;

#[derive(Default)]
pub struct Ripple {
    rings: Vec<Ring>,
    /// The bass level the last ring started at, so one beat starts one ring.
    last_beat: f32,
}

impl Look for Ripple {
    const ID: &'static str = "ripple";
    const NAME: &'static str = "Ripple";

    fn draw(&mut self, frame: &Frame, canvas: &mut Canvas) {
        let (width, height) = canvas.size();
        let (cx, cy) = (width / 2.0, height / 2.0);
        let unit = width.min(height);
        background(canvas, width, height, frame.time);

        if frame.bass > self.last_beat + 0.12 || (self.rings.is_empty() && frame.bass > 0.2) {
            self.rings.push(Ring { born: frame.time, strength: frame.bass });
            self.last_beat = frame.bass;
        }
        self.last_beat = (self.last_beat - frame.dt * 0.6).max(0.0);
        self.rings.retain(|ring| frame.time - ring.born < RING_LIFE);
        for ring in &self.rings {
            let age = (frame.time - ring.born) / RING_LIFE;
            let color = Color::hsva(0.55 + ring.born * 0.02, 0.6, 1.0, (1.0 - age) * (0.3 + ring.strength * 0.7));
            canvas.stroke_circle(cx, cy, unit * (0.12 + age * 0.6), 1.0 + ring.strength * 6.0 * (1.0 - age), color);
        }

        for i in 0..BARS {
            let position = i as f32 / BARS as f32;
            // Mirror the spectrum so the circle is symmetrical, bass at the top and bottom.
            let band = frame.band_at(1.0 - (position * 2.0 - 1.0).abs());
            let angle = position * TAU + frame.time * 0.1;
            let (sin, cos) = angle.sin_cos();
            let inner = unit * 0.12;
            let outer = inner + 4.0 + band * unit * 0.25;
            let color = Color::hsva(position + frame.time * 0.03, 0.7, 1.0, 0.9);
            canvas.line(
                &[(cx + cos * inner, cy + sin * inner), (cx + cos * outer, cy + sin * outer)],
                false,
                3.0,
                color,
            );
        }

        let wave = frame.waveform();
        let points: Vec<(f32, f32)> = wave
            .iter()
            .enumerate()
            .map(|(i, sample)| (i as f32 / (wave.len() - 1) as f32 * width, height * 0.85 + sample * height * 0.08))
            .collect();
        canvas.line(&points, false, 1.5, Color::rgba(255, 255, 255, 120));
        canvas.fill_circle(cx, cy, unit * (0.06 + frame.bass * 0.04), Color::hsva(0.6, 0.3, 1.0, 0.9));
    }
}

/// A vertical gradient, slowly changing hue.
fn background(canvas: &mut Canvas, width: f32, height: f32, time: f32) {
    let top = Color::hsva(0.62 + time * 0.01, 0.8, 0.18, 1.0);
    let bottom = Color::rgb(2, 2, 8);
    let corners = [(0.0, 0.0, top), (width, 0.0, top), (width, height, bottom), (0.0, height, bottom)];
    let vertices: Vec<Vertex> = corners.iter().map(|&(x, y, color)| Vertex { x, y, color }).collect();
    canvas.mesh(&vertices, &[0, 1, 2, 0, 2, 3]);
}

export!(Ripple);

#[cfg(test)]
mod tests {
    use super::*;
    use tunebox_visualizer_sdk::abi::{BANDS, WAVE_SAMPLES};

    #[test]
    fn draws_every_shape_it_means_to() {
        let frame = Frame {
            bands: [0.5; BANDS],
            peaks: [0.6; BANDS],
            level: 0.5,
            bass: 0.8,
            waveforms: vec![[0.2; WAVE_SAMPLES]],
            time: 1.0,
            dt: 0.016,
            playing: true,
        };
        let mut look = Ripple::default();
        let mut canvas = Canvas::new(800.0, 600.0);
        look.draw(&frame, &mut canvas);
        assert_eq!(canvas.left_out(), 0);
        assert_eq!(look.rings.len(), 1);
        assert!(!canvas.words().is_empty());
    }
}
