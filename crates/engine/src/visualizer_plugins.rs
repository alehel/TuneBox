//! Visualizer plug-ins: scanning the `Visualizers` folder on a worker thread and handing the
//! loaded plug-ins to the frontend.
//!
//! Loading validates and compiles each module, which can take a moment for a large one, so it
//! runs as a job. Files that are not usable plug-ins are reported once per scan, as notices.
//! The session only carries the plug-ins across; the frontend that paints them owns them (see
//! `tunebox_visualizer`), and turns off any that faults while drawing.

use crate::jobs::JobKind;
use crate::notices::Notice;
use crate::session::{Outcome, Session};
use std::path::Path;
use tunebox_plugins::{FolderReport, Plugin, load_folder};

impl Session {
    /// The folder plug-ins are loaded from.
    pub fn visualizer_plugins_folder(&self) -> &Path {
        &self.paths.visualizer_plugins_folder
    }

    /// Makes sure the plug-ins folder exists (it is TuneBox's own, in its data folder), so it
    /// can be shown to the listener to put plug-ins in.
    pub fn create_visualizer_plugins_folder(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.paths.visualizer_plugins_folder)
    }

    /// Loads (or reloads) the plug-ins in [`Session::visualizer_plugins_folder`] in the
    /// background; [`Session::take_visualizer_plugins`] hands them over when done. With
    /// `announce`, says how many were found (for a reload the listener asked for).
    pub fn reload_visualizer_plugins(&mut self, announce: bool) {
        if self.jobs.is_running(JobKind::VisualizerPlugins) {
            return;
        }
        let folder = self.paths.visualizer_plugins_folder.clone();
        self.jobs.spawn(JobKind::VisualizerPlugins, "Loading visualizer plug-ins".into(), move |_| {
            Outcome::VisualizerPlugins { report: load_folder(&folder), announce }
        });
    }

    /// The plug-ins the last finished scan loaded, once (`None` until a scan finishes).
    pub fn take_visualizer_plugins(&mut self) -> Option<Vec<Plugin>> {
        self.loaded_visualizer_plugins.take()
    }

    pub(crate) fn finish_visualizer_plugins(&mut self, report: FolderReport, announce: bool) {
        for (file, problem) in &report.problems {
            self.notices
                .push(Notice::warning(format!("“{file}” in the Visualizers folder was not loaded: {problem}.")));
        }
        if announce {
            let text = match report.plugins.len() {
                0 => "No visualizer plug-ins were found.".to_owned(),
                1 => "1 visualizer plug-in was loaded.".to_owned(),
                n => format!("{n} visualizer plug-ins were loaded."),
            };
            self.notices.push(Notice::info(text));
        }
        self.loaded_visualizer_plugins = Some(report.plugins);
    }
}
