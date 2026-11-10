//! tunebox-visualizer — music drawn as moving light.
//!
//! A visualizer is anything that implements [`Visualizer`]: it gets a [`Frame`] describing the
//! audio playing right now (spectrum bands, peaks, loudness, bass, recent waveforms, time) and
//! paints one frame with an egui [`Painter`]. It may keep its own animation state.
//!
//! [`Meter`] turns the player's audio tap into frames; [`Registry`] holds the looks the listener
//! chooses between. The built-in looks live in [`looks`], one file each.
//!
//! **Adding a look** takes one file and one line: implement [`Visualizer`] in
//! `src/looks/<name>.rs`, and add it to [`looks::builtin`]. A look can also come from another
//! crate: build the app with `tunebox_ui::run_with(options, vec![Box::new(MyLook)])`. See
//! `docs/visualizers.md` for a complete example.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod looks;
mod meter;
pub mod paint;

/// The egui version looks are drawn with.
pub use egui;
pub use egui::{Painter, Rect};
pub use meter::{BANDS, Meter};

/// What the audio is doing, for one frame.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    /// Energy per band, 0–1, lowest frequencies first, spaced like hearing ([`BANDS`] of them).
    /// Smoothed: rises quickly, falls gently.
    pub bands: &'a [f32],
    /// Per band, the recent maximum, falling slowly (for peak markers).
    pub peaks: &'a [f32],
    /// Overall loudness, 0–1.
    pub level: f32,
    /// Loudness of the lowest bands, 0–1: the beat.
    pub bass: f32,
    /// The last few waveforms, oldest first; each is 256 samples scaled to ±1.
    pub waveforms: &'a [Vec<f32>],
    /// Seconds since the visualizer opened.
    pub time: f32,
    /// Seconds since the previous frame.
    pub dt: f32,
    /// Whether anything is playing. Looks keep moving gently when nothing is.
    pub playing: bool,
}

impl Frame<'_> {
    /// The band nearest `position` (0 = lowest frequency, 1 = highest).
    pub fn band_at(&self, position: f32) -> f32 {
        let last = self.bands.len().saturating_sub(1);
        self.bands.get((position.clamp(0.0, 1.0) * last as f32).round() as usize).copied().unwrap_or(0.0)
    }

    /// The newest waveform.
    pub fn waveform(&self) -> &[f32] {
        self.waveforms.last().map_or(&[], Vec::as_slice)
    }
}

/// One look.
pub trait Visualizer {
    /// A stable identifier, saved in preferences: lowercase, no spaces, never changed once
    /// released (e.g. `"spectrum"`).
    fn id(&self) -> &'static str;

    /// The name shown in the picker (e.g. `"Spectrum"`).
    fn name(&self) -> &'static str;

    /// Paints one frame filling `rect`. The painter is clipped to `rect`.
    fn paint(&mut self, painter: &Painter, rect: Rect, frame: &Frame);
}

/// The looks to choose between, in picker order.
pub struct Registry {
    looks: Vec<Box<dyn Visualizer>>,
}

impl Registry {
    /// The built-in looks.
    pub fn with_builtins() -> Self {
        Self { looks: looks::builtin() }
    }

    /// Adds a look, replacing any with the same id.
    pub fn add(&mut self, look: Box<dyn Visualizer>) {
        match self.looks.iter_mut().find(|l| l.id() == look.id()) {
            Some(slot) => *slot = look,
            None => self.looks.push(look),
        }
    }

    /// `(id, name)` of every look, in picker order.
    pub fn entries(&self) -> Vec<(&'static str, &'static str)> {
        self.looks.iter().map(|l| (l.id(), l.name())).collect()
    }

    /// The look with `id`, or the first look when there is none (an unknown preference).
    pub fn get_mut(&mut self, id: &str) -> Option<&mut (dyn Visualizer + 'static)> {
        let index = self.looks.iter().position(|l| l.id() == id).unwrap_or(0);
        self.looks.get_mut(index).map(|b| b.as_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Blank;

    impl Visualizer for Blank {
        fn id(&self) -> &'static str {
            "spectrum"
        }
        fn name(&self) -> &'static str {
            "Blank"
        }
        fn paint(&mut self, _: &Painter, _: Rect, _: &Frame) {}
    }

    #[test]
    fn builtin_ids_are_unique_and_lookup_falls_back_to_the_first() {
        let mut registry = Registry::with_builtins();
        let entries = registry.entries();
        let mut ids: Vec<_> = entries.iter().map(|e| e.0).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), entries.len());
        assert_eq!(registry.get_mut("no such look").map(|l| l.id()), entries.first().map(|e| e.0));
    }

    #[test]
    fn adding_a_look_with_a_known_id_replaces_it() {
        let mut registry = Registry::with_builtins();
        let count = registry.entries().len();
        registry.add(Box::new(Blank));
        assert_eq!(registry.entries().len(), count);
        assert_eq!(registry.get_mut("spectrum").map(|l| l.name()), Some("Blank"));
    }

    #[test]
    fn every_builtin_paints_without_panicking() {
        let ctx = egui::Context::default();
        let mut registry = Registry::with_builtins();
        let bands = vec![0.5; BANDS];
        let waves = vec![vec![0.3; 256]];
        let frame = Frame {
            bands: &bands,
            peaks: &bands,
            level: 0.5,
            bass: 0.7,
            waveforms: &waves,
            time: 1.0,
            dt: 0.016,
            playing: true,
        };
        let empty = Frame { bands: &[], peaks: &[], waveforms: &[], playing: false, ..frame };
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
            for (id, _) in registry.entries() {
                if let Some(look) = registry.get_mut(id) {
                    look.paint(ui.painter(), rect, &frame);
                    look.paint(ui.painter(), rect, &empty);
                }
            }
        });
        output.textures_delta.clear();
    }
}
