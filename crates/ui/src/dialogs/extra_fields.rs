//! The additional tag fields in Get Info: credits, classical, release, sorting and more. The
//! fields pinned in Preferences show in cards of their own; the rest wait behind "Show all
//! fields", which also says how many of them have a value, so nothing is hidden unawares.

use crate::dialogs::layout::{card, muted, value_row};
use crate::theme;
use egui::{Color32, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::collections::BTreeMap;
use tunebox_engine::model::{FieldGroup, FieldKind, TagField, TrackMeta};

/// Every additional field as text. With several songs a field holds the value they share, or
/// is empty when they differ; only fields the listener changes are written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtraDraft(BTreeMap<TagField, String>);

impl ExtraDraft {
    pub fn of(meta: &TrackMeta) -> Self {
        Self(TagField::ALL.into_iter().map(|f| (f, meta.extra(f).to_owned())).collect())
    }

    /// The values all `drafts` agree on; others are left empty.
    pub fn common<'a>(mut drafts: impl Iterator<Item = &'a ExtraDraft>) -> Self {
        let Some(first) = drafts.next() else { return Self::default() };
        let mut common = first.clone();
        for draft in drafts {
            for (field, value) in &mut common.0 {
                if draft.get(*field) != value.as_str() {
                    value.clear();
                }
            }
        }
        common
    }

    pub fn get(&self, field: TagField) -> &str {
        self.0.get(&field).map_or("", String::as_str)
    }

    /// Writes into `meta` every field changed since `original` whose new value suits it.
    /// Fields the listener did not touch stay out of `meta.extra`, so they are never written.
    pub fn apply(&self, original: &ExtraDraft, meta: &mut TrackMeta) {
        for (field, value) in &self.0 {
            if value != original.get(*field) && field.accepts(value) {
                meta.extra.insert(*field, value.trim().to_owned());
            }
        }
    }

    fn filled(&self, field: TagField) -> bool {
        !self.get(field).trim().is_empty()
    }
}

/// The state of "Show all fields".
#[derive(Clone, Copy, Debug, Default)]
pub struct Disclosure {
    pub open: bool,
    /// Scroll the newly shown fields into view on the next frame.
    reveal: bool,
}

impl Disclosure {
    /// Open, with the fields scrolled into view.
    pub fn revealed() -> Self {
        Self { open: true, reveal: true }
    }

    /// Closed, with the row itself scrolled into view.
    pub fn in_view() -> Self {
        Self { open: false, reveal: true }
    }
}

/// What the cards need to know besides the draft.
pub struct Context<'a> {
    /// The fields Preferences pins to Get Info.
    pub pinned: &'a [TagField],
    /// Why a field cannot be edited, for fields some of the songs' files have no place for.
    pub blocked: &'a BTreeMap<TagField, String>,
    /// Editing several songs at once.
    pub several: bool,
}

/// The pinned fields, in cards by group.
pub fn pinned(ui: &mut Ui, draft: &mut ExtraDraft, context: &Context<'_>) {
    group_cards(ui, draft, context, |field| context.pinned.contains(&field));
}

/// "Show all fields", and under it, when open, every field that is not pinned.
pub fn the_rest(ui: &mut Ui, draft: &mut ExtraDraft, context: &Context<'_>, state: &mut Disclosure) {
    let hidden = |field: TagField| !context.pinned.contains(&field);
    let filled = TagField::ALL.into_iter().filter(|f| hidden(*f) && draft.filled(*f)).count();
    let row = disclosure(ui, state, filled);
    if state.open {
        group_cards(ui, draft, context, hidden);
    }
    if std::mem::take(&mut state.reveal) {
        // The fields open below the fold otherwise, and look as if nothing happened.
        row.scroll_to_me(Some(if state.open { egui::Align::Min } else { egui::Align::Max }));
    }
}

fn group_cards(ui: &mut Ui, draft: &mut ExtraDraft, context: &Context<'_>, show: impl Fn(TagField) -> bool) {
    for group in FieldGroup::ALL {
        let fields: Vec<TagField> = group.fields().filter(|f| show(*f)).collect();
        if fields.is_empty() {
            continue;
        }
        card(ui, group.title(), group.description(), |ui| {
            for field in fields {
                field_row(ui, field, draft, context);
            }
        });
    }
}

/// One field: a text box sized to what it holds, disabled with the reason when the file has no
/// place for it, and a note when what was typed will not be saved.
fn field_row(ui: &mut Ui, field: TagField, draft: &mut ExtraDraft, context: &Context<'_>) {
    let Some(value) = draft.0.get_mut(&field) else { return };
    let blocked = context.blocked.get(&field);
    // Number boxes are too narrow for a hint; their labels say enough.
    let hint = match (context.several, field.kind()) {
        (true, _) => "Keep as is",
        (false, FieldKind::Number) => "",
        (false, _) => field.hint(),
    };
    let mut edit = |ui: &mut Ui| {
        let editor = match field.kind() {
            FieldKind::Lines => egui::TextEdit::multiline(value).desired_rows(6).desired_width(f32::INFINITY),
            FieldKind::Date => egui::TextEdit::singleline(value).desired_width(120.0),
            FieldKind::Number => egui::TextEdit::singleline(value).desired_width(60.0),
            FieldKind::Text => egui::TextEdit::singleline(value).desired_width(ui.available_width()),
        };
        let response = ui.add_enabled(blocked.is_none(), editor.hint_text(hint));
        if let Some(reason) = blocked {
            response.on_disabled_hover_text(reason);
        }
    };
    if field.kind() == FieldKind::Lines {
        // Lyrics get the card's full width.
        edit(ui);
    } else {
        value_row(ui, field.label(), edit);
    }
    if !field.accepts(value) {
        let note = match field.kind() {
            FieldKind::Date => "Not saved: use a date such as 2011-04-02, 2011-04 or 2011.",
            _ => "Not saved: use a whole number.",
        };
        value_row(ui, "", |ui| {
            ui.label(RichText::new(note).size(11.5).color(theme::WARNING));
        });
    }
}

/// A full-width row with a turning chevron: "Show all fields", and how many hidden ones have a
/// value.
fn disclosure(ui: &mut Ui, state: &mut Disclosure, filled: usize) -> egui::Response {
    let open = &mut state.open;
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 8.0, Color32::from_black_alpha(8));
    }
    let center = pos2(rect.left() + 16.0, rect.center().y);
    let points = if *open {
        vec![center + vec2(-4.0, -2.0), center + vec2(0.0, 2.5), center + vec2(4.0, -2.0)]
    } else {
        vec![center + vec2(-2.0, -4.0), center + vec2(2.5, 0.0), center + vec2(-2.0, 4.0)]
    };
    painter.add(egui::Shape::line(points, Stroke::new(1.8, theme::ACCENT)));
    let label = if *open { "Show fewer fields" } else { "Show all fields" };
    let text_left = pos2(rect.left() + 30.0, rect.center().y);
    let galley = painter.layout_no_wrap(label.to_owned(), crate::fonts::bold(13.0), theme::ACCENT);
    let label_width = galley.size().x;
    painter.galley(text_left - vec2(0.0, galley.size().y / 2.0), galley, theme::ACCENT);
    let count = match filled {
        0 => String::new(),
        1 => "1 more has a value".to_owned(),
        n => format!("{n} more have values"),
    };
    if !*open && !count.is_empty() {
        painter.text(
            text_left + vec2(label_width + 10.0, 0.0),
            egui::Align2::LEFT_CENTER,
            count,
            egui::FontId::proportional(12.0),
            theme::TEXT_MUTED,
        );
    }
    if response.clicked() {
        *open = !*open;
        state.reveal = *open;
    }
    ui.add_space(8.0);
    response.on_hover_text(muted("Choose fields to always show in Preferences, under Get Info."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_songs_share_only_agreed_values_and_untouched_fields_stay_unwritten() {
        let mut a = TrackMeta::default();
        a.extra.insert(TagField::Conductor, "C. Baton".into());
        a.extra.insert(TagField::Label, "Harbour".into());
        let mut b = TrackMeta::default();
        b.extra.insert(TagField::Conductor, "D. Wand".into());
        b.extra.insert(TagField::Label, "Harbour".into());
        let drafts = [ExtraDraft::of(&a), ExtraDraft::of(&b)];
        let original = ExtraDraft::common(drafts.iter());
        assert_eq!(original.get(TagField::Conductor), "", "differing values start empty");
        assert_eq!(original.get(TagField::Label), "Harbour");

        let mut draft = original.clone();
        draft.0.insert(TagField::Mood, " Calm ".into());
        draft.0.insert(TagField::Label, String::new());
        draft.0.insert(TagField::ReleaseDate, "spring".into());
        let mut edited = b.clone();
        draft.apply(&original, &mut edited);
        assert_eq!(edited.extra(TagField::Conductor), "D. Wand", "kept: not touched");
        assert_eq!(edited.extra(TagField::Mood), "Calm");
        assert_eq!(edited.extra.get(&TagField::Label).map(String::as_str), Some(""), "cleared on purpose");
        assert!(!edited.extra.contains_key(&TagField::ReleaseDate), "not a date, so not saved");
        assert!(!edited.extra.contains_key(&TagField::Work), "never touched, never written");
    }
}
