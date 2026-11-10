//! Preferences: four panes (Library, Importing, Columns, Get Info) chosen from an icon tab bar. Each
//! pane is a column of cards: a bold heading, one line saying what the card is for, then its
//! settings, with on/off settings as switches that carry their explanation beneath them.

use crate::actions::Action;
use crate::dialogs::layout::{Tone, banner, card, muted, switch_row, value_row};
use crate::dialogs::{layout, window};
use crate::glyphs::{self, Glyph};
use crate::panels::widgets;
use crate::theme;
use egui::{Align, Color32, Frame, Layout, Rect, Sense, Stroke, Ui, pos2, vec2};
use tunebox_engine::model::{Column, FieldGroup};
use tunebox_engine::{DriveIdentity, LibraryFolder, LosslessFormat, Preferences};

const WIDTH: f32 = 640.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Library,
    Importing,
    Columns,
    GetInfo,
}

#[derive(Clone, Debug)]
pub struct PreferencesDialog {
    tab: Tab,
    draft: Preferences,
    /// The drive holding a disc, whose read offset the Importing pane edits.
    drive: Option<DriveIdentity>,
    /// The read offset as typed, checked before it is saved.
    offset_text: String,
}

impl PreferencesDialog {
    pub fn new(current: &Preferences, drive: Option<DriveIdentity>) -> Self {
        Self::on_tab(current, drive, Tab::Library)
    }

    pub fn on_tab(current: &Preferences, drive: Option<DriveIdentity>, tab: Tab) -> Self {
        let offset = drive.as_ref().map(|d| current.read_offset(d.key())).unwrap_or(0);
        Self { tab, draft: current.clone(), drive, offset_text: format!("{offset:+}") }
    }

    pub fn show(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let (tab, draft, drive, offset_text) = (&mut self.tab, &mut self.draft, &self.drive, &mut self.offset_text);
        let mut offset = Ok(0);
        let result = window(ctx, "Preferences", WIDTH, "OK", |ui| {
            layout::tab_bar(ui, tab, &TABS);
            offset = parse_offset(offset_text);
            layout::scrolling_body(ui, |ui| match tab {
                Tab::Library => library(ui, draft, actions),
                Tab::Importing => importing(ui, draft, drive.as_ref(), offset_text),
                Tab::Columns => columns(ui, draft),
                Tab::GetInfo => get_info_fields(ui, draft),
            });
        });
        match result {
            // An offset no drive has: stay open on the Importing pane, which says why.
            Some(true) if offset.is_err() => {
                self.tab = Tab::Importing;
                true
            }
            Some(true) => {
                if let (Some(drive), Ok(offset)) = (&self.drive, offset) {
                    match offset {
                        0 => self.draft.read_offsets.remove(drive.key()),
                        n => self.draft.read_offsets.insert(drive.key().to_string(), n),
                    };
                }
                actions.push(Action::SetPreferences(Box::new(self.draft.clone())));
                false
            }
            Some(false) => false,
            None => true,
        }
    }
}

// ----- tabs -----

const TABS: [(Tab, Glyph, &str); 4] = [
    (Tab::Library, Glyph::Folder, "Library"),
    (Tab::Importing, Glyph::Disc, "Importing"),
    (Tab::Columns, Glyph::ListView, "Columns"),
    (Tab::GetInfo, Glyph::Info, "Get Info"),
];

// ----- the panes -----

fn library(ui: &mut Ui, prefs: &mut Preferences, actions: &mut Vec<Action>) {
    card(
        ui,
        "Library folders",
        "Where your music lives. TuneBox never changes, moves or deletes anything in a read-only folder; a folder inside another follows its own setting.",
        |ui| folder_list(ui, prefs, actions),
    );
    card(ui, "Adding music", "", |ui| {
        switch_row(
            ui,
            "Watch library folders for changes",
            "Songs that appear in these folders are added, and songs another program changes are read again, as it happens. Watching only reads; deleted songs stay in the library.",
            &mut prefs.watch_library_folders,
        );
        ui.add_space(10.0);
        switch_row(
            ui,
            "Copy files to the media folder when adding to the library",
            "Off: songs play from where they are and nothing is added to your folders. On: copies are filed by artist and album in the imports folder.",
            &mut prefs.copy_to_media_folder,
        );
    });
}

/// The library folders as a list: one row each, then Add and Scan below.
fn folder_list(ui: &mut Ui, prefs: &mut Preferences, actions: &mut Vec<Action>) {
    let mut remove = None;
    let mut use_for_imports = None;
    Frame::new()
        .fill(Color32::from_rgb(0xF7, 0xF8, 0xFA))
        .stroke(Stroke::new(1.0, theme::GRID_LINE))
        .corner_radius(8)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if prefs.library_folders.is_empty() {
                ui.add_space(14.0);
                ui.vertical_centered(|ui| {
                    ui.label(muted("No folders yet. Songs you add are played from wherever they are."))
                });
                ui.add_space(14.0);
            }
            let count = prefs.library_folders.len();
            let imports = prefs.media_folder.clone();
            for (index, folder) in prefs.library_folders.iter_mut().enumerate() {
                match folder_row(ui, index, folder, folder.path == imports) {
                    RowAction::Remove => remove = Some(index),
                    RowAction::UseForImports => use_for_imports = Some(folder.path.clone()),
                    RowAction::None => {}
                }
                if index + 1 < count {
                    let rect = ui.available_rect_before_wrap();
                    ui.painter().hline(rect.x_range().shrink(12.0), rect.top(), Stroke::new(1.0, theme::GRID_LINE));
                }
            }
        });
    if let Some(index) = remove {
        prefs.library_folders.remove(index);
    }
    if let Some(path) = use_for_imports {
        prefs.media_folder = path;
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("+  Add Folder…").clicked()
            && let Some(folder) = rfd::FileDialog::new().set_title("Add a Library Folder").pick_folder()
        {
            prefs.add_library_folder(folder.clone(), true);
            actions.push(Action::AddToLibrary(vec![folder]));
        }
        let scan = ui.add_enabled(!prefs.library_folders.is_empty(), egui::Button::new("Scan All"));
        if scan.on_hover_text("Add songs that appeared in these folders since the last scan").clicked() {
            actions.push(Action::AddToLibrary(prefs.library_folders.iter().map(|f| f.path.clone()).collect()));
        }
    });
}

enum RowAction {
    None,
    Remove,
    UseForImports,
}

/// One folder: icon, name and path, its rule, the imports badge, and remove.
fn folder_row(ui: &mut Ui, index: usize, folder: &mut LibraryFolder, is_imports: bool) -> RowAction {
    let mut action = RowAction::None;
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::hover());
    let painter = ui.painter();
    let icon = Rect::from_center_size(pos2(row.left() + 26.0, row.center().y), vec2(24.0, 24.0));
    glyphs::paint(
        painter,
        icon,
        Glyph::Folder,
        if folder.read_only { theme::ICON_PLAYLIST } else { theme::ICON_LIBRARY },
    );
    if folder.read_only {
        let badge = Rect::from_center_size(icon.right_bottom() - vec2(3.0, 4.0), vec2(12.0, 12.0));
        painter.circle_filled(badge.center(), 7.5, Color32::WHITE);
        glyphs::paint(painter, badge.shrink(1.5), Glyph::Lock, theme::GLYPH);
    }
    let name = folder
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| folder.path.display().to_string());
    let text_left = icon.right() + 14.0;
    let text_width = 210.0;
    theme::font_in_row(
        painter,
        pos2(text_left, row.center().y - 8.0),
        &name,
        crate::fonts::bold(13.0),
        theme::TEXT,
        text_width,
    );
    let path = tunebox_platform::display_path(&folder.path);
    theme::text_in_row(painter, pos2(text_left, row.center().y + 9.0), &path, 11.5, theme::TEXT_MUTED, text_width);

    let controls = Rect::from_min_max(pos2(text_left + text_width + 8.0, row.top()), row.max - vec2(10.0, 0.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(controls).layout(Layout::right_to_left(Align::Center)), |ui| {
        let remove = remove_button(ui).on_hover_text("Remove from the list (the songs stay in the library)");
        if remove.clicked() {
            action = RowAction::Remove;
        }
        ui.add_space(4.0);
        if is_imports {
            imports_badge(ui);
        } else if !folder.read_only && ui.link("Use for imports").clicked() {
            action = RowAction::UseForImports;
        }
        ui.add_space(6.0);
        let mut choice = usize::from(!folder.read_only);
        if widgets::segmented(ui, ("folder-rule", index), &["Read only", "Writable"], &mut choice) {
            folder.read_only = choice == 0;
        }
    });
    action
}

/// A small round button with a cross, darkening under the pointer.
fn remove_button(ui: &mut Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.circle_filled(rect.center(), 11.0, Color32::from_black_alpha(14));
    }
    let color = if response.hovered() { theme::GLYPH_PRESSED } else { theme::TEXT_MUTED };
    let stroke = Stroke::new(1.5, color);
    let c = rect.center();
    painter.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], stroke);
    painter.line_segment([c + vec2(4.0, -4.0), c + vec2(-4.0, 4.0)], stroke);
    response
}

fn imports_badge(ui: &mut Ui) {
    let font = egui::FontId::proportional(11.0);
    let text = "Imports go here";
    let width = ui.painter().layout_no_wrap(text.into(), font.clone(), theme::ACCENT).size().x + 16.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
    ui.painter().rect_filled(rect, 10.0, theme::ACCENT.gamma_multiply(0.12));
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, font, theme::ACCENT);
}

/// The read offset as typed: "+667", "667", "-1164"; blank means 0.
fn parse_offset(text: &str) -> Result<i32, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(0);
    }
    let number: i64 = text
        .strip_prefix('+')
        .unwrap_or(text)
        .parse()
        .map_err(|_| format!("“{text}” is not a number of samples. Type a whole number such as +667 or -1164."))?;
    tunebox_engine::check_read_offset(number)
}

fn importing(ui: &mut Ui, prefs: &mut Preferences, drive: Option<&DriveIdentity>, offset_text: &mut String) {
    card(ui, "CD import", "Every format here is lossless: every bit on the disc is kept.", |ui| {
        value_row(ui, "Import using", |ui| {
            egui::ComboBox::from_id_salt("rip-format")
                .width(200.0)
                .selected_text(prefs.rip_format.encoder_name())
                .show_ui(ui, |ui| {
                    for format in LosslessFormat::ALL {
                        ui.selectable_value(&mut prefs.rip_format, format, format.encoder_name());
                    }
                });
            ui.label(muted(match prefs.rip_format {
                LosslessFormat::Flac => "About half the size of WAV",
                LosslessFormat::Wav | LosslessFormat::Aiff => "Uncompressed",
            }));
        });
        ui.add_space(6.0);
        value_row(ui, "Imports folder", |ui| {
            ui.label(tunebox_platform::display_path(&prefs.media_folder));
            if ui.button("Change…").clicked()
                && let Some(folder) = rfd::FileDialog::new().set_directory(&prefs.media_folder).pick_folder()
            {
                prefs.media_folder = folder;
            }
        });
        if !tunebox_engine::files::may_write(prefs, &prefs.media_folder) {
            ui.add_space(6.0);
            banner(
                ui,
                Tone::Warning,
                Glyph::Lock,
                "This folder is in a read-only library folder, so CDs cannot be imported there. Choose a writable folder.",
            );
        }
    });
    card(ui, "Track names", "", |ui| {
        switch_row(
            ui,
            "Look up track names when a disc is inserted",
            "From MusicBrainz. Only the disc's identifier is sent, and only when this is on or you choose Get Track Names.",
            &mut prefs.lookup_cd_names_automatically,
        );
    });
    card(
        ui,
        "Drive read offset",
        "Every CD drive reads audio a fixed number of samples early or late. Enter your drive's offset and TuneBox corrects it, so imports match the disc sample for sample. If your drive is not in the list, leave it at 0.",
        |ui| read_offset(ui, drive, offset_text),
    );
}

/// The read offset of the drive in use, with where to look it up.
fn read_offset(ui: &mut Ui, drive: Option<&DriveIdentity>, offset_text: &mut String) {
    let Some(drive) = drive else {
        ui.label(muted("Insert a CD to set the read offset of the drive it is in."));
        ui.add_space(4.0);
        ui.hyperlink_to("Look up drive offsets (AccurateRip drive list)", DRIVE_OFFSET_LIST);
        return;
    };
    value_row(ui, "Read offset", |ui| {
        ui.add(egui::TextEdit::singleline(offset_text).desired_width(70.0));
        ui.label(muted("samples"));
        ui.hyperlink_to("Look up your drive", DRIVE_OFFSET_LIST);
    });
    ui.add_space(4.0);
    ui.label(muted(&format!("Saved for {}.", drive.describe())));
    if let Err(problem) = parse_offset(offset_text) {
        ui.add_space(6.0);
        banner(ui, Tone::Warning, Glyph::Disc, &problem);
    }
}

/// AccurateRip's public list of drive read offsets. TuneBox links to it rather than copying it.
const DRIVE_OFFSET_LIST: &str = "https://www.accuraterip.com/driveoffsets.htm";

fn columns(ui: &mut Ui, prefs: &mut Preferences) {
    card(ui, "Song list columns", "Choose what the song list shows. Name is always shown.", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for column in Column::ALL {
                let mut on = prefs.visible_columns.contains(&column);
                if widgets::chip(ui, column.title(), &mut on, column != Column::Name) {
                    if on {
                        // Keep the registry's order so columns appear in a familiar sequence.
                        prefs.visible_columns.push(column);
                        prefs.visible_columns.sort_by_key(|c| Column::ALL.iter().position(|x| x == c));
                    } else {
                        prefs.visible_columns.retain(|c| *c != column);
                    }
                }
            }
        });
        ui.add_space(10.0);
        if ui.button("Restore Defaults").clicked() {
            prefs.visible_columns = Column::DEFAULT_VISIBLE.to_vec();
        }
    });
}

fn get_info_fields(ui: &mut Ui, prefs: &mut Preferences) {
    card(
        ui,
        "Always show in Get Info",
        "Name, artist, album and the other everyday fields are always shown. Choose more to show beside them; the rest stay one click away under Show all fields.",
        |ui| {
            for group in FieldGroup::ALL {
                group_heading(ui, group.title());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    for field in group.fields() {
                        let mut on = prefs.info_fields.contains(&field);
                        if widgets::chip(ui, field.label(), &mut on, true) {
                            prefs.pin_info_field(field, on);
                        }
                    }
                });
                ui.add_space(6.0);
            }
            ui.add_space(6.0);
            if ui.add_enabled(!prefs.info_fields.is_empty(), egui::Button::new("Show None")).clicked() {
                prefs.info_fields.clear();
            }
        },
    );
}

/// A small spaced-capitals heading over a group of chips.
fn group_heading(ui: &mut Ui, title: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    theme::spaced_caps(ui.painter(), rect.left_bottom() - vec2(0.0, 5.0), title, theme::TEXT_MUTED);
}

#[cfg(test)]
mod tests {
    use super::parse_offset;

    #[test]
    fn reads_typed_offsets() {
        assert_eq!(parse_offset("+667"), Ok(667));
        assert_eq!(parse_offset(" -1164 "), Ok(-1164));
        assert_eq!(parse_offset("6"), Ok(6));
        assert_eq!(parse_offset(""), Ok(0));
        assert!(parse_offset("six").unwrap_err().contains("not a number"));
        assert!(parse_offset("+66700").unwrap_err().contains("between"));
    }
}
