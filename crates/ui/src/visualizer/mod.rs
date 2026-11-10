//! The visualizer view: the chosen look filling the middle of the window, with the playing
//! track's details over it if wanted. The looks themselves live in `tunebox-visualizer`.

mod overlay;

use crate::actions::Action;
use crate::panels::PanelContext;
use egui::Ui;
use tunebox_engine::Notice;
use tunebox_engine::plugins::Plugin;
use tunebox_visualizer::{Meter, Registry, Visualizer};

/// Samples read from the player per frame.
const WINDOW: usize = 2048;

/// What the visualizer view keeps between frames.
pub struct VisualizerView {
    meter: Meter,
    registry: Registry,
    last_time: Option<f64>,
    /// When the pointer last moved, to show the controls.
    pointer_moved_at: f64,
    /// Keeps the controls showing regardless of the pointer (for screenshots).
    pub(crate) controls_pinned: bool,
}

impl VisualizerView {
    /// The built-in looks plus `extra` ones (which replace built-ins with the same id).
    pub fn new(extra: Vec<Box<dyn Visualizer>>) -> Self {
        let mut registry = Registry::with_builtins();
        for look in extra {
            registry.add(look);
        }
        Self {
            meter: Meter::default(),
            registry,
            last_time: None,
            pointer_moved_at: f64::NEG_INFINITY,
            controls_pinned: false,
        }
    }

    /// Replaces the plug-in looks with a fresh scan's.
    pub fn set_plugins(&mut self, plugins: Vec<Plugin>) {
        self.registry.set_plugins(plugins);
    }

    /// Draws the visualizer filling the space `ui` gives it.
    pub fn show(&mut self, ui: &mut Ui, cx: &mut PanelContext) {
        let rect = ui.max_rect();
        let now = ui.input(|i| i.time);
        if ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
            self.pointer_moved_at = now;
        }
        let dt = self.last_time.map_or(0.0, |t| (now - t) as f32);
        self.last_time = Some(now);
        self.meter.update(cx.session.recent_audio(WINDOW).as_ref(), dt);

        let chosen = cx.session.preferences().visualizer_style.clone();
        let painted = self.registry.paint(&chosen, &ui.painter_at(rect), rect, &self.meter.frame());
        if let Some(notice) = painted.turned_off {
            cx.act(Action::Notify(Notice::warning(notice)));
        }
        let show_controls = self.controls_pinned || now - self.pointer_moved_at < 2.5;
        overlay::show(ui, rect, cx, &self.registry.entries(), &painted.shown, show_controls);
        ui.ctx().request_repaint();
    }
}
