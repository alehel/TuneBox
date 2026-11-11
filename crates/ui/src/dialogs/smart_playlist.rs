//! The smart playlist editor: rules, a limit, and "match only checked items".

use crate::actions::Action;
use crate::dialogs::window;
use egui::{ComboBox, Ui};
use tunebox_engine::model::smart::{
    DateField, DateOp, Limit, LimitUnit, MatchMode, NaiveDate, NumberField, NumberOp, Period, PeriodUnit, Rule,
    SelectedBy, TextField, TextOp,
};
use tunebox_engine::model::{PlaylistId, SmartRules};

/// A field as the editor lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Text(TextField),
    Number(NumberField),
    Date(DateField),
}

const FIELDS: [(Field, &str); 20] = [
    (Field::Text(TextField::Name), "Name"),
    (Field::Text(TextField::Artist), "Artist"),
    (Field::Text(TextField::AlbumArtist), "Album Artist"),
    (Field::Text(TextField::Album), "Album"),
    (Field::Text(TextField::Genre), "Genre"),
    (Field::Text(TextField::Composer), "Composer"),
    (Field::Text(TextField::Grouping), "Grouping"),
    (Field::Text(TextField::Comments), "Comments"),
    (Field::Text(TextField::Kind), "Kind"),
    (Field::Text(TextField::MediaKind), "Media Kind"),
    (Field::Number(NumberField::Rating), "Rating (stars)"),
    (Field::Number(NumberField::Plays), "Plays"),
    (Field::Number(NumberField::Skips), "Skips"),
    (Field::Number(NumberField::Year), "Year"),
    (Field::Number(NumberField::BitRate), "Bit Rate (kbps)"),
    (Field::Number(NumberField::Time), "Time (seconds)"),
    (Field::Number(NumberField::Bpm), "BPM"),
    (Field::Number(NumberField::TrackNumber), "Track Number"),
    (Field::Date(DateField::DateAdded), "Date Added"),
    (Field::Date(DateField::LastPlayed), "Last Played"),
];

const TEXT_OPS: [&str; 6] = ["contains", "does not contain", "is", "is not", "starts with", "ends with"];
const NUMBER_OPS: [&str; 5] = ["is", "is not", "is greater than", "is less than", "is in the range"];
const DATE_OPS: [&str; 4] = ["is in the last", "is not in the last", "is before", "is after"];
const UNITS: [(PeriodUnit, &str); 3] =
    [(PeriodUnit::Days, "days"), (PeriodUnit::Weeks, "weeks"), (PeriodUnit::Months, "months")];
const LIMIT_UNITS: [(LimitUnit, &str); 5] = [
    (LimitUnit::Items, "items"),
    (LimitUnit::Minutes, "minutes"),
    (LimitUnit::Hours, "hours"),
    (LimitUnit::Megabytes, "MB"),
    (LimitUnit::Gigabytes, "GB"),
];
const SELECTED_BY: [(SelectedBy, &str); 13] = [
    (SelectedBy::Random, "random"),
    (SelectedBy::Name, "name"),
    (SelectedBy::Album, "album"),
    (SelectedBy::Artist, "artist"),
    (SelectedBy::Genre, "genre"),
    (SelectedBy::HighestRating, "highest rating"),
    (SelectedBy::LowestRating, "lowest rating"),
    (SelectedBy::MostRecentlyPlayed, "most recently played"),
    (SelectedBy::LeastRecentlyPlayed, "least recently played"),
    (SelectedBy::MostOftenPlayed, "most often played"),
    (SelectedBy::LeastOftenPlayed, "least often played"),
    (SelectedBy::MostRecentlyAdded, "most recently added"),
    (SelectedBy::LeastRecentlyAdded, "least recently added"),
];

/// One rule row while it is being edited: every input as text.
#[derive(Clone, Debug, PartialEq)]
struct RuleDraft {
    field: Field,
    op: usize,
    text: String,
    low: String,
    high: String,
    unit: PeriodUnit,
}

impl RuleDraft {
    fn new() -> Self {
        Self {
            field: Field::Text(TextField::Artist),
            op: 0,
            text: String::new(),
            low: String::new(),
            high: String::new(),
            unit: PeriodUnit::Weeks,
        }
    }

    fn from_rule(rule: &Rule) -> Self {
        let mut draft = Self::new();
        match rule {
            Rule::Text { field, op, value } => {
                draft.field = Field::Text(*field);
                draft.op = [
                    TextOp::Contains,
                    TextOp::DoesNotContain,
                    TextOp::Is,
                    TextOp::IsNot,
                    TextOp::StartsWith,
                    TextOp::EndsWith,
                ]
                .iter()
                .position(|o| o == op)
                .unwrap_or(0);
                draft.text = value.clone();
            }
            Rule::Number { field, op } => {
                draft.field = Field::Number(*field);
                let (index, low, high) = match op {
                    NumberOp::Is(n) => (0, *n, 0),
                    NumberOp::IsNot(n) => (1, *n, 0),
                    NumberOp::GreaterThan(n) => (2, *n, 0),
                    NumberOp::LessThan(n) => (3, *n, 0),
                    NumberOp::InRange(a, b) => (4, *a, *b),
                };
                draft.op = index;
                draft.low = low.to_string();
                draft.high = high.to_string();
            }
            Rule::Date { field, op } => {
                draft.field = Field::Date(*field);
                match op {
                    DateOp::InTheLast(p) | DateOp::NotInTheLast(p) => {
                        draft.op = usize::from(matches!(op, DateOp::NotInTheLast(_)));
                        draft.low = p.amount.to_string();
                        draft.unit = p.unit;
                    }
                    DateOp::Before(d) | DateOp::After(d) => {
                        draft.op = if matches!(op, DateOp::Before(_)) { 2 } else { 3 };
                        draft.text = d.format("%Y-%m-%d").to_string();
                    }
                }
            }
        }
        draft
    }

    /// The rule, or `None` while an input is not a valid number or date.
    fn to_rule(&self) -> Option<Rule> {
        let number = |s: &str| s.trim().parse::<u64>().ok();
        Some(match self.field {
            Field::Text(field) => {
                let op = [
                    TextOp::Contains,
                    TextOp::DoesNotContain,
                    TextOp::Is,
                    TextOp::IsNot,
                    TextOp::StartsWith,
                    TextOp::EndsWith,
                ]
                .get(self.op)
                .copied()?;
                Rule::Text { field, op, value: self.text.clone() }
            }
            Field::Number(field) => {
                let low = number(&self.low)?;
                let op = match self.op {
                    0 => NumberOp::Is(low),
                    1 => NumberOp::IsNot(low),
                    2 => NumberOp::GreaterThan(low),
                    3 => NumberOp::LessThan(low),
                    _ => NumberOp::InRange(low, number(&self.high)?),
                };
                Rule::Number { field, op }
            }
            Field::Date(field) => {
                let op = match self.op {
                    0 | 1 => {
                        let period = Period { amount: u32::try_from(number(&self.low)?).ok()?, unit: self.unit };
                        if self.op == 0 { DateOp::InTheLast(period) } else { DateOp::NotInTheLast(period) }
                    }
                    2 => DateOp::Before(parse_date(&self.text)?),
                    _ => DateOp::After(parse_date(&self.text)?),
                };
                Rule::Date { field, op }
            }
        })
    }

    fn ops(&self) -> &'static [&'static str] {
        match self.field {
            Field::Text(_) => &TEXT_OPS,
            Field::Number(_) => &NUMBER_OPS,
            Field::Date(_) => &DATE_OPS,
        }
    }
}

/// A date typed as YYYY-MM-DD.
fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
}

#[derive(Clone, Debug)]
pub struct SmartPlaylistDialog {
    id: Option<PlaylistId>,
    name: String,
    match_mode: MatchMode,
    rules: Vec<RuleDraft>,
    limited: bool,
    limit_amount: String,
    limit_unit: LimitUnit,
    selected_by: SelectedBy,
    only_checked: bool,
    random_seed: u64,
}

impl SmartPlaylistDialog {
    /// A new smart playlist.
    pub fn new_rules() -> Self {
        Self::from_rules(None, "untitled smart playlist", &SmartRules { rules: vec![], ..SmartRules::default() })
    }

    /// Editing playlist `id`.
    pub fn from_rules(id: Option<PlaylistId>, name: &str, rules: &SmartRules) -> Self {
        let limit = rules.limit;
        let mut drafts: Vec<RuleDraft> = rules.rules.iter().map(RuleDraft::from_rule).collect();
        if drafts.is_empty() {
            drafts.push(RuleDraft::new());
        }
        Self {
            id,
            name: name.to_owned(),
            match_mode: rules.match_mode,
            rules: drafts,
            limited: limit.is_some(),
            limit_amount: limit.map_or_else(|| "25".to_owned(), |l| l.amount.to_string()),
            limit_unit: limit.map_or(LimitUnit::Items, |l| l.unit),
            selected_by: limit.map_or(SelectedBy::Random, |l| l.selected_by),
            only_checked: rules.only_checked,
            random_seed: rules.random_seed,
        }
    }

    fn to_rules(&self) -> Option<SmartRules> {
        let rules = self.rules.iter().map(RuleDraft::to_rule).collect::<Option<Vec<_>>>()?;
        let limit = if self.limited {
            Some(Limit {
                amount: self.limit_amount.trim().parse().ok()?,
                unit: self.limit_unit,
                selected_by: self.selected_by,
            })
        } else {
            None
        };
        Some(SmartRules {
            match_mode: self.match_mode,
            rules,
            limit,
            only_checked: self.only_checked,
            random_seed: self.random_seed,
        })
    }

    pub fn show(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let valid = self.to_rules().is_some();
        let result = window(ctx, "Smart Playlist", 640.0, "OK", |ui| self.body(ui, valid));
        match (result, self.to_rules()) {
            (Some(true), Some(rules)) => {
                actions.push(Action::SaveSmartPlaylist { id: self.id, name: self.name.trim().to_owned(), rules });
                false
            }
            (Some(true), None) => true,
            (Some(false), _) => false,
            (None, _) => true,
        }
    }

    fn body(&mut self, ui: &mut Ui, valid: bool) {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(300.0));
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Match");
            ComboBox::from_id_salt("match")
                .width(56.0)
                .selected_text(if self.match_mode == MatchMode::All { "all" } else { "any" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.match_mode, MatchMode::All, "all");
                    ui.selectable_value(&mut self.match_mode, MatchMode::Any, "any");
                });
            ui.label("of the following rules:");
        });
        let mut remove = None;
        let mut add_after = None;
        for (index, rule) in self.rules.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                rule_row(ui, index, rule);
                if ui.small_button("−").on_hover_text("Remove this rule").clicked() {
                    remove = Some(index);
                }
                if ui.small_button("+").on_hover_text("Add a rule").clicked() {
                    add_after = Some(index);
                }
            });
        }
        if let Some(index) = add_after {
            self.rules.insert(index + 1, RuleDraft::new());
        }
        if let Some(index) = remove
            && self.rules.len() > 1
        {
            self.rules.remove(index);
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.limited, "Limit to");
            ui.add_enabled(self.limited, egui::TextEdit::singleline(&mut self.limit_amount).desired_width(40.0));
            let unit = LIMIT_UNITS.iter().find(|(u, _)| *u == self.limit_unit).map_or("items", |(_, l)| *l);
            ui.add_enabled_ui(self.limited, |ui| {
                ComboBox::from_id_salt("limit-unit").width(70.0).selected_text(unit).show_ui(ui, |ui| {
                    for (value, label) in LIMIT_UNITS {
                        ui.selectable_value(&mut self.limit_unit, value, label);
                    }
                });
                ui.label("selected by");
                let by = SELECTED_BY.iter().find(|(s, _)| *s == self.selected_by).map_or("random", |(_, l)| *l);
                ComboBox::from_id_salt("selected-by").width(160.0).selected_text(by).show_ui(ui, |ui| {
                    for (value, label) in SELECTED_BY {
                        ui.selectable_value(&mut self.selected_by, value, label);
                    }
                });
            });
        });
        ui.checkbox(&mut self.only_checked, "Match only checked items");
        if !valid {
            ui.colored_label(
                egui::Color32::from_rgb(0xB0, 0x30, 0x30),
                "Enter a number (or a date as YYYY-MM-DD) in every rule.",
            );
        }
    }
}

fn rule_row(ui: &mut Ui, index: usize, rule: &mut RuleDraft) {
    let field_label = FIELDS.iter().find(|(f, _)| *f == rule.field).map_or("", |(_, l)| *l);
    ComboBox::from_id_salt(("field", index)).width(130.0).selected_text(field_label).show_ui(ui, |ui| {
        for (field, label) in FIELDS {
            if ui.selectable_label(rule.field == field, label).clicked() && rule.field != field {
                let same_kind = std::mem::discriminant(&rule.field) == std::mem::discriminant(&field);
                rule.field = field;
                if !same_kind {
                    rule.op = 0;
                }
            }
        }
    });
    let ops = rule.ops();
    let op_label = ops.get(rule.op).copied().unwrap_or_default();
    ComboBox::from_id_salt(("op", index)).width(120.0).selected_text(op_label).show_ui(ui, |ui| {
        for (i, label) in ops.iter().enumerate() {
            ui.selectable_value(&mut rule.op, i, *label);
        }
    });
    match (rule.field, rule.op) {
        (Field::Text(_), _) => {
            ui.add(egui::TextEdit::singleline(&mut rule.text).desired_width(180.0));
        }
        (Field::Number(_), 4) => {
            ui.add(egui::TextEdit::singleline(&mut rule.low).desired_width(60.0));
            ui.label("to");
            ui.add(egui::TextEdit::singleline(&mut rule.high).desired_width(60.0));
        }
        (Field::Number(_), _) => {
            ui.add(egui::TextEdit::singleline(&mut rule.low).desired_width(80.0));
        }
        (Field::Date(_), 0 | 1) => {
            ui.add(egui::TextEdit::singleline(&mut rule.low).desired_width(40.0));
            let unit = UNITS.iter().find(|(u, _)| *u == rule.unit).map_or("weeks", |(_, l)| *l);
            ComboBox::from_id_salt(("unit", index)).width(70.0).selected_text(unit).show_ui(ui, |ui| {
                for (value, label) in UNITS {
                    ui.selectable_value(&mut rule.unit, value, label);
                }
            });
        }
        (Field::Date(_), _) => {
            ui.add(egui::TextEdit::singleline(&mut rule.text).hint_text("YYYY-MM-DD").desired_width(100.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_survive_a_round_trip_through_the_editor() {
        for (name, rules) in tunebox_engine::model::smart::presets() {
            let dialog = SmartPlaylistDialog::from_rules(None, name, &rules);
            assert_eq!(dialog.to_rules(), Some(rules), "{name}");
        }
    }

    #[test]
    fn invalid_numbers_block_saving() {
        let mut draft = RuleDraft::new();
        draft.field = Field::Number(NumberField::Year);
        draft.low = "nineteen".into();
        assert_eq!(draft.to_rule(), None);
        draft.low = "1999".into();
        assert!(draft.to_rule().is_some());
    }
}
