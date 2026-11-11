//! The looks to choose between: built-in and compiled-in looks first, then plug-ins.
//!
//! The registry paints the chosen look. When the choice is unknown (a look that was removed,
//! a plug-in that is gone) it paints the first built-in look instead, leaving the preference
//! alone so the look returns if its plug-in does. A plug-in that faults is taken out for the
//! rest of the session and the frame is painted by the first built-in look.

use crate::{Frame, PluginLook, Visualizer, looks};
use egui::{Painter, Rect};
use tunebox_engine::plugins::Plugin;

/// One look as the picker shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookEntry {
    pub id: String,
    pub name: String,
    /// Loaded at run time from the Visualizers folder.
    pub plugin: bool,
}

/// What [`Registry::paint`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Painted {
    /// The id of the look painted.
    pub shown: String,
    /// When the chosen plug-in failed this frame and was turned off: the notice for the listener.
    pub turned_off: Option<String>,
}

/// The looks to choose between, in picker order.
pub struct Registry {
    looks: Vec<Box<dyn Visualizer>>,
    plugins: Vec<PluginLook>,
}

impl Registry {
    /// The built-in looks.
    pub fn with_builtins() -> Self {
        Self { looks: looks::builtin(), plugins: Vec::new() }
    }

    /// Adds a look, replacing any with the same id.
    pub fn add(&mut self, look: Box<dyn Visualizer>) {
        match self.looks.iter_mut().find(|l| l.id() == look.id()) {
            Some(slot) => *slot = look,
            None => self.looks.push(look),
        }
    }

    /// Replaces the plug-in looks (a fresh scan of the Visualizers folder).
    pub fn set_plugins(&mut self, plugins: Vec<Plugin>) {
        self.plugins = plugins.into_iter().map(PluginLook::new).collect();
    }

    /// Every look, in picker order: built-in and compiled-in looks, then plug-ins.
    pub fn entries(&self) -> Vec<LookEntry> {
        let entry =
            |look: &dyn Visualizer, plugin| LookEntry { id: look.id().into(), name: look.name().into(), plugin };
        let looks = self.looks.iter().map(|l| entry(l.as_ref(), false));
        looks.chain(self.plugins.iter().map(|p| entry(p, true))).collect()
    }

    /// Paints the look with `id` (the first look when there is none) filling `rect`.
    pub fn paint(&mut self, id: &str, painter: &Painter, rect: Rect, frame: &Frame) -> Painted {
        let mut turned_off = None;
        if let Some(index) = self.plugins.iter().position(|p| p.id() == id) {
            let plugin = &mut self.plugins[index];
            plugin.paint(painter, rect, frame);
            match plugin.fault() {
                None => return Painted { shown: id.to_owned(), turned_off: None },
                Some(fault) => {
                    turned_off = Some(fault.notice(plugin.name()));
                    self.plugins.remove(index);
                }
            }
        }
        let index = self.looks.iter().position(|l| l.id() == id).unwrap_or(0);
        let Some(look) = self.looks.get_mut(index) else { return Painted { shown: String::new(), turned_off } };
        look.paint(painter, rect, frame);
        Painted { shown: look.id().to_owned(), turned_off }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BANDS;

    struct Blank;

    impl Visualizer for Blank {
        fn id(&self) -> &str {
            "spectrum"
        }
        fn name(&self) -> &str {
            "Blank"
        }
        fn paint(&mut self, _: &Painter, _: Rect, _: &Frame) {}
    }

    /// Paints with every look in `ids` (one frame each, real-looking data then empty data) and
    /// returns what each paint reported.
    fn paint_each(registry: &mut Registry, ids: &[String]) -> Vec<Painted> {
        let ctx = egui::Context::default();
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
        let mut painted = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
            for id in ids {
                painted.push(registry.paint(id, ui.painter(), rect, &frame));
                painted.push(registry.paint(id, ui.painter(), rect, &empty));
            }
        });
        output.textures_delta.clear();
        painted
    }

    fn ids(registry: &Registry) -> Vec<String> {
        registry.entries().into_iter().map(|e| e.id).collect()
    }

    #[test]
    fn builtin_ids_are_unique_and_an_unknown_id_paints_the_first() {
        let mut registry = Registry::with_builtins();
        let mut unique = ids(&registry);
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), registry.entries().len());
        let painted = paint_each(&mut registry, &["no such look".into()]);
        assert_eq!(painted[0], Painted { shown: "spectrum".into(), turned_off: None });
    }

    #[test]
    fn adding_a_look_with_a_known_id_replaces_it() {
        let mut registry = Registry::with_builtins();
        let count = registry.entries().len();
        registry.add(Box::new(Blank));
        assert_eq!(registry.entries().len(), count);
        assert_eq!(registry.entries()[0].name, "Blank");
    }

    #[test]
    fn every_builtin_paints_without_panicking() {
        let mut registry = Registry::with_builtins();
        let all = ids(&registry);
        let painted = paint_each(&mut registry, &all);
        let shown: Vec<String> = painted.iter().step_by(2).map(|p| p.shown.clone()).collect();
        assert_eq!(shown, all);
    }

    /// A plug-in whose draw is `draw`: it fills the whole visualizer, or loops forever.
    fn plugin(draw: &str) -> Plugin {
        let text = format!(
            r#"(module
                (memory (export "memory") 1)
                (data (i32.const 100) "\c8\00\00\00\04\00\00\00\2c\01\00\00\04\00\00\00")
                (data (i32.const 200) "fill")
                (data (i32.const 300) "Fill")
                (func (export "tunebox_abi_version") (result i32) i32.const 1)
                (func (export "tunebox_info") (result i32) i32.const 100)
                (func (export "tunebox_frame_buffer") (result i32) i32.const 1024)
                (func (export "tunebox_draw") (param $w f32) (param $h f32) (result i32) {draw}))"#
        );
        Plugin::load(&wat::parse_str(text).unwrap(), "fill.wasm").unwrap()
    }

    const FILL: &str = r#"
        (i32.store (i32.const 8192) (i32.const 28))
        (i32.store (i32.const 8196) (i32.const 1))
        (f32.store (i32.const 8208) (local.get $w))
        (f32.store (i32.const 8212) (local.get $h))
        (i32.store (i32.const 8220) (i32.const 0xffffffff))
        (i32.const 8192)"#;

    #[test]
    fn plugins_come_after_the_builtins_and_paint() {
        let mut registry = Registry::with_builtins();
        registry.set_plugins(vec![plugin(FILL)]);
        let entries = registry.entries();
        let last = entries.last().unwrap();
        assert_eq!((last.id.as_str(), last.name.as_str(), last.plugin), ("plugin:fill", "Fill", true));
        assert!(entries.iter().filter(|e| e.plugin).count() == 1);
        let painted = paint_each(&mut registry, &["plugin:fill".into()]);
        assert!(painted.iter().all(|p| *p == Painted { shown: "plugin:fill".into(), turned_off: None }));
    }

    #[test]
    fn a_failing_plugin_is_turned_off_once_and_a_builtin_paints_instead() {
        let mut registry = Registry::with_builtins();
        registry.set_plugins(vec![plugin("(loop $forever (br $forever)) (i32.const 0)")]);
        let painted = paint_each(&mut registry, &["plugin:fill".into()]);
        assert_eq!(painted[0].shown, "spectrum");
        assert_eq!(
            painted[0].turned_off.as_deref(),
            Some("The “Fill” visualizer stopped working and was turned off: it took too long to draw a frame.")
        );
        // Gone from the picker; later frames paint the built-in without another notice.
        assert_eq!(painted[1], Painted { shown: "spectrum".into(), turned_off: None });
        assert!(registry.entries().iter().all(|e| !e.plugin));
    }
}
