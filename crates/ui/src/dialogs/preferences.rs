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
    library_folders(ui, prefs, actions);
    ui.separator();
    ui.label(RichText::new("Imports folder (CD imports and copies)").strong());
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
    if !tunebox_engine::files::may_write(prefs, &prefs.media_folder) {
        ui.colored_label(
            egui::Color32::from_rgb(0xB0, 0x30, 0x30),
            "The media folder is in a read-only library folder, so CD imports and copies are not possible. Choose a writable folder.",
        );
    }
}

/// The listener's music folders, each read-only or writable.
fn library_folders(ui: &mut egui::Ui, prefs: &mut Preferences, actions: &mut Vec<Action>) {
    ui.label(RichText::new("Library folders").strong());
    let mut remove = None;
    let mut use_for_imports = None;
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(0xB0)))
        .inner_margin(6)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if prefs.library_folders.is_empty() {
                ui.label(RichText::new("No folders yet. Songs you add are played from wherever they are.").weak());
            }
            egui::Grid::new("library-folders").num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
                for (index, folder) in prefs.library_folders.iter_mut().enumerate() {
                    ui.label(tunebox_platform::display_path(&folder.path));
                    ui.checkbox(&mut folder.read_only, "Read only");
                    let is_imports = folder.path == prefs.media_folder;
                    if is_imports {
                        ui.label(RichText::new("Imports go here").weak());
                    } else if ui.add_enabled(!folder.read_only, egui::Button::new("Use for Imports")).clicked() {
                        use_for_imports = Some(folder.path.clone());
                    }
                    if ui
                        .small_button("Remove")
                        .on_hover_text("Stop listing it; its songs stay in the library")
                        .clicked()
                    {
                        remove = Some(index);
                    }
                    ui.end_row();
                }
            });
        });
    if let Some(index) = remove {
        prefs.library_folders.remove(index);
    }
    if let Some(path) = use_for_imports {
        prefs.media_folder = path;
    }
    ui.horizontal(|ui| {
        if ui.button("Add Folder…").clicked()
            && let Some(folder) = rfd::FileDialog::new().set_title("Add a Library Folder").pick_folder()
        {
            prefs.add_library_folder(folder.clone(), true);
            actions.push(Action::AddToLibrary(vec![folder]));
        }
        if ui
            .add_enabled(!prefs.library_folders.is_empty(), egui::Button::new("Scan All"))
            .on_hover_text("Add songs that appeared since the last scan")
            .clicked()
        {
            actions.push(Action::AddToLibrary(prefs.library_folders.iter().map(|f| f.path.clone()).collect()));
        }
    });
    ui.label(
        RichText::new("New folders are read only. TuneBox never changes, moves or deletes anything in a read-only folder: edits stay in its library, and deleting only removes songs from the library. A folder inside another follows its own setting.")
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
