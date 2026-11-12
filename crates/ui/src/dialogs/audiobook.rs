//! The audiobook's dialogs: starting a book, the request for the next disc, the question about
//! a disc that may not belong, building, the confirmations, and the sheet of shortcuts.
//!
//! Between discs the book page shows "Insert disc N" as a modal dialog with That Was the Last
//! Disc: no editing until the disc is in. Not Now leaves the page (it does not edit), so the
//! rest of TuneBox stays usable while a disc is looked for.

use crate::actions::Action;
use crate::dialogs::{layout, modal, title_heading, window};
use crate::panels::widgets;
use crate::state::SourceItem;
use crate::theme;
use egui::{RichText, Ui, vec2};
use tunebox_engine::commands::{COMMANDS, CommandId, Scope};
use tunebox_engine::model::audiobook::layout::{chapter_file_path, single_file_path};
use tunebox_engine::model::audiobook::{
    BookDetails, MAX_CHAPTERS_PER_FILE, OutputFormat, OutputLayout, OutputSettings, seconds,
};
use tunebox_engine::model::format_total_time;
use tunebox_engine::{DiscQuestion, Session};

/// Import as Audiobook…: the book's details, filled in from the disc's names.
#[derive(Clone, Debug)]
pub struct StartAudiobook {
    pub details: BookDetails,
    series_number: String,
    year: String,
}

impl StartAudiobook {
    pub fn new(session: &Session) -> Self {
        let mut details = BookDetails::default();
        if let Some(cd) = session.cd() {
            details.title = cd.info.album.clone();
            details.author = cd.info.artist.clone();
            details.year = cd.info.year;
        }
        let year = details.year.map(|y| y.to_string()).unwrap_or_default();
        Self { details, series_number: String::new(), year }
    }

    pub fn show(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let (details, number, year) = (&mut self.details, &mut self.series_number, &mut self.year);
        let result = window(ctx, "Import as Audiobook", 460.0, "Start Audiobook", |ui| {
            layout::text_field(ui, "Title", &mut details.title, "The book's title");
            layout::text_field(ui, "Author", &mut details.author, "Who wrote it");
            layout::text_field(ui, "Narrator", &mut details.narrator, "Who reads it");
            layout::text_field(ui, "Series", &mut details.series, "Optional");
            layout::text_field(ui, "Number in series", number, "Optional");
            layout::text_field(ui, "Year", year, "Optional");
            layout::text_field(ui, "Publisher", &mut details.publisher, "Optional");
            ui.label(layout::muted("Description"));
            ui.add(egui::TextEdit::multiline(&mut details.description).desired_rows(3).desired_width(f32::INFINITY));
            ui.add_space(6.0);
            ui.label(layout::muted(
                "Each disc starts as one chapter. You split and name chapters on the book's page. Nothing is \
                 ripped until you choose Rip Disc.",
            ));
        });
        match result {
            Some(true) => {
                let mut details = self.details.clone();
                details.series_number = self.series_number.trim().parse().ok();
                details.year = self.year.trim().parse().ok();
                actions.push(Action::StartAudiobook(Box::new(details)));
                false
            }
            Some(false) => false,
            None => true,
        }
    }
}

/// "Insert disc N": shown on the book page between discs. It closes by itself when the disc
/// is in.
#[derive(Clone, Debug)]
pub struct InsertDisc {
    pub next: u16,
}

impl InsertDisc {
    pub fn show(&mut self, ctx: &egui::Context, session: &Session, actions: &mut Vec<Action>) -> bool {
        if session.audiobook_waiting_for() != Some(self.next) {
            return false;
        }
        let ripped = self.next.saturating_sub(1);
        let mut open = true;
        modal(ctx, "insert-disc", |ui| {
            ui.set_width(420.0);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(52.0, 52.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 25.0, theme::ICON_DISC.gamma_multiply(0.15));
                crate::glyphs::paint(ui.painter(), rect.shrink(13.0), crate::glyphs::Glyph::Eject, theme::ICON_DISC);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("Disc {ripped} is ripped and ejected.")).color(theme::TEXT_MUTED));
                    ui.label(RichText::new(format!("Insert disc {}", self.next)).font(crate::fonts::bold(24.0)));
                    ui.label(layout::muted("Its tracks appear as soon as it is in."));
                });
            });
            ui.add_space(14.0);
            buttons(ui, |ui| {
                if widgets::primary_button(ui, "That Was the Last Disc", true, vec2(180.0, 28.0)).clicked() {
                    actions.push(Action::Run(CommandId::FinishRipping));
                    open = false;
                }
                let not_now = ui
                    .add(egui::Button::new("Not Now").min_size(vec2(88.0, 28.0)))
                    .on_hover_text("Leave the book until the disc is found. It waits, saved.");
                if not_now.clicked() {
                    actions.push(Action::SelectSource(SourceItem::Music));
                    open = false;
                }
            });
        });
        open
    }
}

/// A disc that may not belong to the book: use it as the next disc, or eject it.
#[derive(Clone, Debug)]
pub struct DiscQuestionDialog {
    pub question: DiscQuestion,
}

impl DiscQuestionDialog {
    pub fn show(&mut self, ctx: &egui::Context, session: &Session, actions: &mut Vec<Action>) -> bool {
        if session.audiobook_question() != Some(&self.question) {
            return false;
        }
        let expected = match self.question {
            DiscQuestion::OtherDisc { expected, .. } | DiscQuestion::Unrelated { expected } => expected,
        };
        let mut open = true;
        modal(ctx, "disc-question", |ui| {
            ui.set_width(440.0);
            title_heading(ui, "Is This the Next Disc?");
            ui.label(self.question.text());
            ui.add_space(12.0);
            buttons(ui, |ui| {
                let label = format!("Use as Disc {expected}");
                if widgets::primary_button(ui, &label, true, vec2(130.0, 28.0)).clicked() {
                    actions.push(Action::AnswerDiscQuestion(true));
                    open = false;
                }
                if ui.add(egui::Button::new("Eject").min_size(vec2(88.0, 28.0))).clicked() {
                    actions.push(Action::AnswerDiscQuestion(false));
                    open = false;
                }
            });
        });
        open
    }
}

/// Build Audiobook…: one file or a file per chapter, the format, and what it will make.
#[derive(Clone, Debug)]
pub struct BuildAudiobook {
    pub output: OutputSettings,
}

impl BuildAudiobook {
    pub fn new(session: &Session) -> Self {
        Self { output: session.audiobook().map(|b| b.output).unwrap_or_default() }
    }

    pub fn show(&mut self, ctx: &egui::Context, session: &Session, actions: &mut Vec<Action>) -> bool {
        let Some(book) = session.audiobook() else { return false };
        let chapters = book.output_chapters().len();
        let length = std::time::Duration::from_secs_f64(seconds(book.kept_samples()));
        let output = &mut self.output;
        let mut problem = output.check().err().map(|e| e.to_string());
        if output.layout == OutputLayout::SingleFile && chapters > MAX_CHAPTERS_PER_FILE {
            problem = Some(format!(
                "This book has {chapters} chapters; one file holds at most {MAX_CHAPTERS_PER_FILE}. Join some \
                 chapters, or build one file per chapter."
            ));
        }
        let mut result = None;
        modal(ctx, "build-audiobook", |ui| {
            ui.set_width(480.0);
            title_heading(ui, &format!("Build “{}”", book.details.title));
            ui.label(layout::muted(&format!(
                "{chapters} chapters · {} · {} discs",
                format_total_time(length),
                book.discs.len()
            )));
            ui.add_space(8.0);
            ui.label(RichText::new("Make").strong());
            ui.radio_value(&mut output.layout, OutputLayout::SingleFile, "One file with chapters (.m4b)");
            ui.radio_value(
                &mut output.layout,
                OutputLayout::PerChapter,
                format!("One file per chapter ({chapters} files)"),
            );
            ui.add_space(8.0);
            layout::value_row(ui, "Format", |ui| {
                egui::ComboBox::from_id_salt("format").selected_text(output.format.name()).show_ui(ui, |ui| {
                    for format in [OutputFormat::Aac, OutputFormat::AppleLossless, OutputFormat::Flac] {
                        ui.selectable_value(&mut output.format, format, format.name());
                    }
                });
            });
            if output.format == OutputFormat::Aac {
                layout::value_row(ui, "Bit rate", |ui| {
                    egui::ComboBox::from_id_salt("bitrate")
                        .selected_text(format!("{} kbit/s", output.aac_bitrate_kbps))
                        .show_ui(ui, |ui| {
                            for rate in OutputSettings::AAC_BITRATES {
                                ui.selectable_value(&mut output.aac_bitrate_kbps, rate, format!("{rate} kbit/s"));
                            }
                        });
                });
                layout::value_row(ui, "Channels", |ui| {
                    ui.radio_value(&mut output.mono, true, "Mono (spoken word)");
                    ui.radio_value(&mut output.mono, false, "Stereo");
                });
            }
            ui.add_space(8.0);
            let media = session.preferences().media_folder.clone();
            let place = match output.layout {
                OutputLayout::SingleFile => single_file_path(&book.details, output.format.extension()),
                OutputLayout::PerChapter => {
                    chapter_file_path(&book.details, 1, chapters, "…", output.format.extension())
                        .parent()
                        .map(|p| p.join("…"))
                        .unwrap_or_default()
                }
            };
            ui.label(format!(
                "About {}, into {}",
                size_estimate(output, seconds(book.kept_samples())),
                media.join(place).display()
            ));
            ui.label(layout::muted("The ripped discs are deleted once the book is in your library."));
            if let Some(problem) = &problem {
                ui.add_space(6.0);
                ui.colored_label(theme::WARNING, problem);
            }
            ui.add_space(12.0);
            buttons(ui, |ui| {
                if widgets::primary_button(ui, "Build", problem.is_none(), vec2(88.0, 28.0)).clicked() {
                    result = Some(true);
                }
                if ui.add(egui::Button::new("Cancel").min_size(vec2(88.0, 28.0))).clicked() {
                    result = Some(false);
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            result = Some(false);
        }
        match result {
            Some(true) => {
                actions.push(Action::BuildAudiobook(self.output));
                false
            }
            Some(false) => false,
            None => true,
        }
    }
}

/// What a book of `seconds` comes to: exact for AAC (constant bit rate), a guess for the
/// lossless formats (speech compresses to about half the CD's 1411 kbit/s).
fn size_estimate(output: &OutputSettings, seconds: f64) -> String {
    let kbit = match output.format {
        OutputFormat::Aac => f64::from(output.aac_bitrate_kbps),
        OutputFormat::AppleLossless | OutputFormat::Flac => 700.0,
    };
    let bytes = kbit * 1000.0 / 8.0 * seconds;
    tunebox_engine::model::format_bytes(bytes as u64)
}

/// MusicBrainz lists more discs than were ripped: finish anyway?
#[derive(Clone, Debug)]
pub struct ConfirmFinish {
    pub message: String,
}

impl ConfirmFinish {
    pub fn show(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let message = &self.message;
        match window(ctx, "That Was the Last Disc?", 420.0, "Finish Anyway", |ui| {
            ui.label(message);
        }) {
            Some(true) => {
                actions.push(Action::FinishRipping { anyway: true });
                false
            }
            Some(false) => false,
            None => true,
        }
    }
}

/// Discard Audiobook…: asked first, since the ripped discs go too.
pub fn confirm_discard(ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
    match window(ctx, "Discard Audiobook", 420.0, "Discard", |ui| {
        ui.label(RichText::new("Throw this audiobook away?").strong());
        ui.label("Its ripped discs, chapters and edits are deleted. Nothing in your library is touched.");
    }) {
        Some(true) => {
            actions.push(Action::DiscardAudiobook);
            false
        }
        Some(false) => false,
        None => true,
    }
}

/// The book page's keys, on one sheet (?).
pub fn shortcuts_sheet(ctx: &egui::Context) -> bool {
    let mac = cfg!(target_os = "macos");
    let mut open = true;
    modal(ctx, "book-shortcuts", |ui| {
        ui.set_width(640.0);
        title_heading(ui, "Audiobook Shortcuts");
        ui.label(layout::muted("On the book's page, while no name is being typed."));
        ui.add_space(8.0);
        ui.columns(2, |columns| {
            let fixed = [
                ("↑ ↓", "previous, next track"),
                ("⇧↑ ⇧↓", "extend the selection"),
                ("Space", "play or pause from the selected track"),
                ("Tab ⇧Tab", "next, previous name while renaming"),
                ("Esc", "stop renaming"),
            ];
            if let Some(column) = columns.get_mut(0) {
                for (keys, what) in fixed {
                    key_row(column, keys, what);
                }
                column.add_space(8.0);
                column.label(RichText::new("Trim editor").strong());
                for (keys, what) in [
                    ("Tab", "start cut, end cut"),
                    ("← →", "move the cut 0.1 s; with ⇧, 1 s"),
                    ("⌥← ⌥→", "previous, next pause"),
                    ("P", "play across the cut"),
                    ("X", "play what is cut"),
                    ("0", "no cut"),
                ] {
                    key_row(column, keys, what);
                }
            }
            if let Some(column) = columns.get_mut(1) {
                for spec in COMMANDS.iter().filter(|c| c.scope == Scope::BookPage) {
                    if let Some(shortcut) = spec.shortcut {
                        key_row(column, &shortcut.label(mac), spec.label);
                    }
                }
            }
        });
        ui.add_space(12.0);
        buttons(ui, |ui| {
            if widgets::primary_button(ui, "Done", true, vec2(88.0, 28.0)).clicked() {
                open = false;
            }
        });
    });
    open && !ctx.input(|i| i.key_pressed(egui::Key::Escape))
}

fn key_row(ui: &mut Ui, keys: &str, what: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(96.0, 20.0), egui::Sense::hover());
        ui.painter().text(
            rect.right_center(),
            egui::Align2::RIGHT_CENTER,
            keys,
            egui::FontId::proportional(12.5),
            theme::TEXT,
        );
        ui.add_space(8.0);
        ui.label(what);
    });
}

/// A right-to-left row of buttons along a dialog's bottom.
fn buttons(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 28.0),
        egui::Layout::right_to_left(egui::Align::Center),
        contents,
    );
}
