//! The visualizer view: the chosen look filling the middle of the window (or, in full screen,
//! the whole screen), with the playing track's details over it if wanted. The looks themselves
//! live in `tunebox-visualizer`.

mod overlay;

use crate::actions::Action;
use crate::panels::PanelContext;
use egui::Ui;
use tunebox_engine::Notice;
use tunebox_engine::commands::CommandId;
use tunebox_engine::plugins::Plugin;
use tunebox_visualizer::{Meter, Registry, Visualizer};

/// Samples read from the player per frame.
const WINDOW: usize = 2048;
/// How long the controls (and, in full screen, the pointer) stay after the pointer stops.
const IDLE_SECONDS: f64 = 2.5;

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
        // Made before the controls, so a click on a control is the control's, not this.
        let background = ui.interact(rect, ui.scope_id().with("visualizer-background"), egui::Sense::click());
        if background.double_clicked() {
            cx.act(Action::Run(CommandId::FullScreenVisualizer));
        }
        let idle = !self.controls_pinned && is_idle(now, self.pointer_moved_at);
        if idle && cx.view.full_screen.is_on() {
            // Nothing should sit over the picture once the listener has stopped moving.
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        }
        overlay::show(ui, rect, cx, &self.registry.entries(), &painted.shown, !idle);
        ui.ctx().request_repaint();
    }
}

/// Whether the pointer has been still long enough to hide the controls (and the pointer).
fn is_idle(now: f64, pointer_moved_at: f64) -> bool {
    now - pointer_moved_at >= IDLE_SECONDS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_hide_a_while_after_the_pointer_stops() {
        assert!(!is_idle(10.0, 9.0));
        assert!(!is_idle(11.4, 9.0));
        assert!(is_idle(11.5, 9.0));
        // Before the pointer has ever moved, nothing shows.
        assert!(is_idle(0.0, f64::NEG_INFINITY));
    }
}
