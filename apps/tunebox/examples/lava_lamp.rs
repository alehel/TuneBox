//! A visualizer plug-in written outside TuneBox's own crates.
//!
//! ```sh
//! cargo run --example lava_lamp -p tunebox
//! ```
//!
//! Then choose View ▸ Visualizer and pick "Lava Lamp" from the bar at the bottom.

use tunebox_ui::tunebox_visualizer::egui::{Color32, Painter, Rect, pos2};
use tunebox_ui::tunebox_visualizer::paint::{background, hue};
use tunebox_ui::tunebox_visualizer::{Frame, Visualizer};

/// Soft blobs that drift and swell with the bass.
struct LavaLamp;

impl Visualizer for LavaLamp {
    fn id(&self) -> &str {
        "lava-lamp"
    }

    fn name(&self) -> &str {
        "Lava Lamp"
    }

    fn paint(&mut self, painter: &Painter, rect: Rect, frame: &Frame) {
        background(painter, rect, Color32::from_rgb(40, 0, 20), Color32::from_rgb(90, 20, 0));
        for blob in 0..7 {
            let b = blob as f32;
            let x = rect.left() + rect.width() * (0.5 + 0.38 * (frame.time * 0.11 + b * 1.9).sin());
            let y = rect.top() + rect.height() * (0.5 + 0.4 * (frame.time * 0.07 + b * 2.3).cos());
            let radius = rect.height() * (0.06 + frame.band_at(b / 7.0) * 0.12 + frame.bass * 0.05);
            for (scale, alpha) in [(1.6, 0.08), (1.25, 0.2), (1.0, 0.9)] {
                painter.circle_filled(pos2(x, y), radius * scale, hue(0.02 + b * 0.02, 0.85, 1.0, alpha));
            }
        }
    }
}

fn main() -> Result<(), String> {
    tunebox_ui::run_with(tunebox_ui::LaunchOptions::default(), vec![Box::new(LavaLamp)])
}
