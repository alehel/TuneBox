//! Tasks ▸ Rename Files from Tags…: the pattern, where the files go, and a live preview of
//! every song (old name, new name, and the songs skipped or in conflict). The engine plans and
//! renames; this only edits the pattern, shows the engine's preview and queues the rename.

use crate::actions::Action;
use crate::dialogs::layout::{Tone, banner, card, muted};
use crate::dialogs::{modal, title_heading};
use crate::glyphs::Glyph;
use crate::panels::widgets;
use crate::theme;
use egui::{Align, Color32, Layout, RichText, Sense, Stroke, Ui, vec2};
use std::path::Path;
use std::time::{Duration, Instant};
use tunebox_engine::model::TrackId;
use tunebox_engine::model::naming::{COMMON_FIELDS, PRESETS};
use tunebox_engine::{JobKind, Planned, RenameBase, RenameConflicts, RenameItem, RenamePlan, RenameRequest, Session};

/// How long a preview stands before the disk is looked at again (files can appear meanwhile).
const PREVIEW_FRESH_FOR: Duration = Duration::from_secs(3);
/// Height of one song in the preview list: the new name over the old.
const ROW_HEIGHT: f32 = 36.0;

#[derive(Clone, Debug)]
pub struct RenameFilesDialog {
    tracks: Vec<TrackId>,
    pattern: String,
    base: RenameBase,
    pub(crate) skip_conflicts: bool,
    preview: Option<Preview>,
}

/// The engine's preview, and what it was made from.
#[derive(Clone, Debug)]
struct Preview {
    pattern: String,
    base: RenameBase,
    revision: u64,
    made: Instant,
    result: Result<RenamePlan, String>,
}

impl RenameFilesDialog {
    pub fn new(tracks: Vec<TrackId>) -> Self {
        let pattern = PRESETS.first().map(|(_, p)| (*p).to_owned()).unwrap_or_default();
        Self { tracks, pattern, base: RenameBase::OwnFolder, skip_conflicts: false, preview: None }
    }

    pub fn with_pattern(mut self, pattern: &str) -> Self {
        pattern.clone_into(&mut self.pattern);
        self
    }

    /// The same choices with a fresh preview, as after the disk changed under the last one.
    pub fn reopened(mut self) -> Self {
        self.preview = None;
        self
    }

    /// The previewed plan the listener confirmed, and what to do with conflicts.
    pub fn confirmed(&self) -> Option<(&RenamePlan, RenameConflicts)> {
        let plan = self.preview.as_ref()?.result.as_ref().ok()?;
        let conflicts = if self.skip_conflicts { RenameConflicts::Skip } else { RenameConflicts::Refuse };
        Some((plan, conflicts))
    }

    fn update_preview(&mut self, session: &Session) {
        let current = self.preview.as_ref().is_some_and(|p| {
            p.pattern == self.pattern
                && p.base == self.base
                && p.revision == session.revision()
                && p.made.elapsed() < PREVIEW_FRESH_FOR
        });
        if current {
            return;
        }
        let request = RenameRequest { tracks: self.tracks.clone(), pattern: self.pattern.clone(), base: self.base };
        self.preview = Some(Preview {
            pattern: self.pattern.clone(),
            base: self.base,
            revision: session.revision(),
            made: Instant::now(),
            result: session.preview_rename(&request).map_err(|e| e.to_string()),
        });
    }

    /// Draws the dialog. Returns `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, session: &Session, actions: &mut Vec<Action>) -> bool {
        self.update_preview(session);
        let busy = [JobKind::Rename, JobKind::Import, JobKind::Refresh, JobKind::Artwork]
            .into_iter()
            .find(|k| session.is_busy(*k));
        let mut result = None;
        modal(ctx, "rename-files", |ui| {
            ui.set_width(640.0);
            title_heading(ui, "Rename Files from Tags");
            self.pattern_card(ui, session);
            self.preview_card(ui, session);
            if let Some(kind) = busy {
                banner(ui, Tone::Info, Glyph::Info, &format!("{} now. Rename once it has finished.", kind.noun()));
            }
            result = self.buttons(ui, busy.is_none());
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            result = Some(false);
        }
        match result {
            Some(true) => {
                actions.push(Action::RenameFiles(Box::new(self.clone())));
                false
            }
            Some(false) => false,
            None => true,
        }
    }

    fn pattern_card(&mut self, ui: &mut Ui, session: &Session) {
        let description = "How each file is named from its tags. “/” starts a folder, {a|b} takes the first that is set, {track:02} pads with zeros. Each file keeps its extension.";
        card(ui, "Pattern", description, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.pattern)
                    .desired_width(ui.available_width())
                    .font(egui::FontId::monospace(13.0)),
            );
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for key in COMMON_FIELDS {
                    if token_chip(ui, &format!("{{{key}}}")).clicked() {
                        insert_field(&mut self.pattern, key);
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(muted("Ready-made:"));
                for (name, pattern) in PRESETS {
                    if ui.add(egui::Button::new(RichText::new(name).size(12.0)).small()).clicked() {
                        pattern.clone_into(&mut self.pattern);
                    }
                }
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let mut choice = usize::from(self.base == RenameBase::MediaFolder);
                let options = ["In each song's library folder", "In the media folder"];
                if widgets::segmented(ui, "rename-base", &options, &mut choice) {
                    self.base = if choice == 1 { RenameBase::MediaFolder } else { RenameBase::OwnFolder };
                }
            });
            ui.add_space(4.0);
            let detail = match self.base {
                RenameBase::OwnFolder => "Files stay inside the folder they are kept in. Songs outside your library folders and the media folder are skipped.".to_owned(),
                RenameBase::MediaFolder => format!("Files move into {}, within their drive.", session.preferences().media_folder.display()),
            };
            ui.add(egui::Label::new(muted(&detail)).wrap());
        });
    }

    fn preview_card(&mut self, ui: &mut Ui, session: &Session) {
        let Some(preview) = &self.preview else { return };
        let plan = match &preview.result {
            Ok(plan) => plan,
            Err(message) => {
                banner(ui, Tone::Warning, Glyph::Missing, message);
                return;
            }
        };
        card(ui, "Preview", &plan.summary(), |ui| {
            // Problems first, where they are seen.
            let mut items: Vec<&RenameItem> = plan.items.iter().collect();
            items.sort_by_key(|item| match item.planned {
                Planned::Conflict { .. } => 0,
                Planned::Skipped(_) => 1,
                Planned::Rename { .. } => 2,
                Planned::Unchanged => 3,
            });
            // As tall as the window allows, beside the pattern and the buttons.
            let room = (ui.ctx().content_rect().height() - 600.0).clamp(ROW_HEIGHT * 3.0, ROW_HEIGHT * 10.0);
            egui::ScrollArea::vertical().max_height(room).auto_shrink([false, true]).show_rows(
                ui,
                ROW_HEIGHT,
                items.len(),
                |ui, range| {
                    for (index, item) in items.iter().enumerate().skip(range.start).take(range.len()) {
                        preview_row(ui, session, item, index);
                    }
                },
            );
        });
        let conflicts = plan.conflicts();
        if conflicts > 0 {
            let text = match conflicts {
                1 => "1 song would get a name that is already taken. Change the pattern, or skip it.".to_owned(),
                n => format!("{n} songs would get names that are already taken. Change the pattern, or skip them."),
            };
            banner(ui, Tone::Warning, Glyph::Missing, &text);
        }
    }

    /// Skip-conflicts on the left, Rename and Cancel on the right. `Some(true)` is Rename.
    fn buttons(&mut self, ui: &mut Ui, idle: bool) -> Option<bool> {
        let plan = self.preview.as_ref().and_then(|p| p.result.as_ref().ok());
        let conflicts = plan.map_or(0, RenamePlan::conflicts);
        let renames = plan.map_or(0, RenamePlan::renamed);
        let ready = idle && renames > 0 && (conflicts == 0 || self.skip_conflicts);
        let mut result = None;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 28.0), Layout::left_to_right(Align::Center), |ui| {
            if conflicts > 0 {
                ui.checkbox(&mut self.skip_conflicts, "Skip the songs in conflict");
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = match renames {
                    1 => "Rename 1 File".to_owned(),
                    n => format!("Rename {n} Files"),
                };
                if widgets::primary_button(ui, &label, ready, vec2(140.0, 28.0)).clicked() && ready {
                    result = Some(true);
                }
                if ui.add(egui::Button::new("Cancel").min_size(vec2(88.0, 28.0))).clicked() {
                    result = Some(false);
                }
            });
        });
        result
    }
}

/// One song in the preview: what it will be called, and what it was (or why it stays).
fn preview_row(ui: &mut Ui, session: &Session, item: &RenameItem, index: usize) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    if index % 2 == 1 {
        ui.painter().rect_filled(rect, 4.0, theme::ROW_ALT);
    }
    let short = |path: &Path| {
        item.base.as_deref().and_then(|base| path.strip_prefix(base).ok()).unwrap_or(path).display().to_string()
    };
    let title = session.track(item.id).map(|t| t.title().to_owned()).unwrap_or_default();
    let (first, second, color) = match &item.planned {
        Planned::Rename { to } => (short(to), format!("was {}", short(&item.from)), theme::TEXT),
        Planned::Unchanged => (short(&item.from), "already named so".to_owned(), theme::TEXT_MUTED),
        Planned::Skipped(reason) => (title, format!("skipped: {}", reason.text()), theme::TEXT_MUTED),
        Planned::Conflict { to, reason } => (short(to), format!("{title}: {}", reason.text()), theme::WARNING),
    };
    let left = rect.left() + 8.0;
    let painter = ui.painter_at(rect.shrink2(vec2(8.0, 0.0)));
    // One line each: a long path is cut at the row's edge, not wrapped over the next row.
    let line = |text: String, size: f32, color: Color32, y: f32| {
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(size), color);
        painter.galley(egui::pos2(left, y), galley, color);
    };
    line(first, 13.0, color, rect.top() + 3.0);
    let muted = if color == theme::WARNING { theme::WARNING.gamma_multiply(0.8) } else { theme::TEXT_MUTED };
    line(second, 11.5, muted, rect.top() + 20.0);
}

/// A pill that adds a field to the pattern.
fn token_chip(ui: &mut Ui, label: &str) -> egui::Response {
    let font = egui::FontId::monospace(11.5);
    let text = ui.painter().layout_no_wrap(label.to_owned(), font.clone(), theme::ACCENT);
    let (rect, response) = ui.allocate_exact_size(vec2(text.size().x + 16.0, 22.0), Sense::click());
    let fill = if response.hovered() { theme::ACCENT.gamma_multiply(0.18) } else { theme::ACCENT.gamma_multiply(0.08) };
    ui.painter().rect_filled(rect, 11.0, fill);
    ui.painter().rect_stroke(
        rect,
        11.0,
        Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.35)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, font, theme::ACCENT);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Adds `{key}` at the end of the pattern, with a space before it unless it starts a name.
fn insert_field(pattern: &mut String, key: &str) {
    if !(pattern.is_empty() || pattern.ends_with('/') || pattern.ends_with(' ')) {
        pattern.push(' ');
    }
    pattern.push_str(&format!("{{{key}}}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_added_with_a_space_unless_a_name_starts() {
        let mut pattern = String::new();
        insert_field(&mut pattern, "artist");
        pattern.push('/');
        insert_field(&mut pattern, "track");
        insert_field(&mut pattern, "title");
        assert_eq!(pattern, "{artist}/{track} {title}");
    }
}
