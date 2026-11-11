//! Smart playlists: rules that pick tracks, an optional limit, and the order the limit keeps.
//! They are always live: the library re-evaluates them whenever it is read.
//!
//! Evaluation is pure. "Now" is a parameter and random selection uses a seed stored with the
//! rules, so a smart playlist shows the same tracks until the library or the seed changes.

use crate::{Track, TrackId};
/// Calendar dates in date rules.
pub use chrono::NaiveDate;
use chrono::{DateTime, Months, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Whether a track must satisfy every rule or just one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchMode {
    #[default]
    All,
    Any,
}

/// A text field a rule can test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextField {
    Name,
    Artist,
    AlbumArtist,
    Album,
    Genre,
    Composer,
    Grouping,
    Comments,
    Kind,
    /// Music or Audiobook.
    MediaKind,
}

/// How a text rule compares (case-insensitive).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextOp {
    Contains,
    DoesNotContain,
    Is,
    IsNot,
    StartsWith,
    EndsWith,
}

/// A numeric field a rule can test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberField {
    /// In stars, 0–5.
    Rating,
    Plays,
    Skips,
    Year,
    BitRate,
    /// Length in seconds.
    Time,
    Bpm,
    TrackNumber,
}

/// How a numeric rule compares. A track without a value for the field never matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberOp {
    Is(u64),
    IsNot(u64),
    GreaterThan(u64),
    LessThan(u64),
    /// Inclusive at both ends.
    InRange(u64, u64),
}

/// A date field a rule can test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateField {
    DateAdded,
    LastPlayed,
    LastSkipped,
}

/// A span of time counted back from now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Period {
    pub amount: u32,
    pub unit: PeriodUnit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeriodUnit {
    Days,
    Weeks,
    Months,
}

/// How a date rule compares. "In the last" never matches a date that was never set ("never
/// played"); "not in the last" always does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateOp {
    InTheLast(Period),
    NotInTheLast(Period),
    Before(NaiveDate),
    After(NaiveDate),
}

/// One condition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rule {
    Text { field: TextField, op: TextOp, value: String },
    Number { field: NumberField, op: NumberOp },
    Date { field: DateField, op: DateOp },
}

/// What a limit counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LimitUnit {
    Items,
    Minutes,
    Hours,
    Megabytes,
    Gigabytes,
}

/// Which tracks a limit keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectedBy {
    Random,
    Name,
    Album,
    Artist,
    Genre,
    HighestRating,
    LowestRating,
    MostRecentlyPlayed,
    LeastRecentlyPlayed,
    MostOftenPlayed,
    LeastOftenPlayed,
    MostRecentlyAdded,
    LeastRecentlyAdded,
}

/// "Limit to 25 items selected by most often played".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limit {
    pub amount: u32,
    pub unit: LimitUnit,
    pub selected_by: SelectedBy,
}

/// A smart playlist's definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartRules {
    pub match_mode: MatchMode,
    pub rules: Vec<Rule>,
    pub limit: Option<Limit>,
    /// "Match only checked items".
    pub only_checked: bool,
    /// Seed for "selected by random"; change it to reshuffle.
    pub random_seed: u64,
}

impl Default for SmartRules {
    fn default() -> Self {
        Self { match_mode: MatchMode::All, rules: Vec::new(), limit: None, only_checked: false, random_seed: 0 }
    }
}

impl SmartRules {
    /// Whether `track` passes the rules (ignoring the limit). No rules match everything.
    pub fn matches(&self, track: &Track, now: DateTime<Utc>) -> bool {
        if self.only_checked && !track.checked {
            return false;
        }
        if self.rules.is_empty() {
            return true;
        }
        match self.match_mode {
            MatchMode::All => self.rules.iter().all(|r| r.matches(track, now)),
            MatchMode::Any => self.rules.iter().any(|r| r.matches(track, now)),
        }
    }

    /// The playlist's tracks: those that match, then ordered and cut by the limit.
    pub fn evaluate<'a>(&self, tracks: impl IntoIterator<Item = &'a Track>, now: DateTime<Utc>) -> Vec<TrackId> {
        let mut matching: Vec<&Track> = tracks.into_iter().filter(|t| self.matches(t, now)).collect();
        let Some(limit) = self.limit else {
            matching.sort_by_key(|t| t.id);
            return matching.into_iter().map(|t| t.id).collect();
        };
        matching.sort_by(|a, b| selection_order(limit.selected_by, self.random_seed, a, b));
        apply_limit(&matching, limit)
    }
}

impl Rule {
    pub fn matches(&self, track: &Track, now: DateTime<Utc>) -> bool {
        match self {
            Rule::Text { field, op, value } => {
                text_matches(&text_value(*field, track).to_lowercase(), *op, &value.to_lowercase())
            }
            Rule::Number { field, op } => number_value(*field, track).is_some_and(|v| number_matches(v, *op)),
            Rule::Date { field, op } => date_matches(date_value(*field, track), *op, now),
        }
    }
}

fn text_value(field: TextField, track: &Track) -> &str {
    let m = &track.meta;
    match field {
        TextField::Name => track.title(),
        TextField::Artist => &m.artist,
        TextField::AlbumArtist => &m.album_artist,
        TextField::Album => &m.album,
        TextField::Genre => &m.genre,
        TextField::Composer => &m.composer,
        TextField::Grouping => &m.grouping,
        TextField::Comments => &m.comments,
        TextField::Kind => track.file.format.kind_label(),
        TextField::MediaKind => track.kind.name(),
    }
}

fn text_matches(haystack: &str, op: TextOp, needle: &str) -> bool {
    match op {
        TextOp::Contains => haystack.contains(needle),
        TextOp::DoesNotContain => !haystack.contains(needle),
        TextOp::Is => haystack == needle,
        TextOp::IsNot => haystack != needle,
        TextOp::StartsWith => haystack.starts_with(needle),
        TextOp::EndsWith => haystack.ends_with(needle),
    }
}

fn number_value(field: NumberField, track: &Track) -> Option<u64> {
    let m = &track.meta;
    match field {
        NumberField::Rating => Some(u64::from(track.stats.rating.stars())),
        NumberField::Plays => Some(u64::from(track.stats.play_count)),
        NumberField::Skips => Some(u64::from(track.stats.skip_count)),
        NumberField::Year => m.year.map(u64::from),
        NumberField::BitRate => track.file.bit_rate_kbps.map(u64::from),
        NumberField::Time => Some(track.file.duration.as_secs()),
        NumberField::Bpm => m.bpm.map(u64::from),
        NumberField::TrackNumber => m.track.number.map(u64::from),
    }
}

fn number_matches(value: u64, op: NumberOp) -> bool {
    match op {
        NumberOp::Is(n) => value == n,
        NumberOp::IsNot(n) => value != n,
        NumberOp::GreaterThan(n) => value > n,
        NumberOp::LessThan(n) => value < n,
        NumberOp::InRange(lo, hi) => (lo.min(hi)..=lo.max(hi)).contains(&value),
    }
}

fn date_value(field: DateField, track: &Track) -> Option<DateTime<Utc>> {
    match field {
        DateField::DateAdded => Some(track.date_added),
        DateField::LastPlayed => track.stats.last_played,
        DateField::LastSkipped => track.stats.last_skipped,
    }
}

fn date_matches(value: Option<DateTime<Utc>>, op: DateOp, now: DateTime<Utc>) -> bool {
    match op {
        DateOp::InTheLast(period) => value.is_some_and(|v| v >= period_start(now, period)),
        DateOp::NotInTheLast(period) => value.is_none_or(|v| v < period_start(now, period)),
        DateOp::Before(day) => value.is_some_and(|v| v.date_naive() < day),
        DateOp::After(day) => value.is_some_and(|v| v.date_naive() > day),
    }
}

/// The moment `period` before `now`, saturating at the earliest representable time.
fn period_start(now: DateTime<Utc>, period: Period) -> DateTime<Utc> {
    let earlier = match period.unit {
        PeriodUnit::Days => TimeDelta::try_days(i64::from(period.amount)).and_then(|d| now.checked_sub_signed(d)),
        PeriodUnit::Weeks => TimeDelta::try_weeks(i64::from(period.amount)).and_then(|d| now.checked_sub_signed(d)),
        PeriodUnit::Months => now.checked_sub_months(Months::new(period.amount)),
    };
    earlier.unwrap_or(DateTime::<Utc>::MIN_UTC)
}

/// The order in which a limit keeps tracks.
fn selection_order(by: SelectedBy, seed: u64, a: &Track, b: &Track) -> Ordering {
    use crate::{Column, SortOrder, compare_tracks};
    let by_column = |column, order| compare_tracks(a, b, column, order);
    match by {
        SelectedBy::Random => random_key(seed, a.id).cmp(&random_key(seed, b.id)),
        SelectedBy::Name => by_column(Column::Name, SortOrder::Ascending),
        SelectedBy::Album => by_column(Column::Album, SortOrder::Ascending),
        SelectedBy::Artist => by_column(Column::Artist, SortOrder::Ascending),
        SelectedBy::Genre => by_column(Column::Genre, SortOrder::Ascending),
        SelectedBy::HighestRating => by_column(Column::Rating, SortOrder::Descending),
        SelectedBy::LowestRating => by_column(Column::Rating, SortOrder::Ascending),
        SelectedBy::MostRecentlyPlayed => by_column(Column::LastPlayed, SortOrder::Descending),
        SelectedBy::LeastRecentlyPlayed => by_column(Column::LastPlayed, SortOrder::Ascending),
        SelectedBy::MostOftenPlayed => by_column(Column::Plays, SortOrder::Descending),
        SelectedBy::LeastOftenPlayed => by_column(Column::Plays, SortOrder::Ascending),
        SelectedBy::MostRecentlyAdded => by_column(Column::DateAdded, SortOrder::Descending),
        SelectedBy::LeastRecentlyAdded => by_column(Column::DateAdded, SortOrder::Ascending),
    }
}

fn random_key(seed: u64, id: TrackId) -> u64 {
    let mut hasher = DefaultHasher::new();
    (seed, id).hash(&mut hasher);
    hasher.finish()
}

/// Keeps tracks from the front of `ordered` until the limit is reached. A track that would
/// overshoot a time or size limit is skipped and smaller ones after it may still fit.
fn apply_limit(ordered: &[&Track], limit: Limit) -> Vec<TrackId> {
    let budget = u64::from(limit.amount);
    let cost = |t: &Track| -> u64 {
        match limit.unit {
            LimitUnit::Items => 1,
            LimitUnit::Minutes | LimitUnit::Hours => t.file.duration.as_secs(),
            LimitUnit::Megabytes | LimitUnit::Gigabytes => t.file.size_bytes,
        }
    };
    let capacity = match limit.unit {
        LimitUnit::Items => budget,
        LimitUnit::Minutes => budget.saturating_mul(60),
        LimitUnit::Hours => budget.saturating_mul(3600),
        LimitUnit::Megabytes => budget.saturating_mul(1_000_000),
        LimitUnit::Gigabytes => budget.saturating_mul(1_000_000_000),
    };
    let mut used = 0u64;
    let mut kept = Vec::new();
    for track in ordered {
        let next = used.saturating_add(cost(track));
        if next <= capacity {
            used = next;
            kept.push(track.id);
        } else if limit.unit == LimitUnit::Items {
            break;
        }
    }
    kept
}

/// The smart playlists a new library starts with.
pub fn presets() -> Vec<(&'static str, SmartRules)> {
    let rules = |rules: Vec<Rule>, limit: Option<Limit>| SmartRules { rules, limit, ..SmartRules::default() };
    let year = |op| Rule::Number { field: NumberField::Year, op };
    let limit = |amount, unit, selected_by| Some(Limit { amount, unit, selected_by });
    vec![
        ("90's Music", rules(vec![year(NumberOp::InRange(1990, 1999))], None)),
        (
            "Classical Music",
            rules(vec![Rule::Text { field: TextField::Genre, op: TextOp::Is, value: "Classical".into() }], None),
        ),
        ("My Top Rated", rules(vec![Rule::Number { field: NumberField::Rating, op: NumberOp::GreaterThan(3) }], None)),
        (
            "Recently Added",
            rules(
                vec![Rule::Date {
                    field: DateField::DateAdded,
                    op: DateOp::InTheLast(Period { amount: 2, unit: PeriodUnit::Weeks }),
                }],
                None,
            ),
        ),
        (
            "Recently Played",
            rules(
                vec![Rule::Date {
                    field: DateField::LastPlayed,
                    op: DateOp::InTheLast(Period { amount: 2, unit: PeriodUnit::Weeks }),
                }],
                None,
            ),
        ),
        (
            "Top 25 Most Played",
            rules(
                vec![Rule::Number { field: NumberField::Plays, op: NumberOp::GreaterThan(0) }],
                limit(25, LimitUnit::Items, SelectedBy::MostOftenPlayed),
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rating;
    use crate::track::fixtures::track;
    use chrono::TimeZone;
    use std::time::Duration;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).single().unwrap()
    }

    fn library() -> Vec<Track> {
        let mut a = track(1, "Alpha", "Ann", "First");
        a.meta.year = Some(1994);
        a.stats.rating = Rating::from_stars(5);
        a.stats.play_count = 10;
        let mut b = track(2, "Beta", "Bob", "Second");
        b.meta.year = Some(2004);
        b.meta.genre = "Jazz".into();
        b.stats.play_count = 3;
        b.stats.last_played = Some(now() - TimeDelta::days(3));
        let mut c = track(3, "Gamma", "Cid", "Third");
        c.checked = false;
        c.stats.play_count = 7;
        vec![a, b, c]
    }

    fn ids(v: &[TrackId]) -> Vec<u64> {
        v.iter().map(|id| id.0).collect()
    }

    #[test]
    fn all_versus_any() {
        let lib = library();
        let jazz = Rule::Text { field: TextField::Genre, op: TextOp::Is, value: "jazz".into() };
        let nineties = Rule::Number { field: NumberField::Year, op: NumberOp::InRange(1999, 1990) };
        let mut rules = SmartRules { rules: vec![jazz, nineties], ..SmartRules::default() };
        assert!(rules.evaluate(&lib, now()).is_empty());
        rules.match_mode = MatchMode::Any;
        assert_eq!(ids(&rules.evaluate(&lib, now())), [1, 2]);
    }

    #[test]
    fn media_kind_rules_tell_music_from_audiobooks() {
        let mut lib = library();
        lib[1].kind = crate::MediaKind::Audiobook;
        let rules = SmartRules {
            rules: vec![Rule::Text { field: TextField::MediaKind, op: TextOp::Is, value: "audiobook".into() }],
            ..SmartRules::default()
        };
        assert_eq!(ids(&rules.evaluate(&lib, now())), [2]);
    }

    #[test]
    fn missing_numbers_never_match() {
        let lib = library();
        let rules = SmartRules {
            rules: vec![Rule::Number { field: NumberField::Year, op: NumberOp::IsNot(1994) }],
            ..SmartRules::default()
        };
        assert_eq!(ids(&rules.evaluate(&lib, now())), [2]);
    }

    #[test]
    fn never_played_is_not_in_the_last_period() {
        let lib = library();
        let week = Period { amount: 1, unit: PeriodUnit::Weeks };
        let recent = SmartRules {
            rules: vec![Rule::Date { field: DateField::LastPlayed, op: DateOp::InTheLast(week) }],
            ..SmartRules::default()
        };
        assert_eq!(ids(&recent.evaluate(&lib, now())), [2]);
        let stale = SmartRules {
            rules: vec![Rule::Date { field: DateField::LastPlayed, op: DateOp::NotInTheLast(week) }],
            ..SmartRules::default()
        };
        assert_eq!(ids(&stale.evaluate(&lib, now())), [1, 3]);
    }

    #[test]
    fn only_checked_skips_unchecked() {
        let lib = library();
        let rules = SmartRules { only_checked: true, ..SmartRules::default() };
        assert_eq!(ids(&rules.evaluate(&lib, now())), [1, 2]);
    }

    #[test]
    fn item_limit_keeps_the_selected_order() {
        let lib = library();
        let rules = SmartRules {
            limit: Some(Limit { amount: 2, unit: LimitUnit::Items, selected_by: SelectedBy::MostOftenPlayed }),
            ..SmartRules::default()
        };
        assert_eq!(ids(&rules.evaluate(&lib, now())), [1, 3]);
    }

    #[test]
    fn time_limit_skips_tracks_that_do_not_fit() {
        let mut lib = library();
        lib[0].file.duration = Duration::from_secs(50 * 60);
        lib[1].file.duration = Duration::from_secs(20 * 60);
        lib[2].file.duration = Duration::from_secs(5 * 60);
        let rules = SmartRules {
            limit: Some(Limit { amount: 1, unit: LimitUnit::Hours, selected_by: SelectedBy::Name }),
            ..SmartRules::default()
        };
        assert_eq!(ids(&rules.evaluate(&lib, now())), [1, 3]);
    }

    #[test]
    fn random_selection_is_stable_for_a_seed() {
        let lib = library();
        let mut rules = SmartRules {
            limit: Some(Limit { amount: 3, unit: LimitUnit::Items, selected_by: SelectedBy::Random }),
            ..SmartRules::default()
        };
        let first = rules.evaluate(&lib, now());
        assert_eq!(first, rules.evaluate(&lib, now()));
        let mut seen_other_order = false;
        for seed in 1..20 {
            rules.random_seed = seed;
            seen_other_order |= rules.evaluate(&lib, now()) != first;
        }
        assert!(seen_other_order);
    }

    #[test]
    fn presets_include_top_25() {
        let lib = library();
        let (_, top) = presets().into_iter().find(|(name, _)| *name == "Top 25 Most Played").unwrap();
        assert_eq!(ids(&top.evaluate(&lib, now())), [1, 3, 2]);
    }
}
