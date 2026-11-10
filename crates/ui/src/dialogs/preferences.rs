//! Preferences: the media folder, importing, and which columns the track list shows.

use crate::actions::Action;
use crate::dialogs::window;
use egui::RichText;
use tunebox_engine::model::Column;
use tunebox_engine::{LosslessFormat, Preferences};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    General,
    Importing,
    Columns,
}

#[derive(Clone, Debug)]
pub struct PreferencesDialog {
    tab: Tab,
    draft: Preferences,
}

impl PreferencesDialog {
    pub fn new(current: &Preferences) -> Self {
        Self { tab: Tab::General, draft: current.clone() }
    }

    pub fn show(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let (tab, draft) = (&mut self.tab, &mut self.draft);
        let result = window(ctx, "Preferences", 520.0, "OK", |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(tab, Tab::General, "General");
                ui.selectable_value(tab, Tab::Importing, "Importing");
                ui.selectable_value(tab, Tab::Columns, "Columns");
            });
            ui.separator();
            ui.add_space(4.0);
            match tab {
                Tab::General => general(ui, draft),
                Tab::Importing => importing(ui, draft),
                Tab::Columns => columns(ui, draft),
            }
        });
        match result {
            Some(true) => {
                actions.push(Action::SetPreferences(Box::new(self.draft.clone())));
                false
            }
            Some(false) => false,
            None => true,
        }
    }
}

fn general(ui: &mut egui::Ui, prefs: &mut Preferences) {
    ui.label(RichText::new("TuneBox Media folder location").strong());
    ui.horizontal(|ui| {
        ui.label(prefs.media_folder.display().to_string());
        if ui.button("Change…").clicked()
            && let Some(folder) = rfd::FileDialog::new().set_directory(&prefs.media_folder).pick_folder()
        {
            prefs.media_folder = folder;
        }
    });
    ui.add_space(6.0);
    ui.checkbox(&mut prefs.copy_to_media_folder, "Copy files to the TuneBox Media folder when adding to library");
    ui.label(
        RichText::new("Copies are filed by artist and album. Unchecked, songs stay where they are.").weak().size(11.0),
    );
}

fn importing(ui: &mut egui::Ui, prefs: &mut Preferences) {
    ui.horizontal(|ui| {
        ui.label("Import CDs using:");
        egui::ComboBox::from_id_salt("rip-format").selected_text(prefs.rip_format.encoder_name()).show_ui(ui, |ui| {
            for format in LosslessFormat::ALL {
                ui.selectable_value(&mut prefs.rip_format, format, format.encoder_name());
            }
        });
    });
    ui.label(
        RichText::new("All three are lossless: every bit on the disc is kept. FLAC files are about half the size.")
            .weak()
            .size(11.0),
    );
    ui.add_space(8.0);
    ui.checkbox(&mut prefs.lookup_cd_names_automatically, "Automatically get CD track names from the Internet");
    ui.label(RichText::new("Names come from MusicBrainz. Only the disc's identifier is sent.").weak().size(11.0));
}

fn columns(ui: &mut egui::Ui, prefs: &mut Preferences) {
    ui.label("Show these columns in the song list:");
    ui.add_space(4.0);
    egui::Grid::new("columns").num_columns(3).spacing([18.0, 4.0]).show(ui, |ui| {
        for (i, column) in Column::ALL.iter().enumerate() {
            let mut on = prefs.visible_columns.contains(column);
            let enabled = *column != Column::Name;
            if ui.add_enabled(enabled, egui::Checkbox::new(&mut on, column.title())).changed() {
                if on {
                    // Keep the registry's order so columns appear in a familiar sequence.
                    prefs.visible_columns.push(*column);
                    prefs.visible_columns.sort_by_key(|c| Column::ALL.iter().position(|x| x == c));
                } else {
                    prefs.visible_columns.retain(|c| c != column);
                }
            }
            if i % 3 == 2 {
                ui.end_row();
            }
        }
    });
}
