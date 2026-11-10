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
        let result = window(ctx, "Preferences", 560.0, "OK", |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(tab, Tab::General, "General");
                ui.selectable_value(tab, Tab::Importing, "Importing");
                ui.selectable_value(tab, Tab::Columns, "Columns");
            });
            ui.separator();
            ui.add_space(4.0);
            match tab {
                Tab::General => general(ui, draft, actions),
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

fn general(ui: &mut egui::Ui, prefs: &mut Preferences, actions: &mut Vec<Action>) {
    music_folder(ui, prefs, actions);
    ui.separator();
    ui.label(RichText::new("TuneBox Media folder location").strong());
    ui.horizontal(|ui| {
        ui.label(tunebox_platform::display_path(&prefs.media_folder));
        if ui.button("Change…").clicked()
            && let Some(folder) = rfd::FileDialog::new().set_directory(&prefs.media_folder).pick_folder()
        {
            prefs.media_folder = folder;
        }
    });
    ui.add_space(6.0);
    ui.checkbox(&mut prefs.copy_to_media_folder, "Copy files to the TuneBox Media folder when adding to library");
    ui.label(
        RichText::new("Off: songs play from where they are and nothing is added to your folders. On: copies are filed by artist and album. CD imports always go to the media folder.")
            .weak()
            .size(11.0),
    );
    if prefs.music_folder_read_only && prefs.music_folder.as_ref().is_some_and(|m| prefs.media_folder.starts_with(m)) {
        ui.colored_label(
            egui::Color32::from_rgb(0xB0, 0x30, 0x30),
            "The media folder is inside your read-only music folder, so CD imports and copies are not possible. Choose another media folder.",
        );
    }
}

/// The listener's own music folder, and protecting it.
fn music_folder(ui: &mut egui::Ui, prefs: &mut Preferences, actions: &mut Vec<Action>) {
    ui.label(RichText::new("Your music folder").strong());
    ui.horizontal(|ui| {
        match &prefs.music_folder {
            Some(folder) => ui.label(tunebox_platform::display_path(folder)),
            None => ui.label(RichText::new("Not chosen").weak()),
        };
        if ui.button("Choose…").clicked()
            && let Some(folder) = rfd::FileDialog::new().set_title("Your Music Folder").pick_folder()
        {
            prefs.music_folder = Some(folder);
        }
        if let Some(folder) = prefs.music_folder.clone() {
            if ui.button("Add to Library").on_hover_text("Adds every song in the folder, in place").clicked() {
                actions.push(Action::AddToLibrary(vec![folder]));
            }
            if ui.button("Forget").clicked() {
                prefs.music_folder = None;
            }
        }
    });
    ui.add_enabled_ui(prefs.music_folder.is_some(), |ui| {
        ui.checkbox(
            &mut prefs.music_folder_read_only,
            "Read only: never change, move or delete anything in this folder",
        );
    });
    ui.label(
        RichText::new("Edits to songs in a read-only folder are kept in TuneBox's library; deleting a song only removes it from the library.")
            .weak()
            .size(11.0),
    );
    ui.add_space(4.0);
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
