//! A plug-in as a look: each frame it hands the plug-in the audio, takes back a drawing and
//! paints it. A fault is kept, not painted over, so the registry can turn the plug-in off.

use crate::{Frame, Visualizer, plugin_shapes};
use egui::{Painter, Rect};
use tunebox_engine::plugins::{Fault, FrameData, Plugin};

/// What plug-in look ids start with in the picker and in preferences, so a plug-in can never
/// take the place of a built-in look (or of one added later).
pub const PLUGIN_ID_PREFIX: &str = "plugin:";

/// A loaded plug-in, painting.
pub struct PluginLook {
    plugin: Plugin,
    /// [`PLUGIN_ID_PREFIX`] and the plug-in's own id.
    id: String,
    /// Why it stopped, once it has.
    fault: Option<Fault>,
}

impl PluginLook {
    pub fn new(plugin: Plugin) -> Self {
        let id = format!("{PLUGIN_ID_PREFIX}{}", plugin.info().id);
        Self { plugin, id, fault: None }
    }

    /// Why the plug-in stopped, if it has. It paints nothing from then on.
    pub fn fault(&self) -> Option<&Fault> {
        self.fault.as_ref()
    }

    /// The `.wasm` file it came from.
    pub fn file(&self) -> &str {
        self.plugin.file()
    }
}

impl Visualizer for PluginLook {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.plugin.info().name
    }

    fn paint(&mut self, painter: &Painter, rect: Rect, frame: &Frame) {
        if self.fault.is_some() {
            return;
        }
        let data = FrameData {
            bands: frame.bands,
            peaks: frame.peaks,
            level: frame.level,
            bass: frame.bass,
            waveforms: frame.waveforms,
            time: frame.time,
            dt: frame.dt,
            playing: frame.playing,
        };
        match self.plugin.draw(&data, rect.width(), rect.height()) {
            Ok(commands) => {
                painter.extend(plugin_shapes::to_shapes(commands, rect.min));
            }
            Err(fault) => {
                log::warn!("visualizer plug-in {} stopped: {fault}", self.plugin.file());
                self.fault = Some(fault);
            }
        }
    }
}
