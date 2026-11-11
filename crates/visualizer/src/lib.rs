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
//!
//! Looks can also be **plug-ins** loaded at run time: WebAssembly modules run in a sandbox by
//! `tunebox_engine::plugins`. [`PluginLook`] paints one, turning its drawing into egui shapes
//! ([`plugin_shapes`]); the registry turns off a plug-in that fails. See
//! `docs/visualizer-plugins.md`.

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
mod plugin_look;
pub mod plugin_shapes;
mod registry;

/// The egui version looks are drawn with.
pub use egui;
pub use egui::{Painter, Rect};
pub use meter::{BANDS, Meter};
pub use plugin_look::{PLUGIN_ID_PREFIX, PluginLook};
pub use registry::{LookEntry, Painted, Registry};

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
    fn id(&self) -> &str;

    /// The name shown in the picker (e.g. `"Spectrum"`).
    fn name(&self) -> &str;

    /// Paints one frame filling `rect`. The painter is clipped to `rect`.
    fn paint(&mut self, painter: &Painter, rect: Rect, frame: &Frame);
}
