//! The full-screen visualizer's bookkeeping: what the window was like before, so leaving puts
//! it back exactly, and which commands work without leaving.
//!
//! Pure: the app reports what the window says each frame and carries out the [`Effect`]s it
//! gets back; nothing here talks to the window. If the system refuses full screen, the window
//! stays as it is and the visualizer fills it without the chrome, which is fallback enough to
//! need no message.
//!
//! Nothing here is saved: TuneBox always starts in a window.

use egui::{Pos2, Vec2, ViewportInfo};
use tunebox_engine::commands::CommandId;

/// How long to wait for the system to leave full screen before putting the window's size and
/// place back anyway.
const RESTORE_WAIT_SECONDS: f64 = 1.0;

/// The window as it was when the full-screen visualizer began.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowBefore {
    /// The outer top-left corner, where the system says (Wayland does not).
    pub position: Option<Pos2>,
    pub inner_size: Option<Vec2>,
    pub maximized: bool,
    /// Already full screen by the system's own means: leaving the visualizer keeps it so.
    pub full_screen: bool,
    /// Whether the visualizer was showing in the window.
    pub show_visualizer: bool,
}

impl WindowBefore {
    pub fn from_viewport(viewport: &ViewportInfo, show_visualizer: bool) -> Self {
        Self {
            position: viewport.outer_rect.map(|rect| rect.min),
            inner_size: viewport.inner_rect.map(|rect| rect.size()),
            maximized: viewport.maximized == Some(true),
            full_screen: viewport.fullscreen == Some(true),
            show_visualizer,
        }
    }
}

/// What the app should do to the window or the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    SetFullScreen(bool),
    MoveTo(Pos2),
    Resize(Vec2),
    Maximize,
    ShowVisualizer(bool),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Off,
    On {
        before: WindowBefore,
        /// The system has said the window is full screen at least once.
        seen_full_screen: bool,
    },
    /// Left, and waiting for the system to leave full screen: a size set while the window is
    /// still full screen would be overwritten.
    Restoring { before: WindowBefore, asked_at: f64 },
}

/// Whether the full-screen visualizer is showing, and how to undo it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FullScreen {
    phase: Phase,
}

impl FullScreen {
    /// The visualizer fills the screen, with no toolbar, source list or status bar.
    pub fn is_on(&self) -> bool {
        matches!(self.phase, Phase::On { .. })
    }

    pub fn enter(&mut self, before: WindowBefore) -> Vec<Effect> {
        let before = match self.phase {
            Phase::On { .. } => return Vec::new(),
            // Back in before the window got back: what it is now is not what it was.
            Phase::Restoring { before, .. } => before,
            Phase::Off => before,
        };
        self.phase = Phase::On { before, seen_full_screen: before.full_screen };
        let mut effects = vec![Effect::ShowVisualizer(true)];
        if !before.full_screen {
            effects.push(Effect::SetFullScreen(true));
        }
        effects
    }

    pub fn leave(&mut self, now: f64) -> Vec<Effect> {
        let Phase::On { before, .. } = self.phase else { return Vec::new() };
        let mut effects = vec![Effect::ShowVisualizer(before.show_visualizer)];
        if before.full_screen {
            self.phase = Phase::Off;
        } else {
            effects.push(Effect::SetFullScreen(false));
            self.phase = Phase::Restoring { before, asked_at: now };
        }
        effects
    }

    /// Called every frame with whether the system says the window is full screen.
    pub fn update(&mut self, window_full_screen: Option<bool>, now: f64) -> Vec<Effect> {
        match self.phase {
            Phase::Off => Vec::new(),
            Phase::On { before, seen_full_screen } => {
                if window_full_screen == Some(true) {
                    self.phase = Phase::On { before, seen_full_screen: true };
                    Vec::new()
                } else if seen_full_screen && window_full_screen == Some(false) {
                    // The system took the window out (its own key or button): follow it, and
                    // leave the window's size to the system that just set it.
                    self.phase = Phase::Off;
                    vec![Effect::ShowVisualizer(before.show_visualizer)]
                } else {
                    Vec::new()
                }
            }
            Phase::Restoring { before, asked_at } => {
                if window_full_screen == Some(true) && now - asked_at < RESTORE_WAIT_SECONDS {
                    return Vec::new();
                }
                self.phase = Phase::Off;
                restore(&before)
            }
        }
    }
}

/// Puts back the size and place the window had.
fn restore(before: &WindowBefore) -> Vec<Effect> {
    if before.maximized {
        return vec![Effect::Maximize];
    }
    before.position.map(Effect::MoveTo).into_iter().chain(before.inner_size.map(Effect::Resize)).collect()
}

/// Commands that work without leaving full screen: listening keeps going while the screen is
/// the visualizer's. Anything else leaves full screen first, so its result can be seen.
pub fn keeps_full_screen(id: CommandId) -> bool {
    matches!(
        id,
        CommandId::PlayPause
            | CommandId::NextTrack
            | CommandId::PreviousTrack
            | CommandId::VolumeUp
            | CommandId::VolumeDown
            | CommandId::Shuffle
            | CommandId::Repeat
            | CommandId::Quit
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn window(show_visualizer: bool) -> WindowBefore {
        WindowBefore {
            position: Some(pos2(120.0, 80.0)),
            inner_size: Some(vec2(1280.0, 780.0)),
            maximized: false,
            full_screen: false,
            show_visualizer,
        }
    }

    #[test]
    fn leaving_puts_the_window_and_view_back() {
        let mut full = FullScreen::default();
        assert_eq!(full.enter(window(false)), [Effect::ShowVisualizer(true), Effect::SetFullScreen(true)]);
        assert!(full.is_on());
        assert!(full.update(Some(true), 0.5).is_empty());
        assert_eq!(full.leave(3.0), [Effect::ShowVisualizer(false), Effect::SetFullScreen(false)]);
        assert!(!full.is_on());
        // Not yet out of full screen: the size would be overwritten.
        assert!(full.update(Some(true), 3.1).is_empty());
        assert_eq!(
            full.update(Some(false), 3.2),
            [Effect::MoveTo(pos2(120.0, 80.0)), Effect::Resize(vec2(1280.0, 780.0))]
        );
        assert!(full.update(Some(false), 3.3).is_empty());
    }

    #[test]
    fn a_maximized_window_is_maximized_again() {
        let mut full = FullScreen::default();
        full.enter(WindowBefore { maximized: true, ..window(true) });
        assert_eq!(full.leave(1.0), [Effect::ShowVisualizer(true), Effect::SetFullScreen(false)]);
        assert_eq!(full.update(None, 1.1), [Effect::Maximize]);
    }

    #[test]
    fn restores_anyway_if_the_system_is_slow_to_leave() {
        let mut full = FullScreen::default();
        full.enter(window(false));
        full.leave(5.0);
        assert!(full.update(Some(true), 5.5).is_empty());
        assert_eq!(full.update(Some(true), 6.0).len(), 2);
    }

    #[test]
    fn a_refused_full_screen_still_fills_the_window() {
        let mut full = FullScreen::default();
        full.enter(window(false));
        for frame in 0..100 {
            assert!(full.update(Some(false), f64::from(frame) / 60.0).is_empty());
        }
        assert!(full.is_on());
    }

    #[test]
    fn follows_the_system_out_of_full_screen() {
        let mut full = FullScreen::default();
        full.enter(window(true));
        full.update(Some(true), 0.2);
        assert_eq!(full.update(Some(false), 4.0), [Effect::ShowVisualizer(true)]);
        assert!(!full.is_on());
    }

    #[test]
    fn a_window_already_full_screen_stays_so() {
        let mut full = FullScreen::default();
        assert_eq!(full.enter(WindowBefore { full_screen: true, ..window(false) }), [Effect::ShowVisualizer(true)]);
        assert_eq!(full.leave(2.0), [Effect::ShowVisualizer(false)]);
        assert!(full.update(Some(true), 2.1).is_empty());
    }

    #[test]
    fn entering_again_while_restoring_remembers_the_first_window() {
        let mut full = FullScreen::default();
        full.enter(window(false));
        full.leave(1.0);
        let now = WindowBefore { inner_size: Some(vec2(1920.0, 1080.0)), show_visualizer: true, ..window(true) };
        full.enter(now);
        full.leave(2.0);
        assert_eq!(
            full.update(Some(false), 2.1),
            [Effect::MoveTo(pos2(120.0, 80.0)), Effect::Resize(vec2(1280.0, 780.0))]
        );
    }

    #[test]
    fn listening_commands_keep_full_screen() {
        assert!(keeps_full_screen(CommandId::PlayPause));
        assert!(keeps_full_screen(CommandId::VolumeDown));
        assert!(!keeps_full_screen(CommandId::Find));
        assert!(!keeps_full_screen(CommandId::FullScreenVisualizer));
    }

    #[test]
    fn reads_the_window_from_the_viewport() {
        let viewport = ViewportInfo {
            outer_rect: Some(egui::Rect::from_min_size(pos2(10.0, 20.0), vec2(900.0, 640.0))),
            inner_rect: Some(egui::Rect::from_min_size(pos2(10.0, 50.0), vec2(900.0, 610.0))),
            maximized: Some(false),
            fullscreen: None,
            ..ViewportInfo::default()
        };
        let before = WindowBefore::from_viewport(&viewport, true);
        assert_eq!(before.position, Some(pos2(10.0, 20.0)));
        assert_eq!(before.inner_size, Some(vec2(900.0, 610.0)));
        assert!(!before.maximized && !before.full_screen && before.show_visualizer);
    }
}
