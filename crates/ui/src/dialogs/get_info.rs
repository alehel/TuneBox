//! Get Info: a song's details, and editing them for one song or many at once. Two panes in the
//! style of Preferences: Summary (artwork, rating, play history, the file) and Details (the tags,
//! in cards). The everyday fields are always on Details; the additional ones are in
//! `extra_fields`.
//!
//! The artwork on Summary is a well: click it (or "Choose…") to pick a picture, "Remove" to
//! take the cover away, or drop an image file onto it. Each applies to every song the dialog
//! shows, straight away, as the rating does; the engine does the writing in the background.

use crate::actions::Action;
use crate::artwork::{self, ArtworkCache};
use crate::dialogs::extra_fields::{self, Disclosure, ExtraDraft};
use crate::dialogs::layout::{self, Tone, banner, card, fact_row, muted, switch_row, text_field, value_row};
use crate::dialogs::window;
use crate::glyphs::Glyph;
use crate::panels::widgets;
use crate::theme;
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use std::collections::BTreeMap;
use tunebox_engine::model::{
    Column, Rating, TagField, Track, TrackId, TrackMeta, format_bytes, format_clock, format_total_time,
};
use tunebox_engine::{JobKind, Session};

const WIDTH: f32 = 600.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Summary,
    Details,
}

const TABS: [(Tab, Glyph, &str); 2] =
    [(Tab::Summary, Glyph::Info, "Summary"), (Tab::Details, Glyph::Pencil, "Details")];

/// The editable fields as text. With several tracks a field holds the value they share, or is
/// empty when they differ; only fields the listener changes are written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Draft {
    title: String,
    artist: String,
    album_artist: String,
    album: String,
    genre: String,
    composer: String,
    grouping: String,
    comments: String,
    year: String,
    track: String,
    track_total: String,
    disc: String,
    disc_total: String,
    bpm: String,
    compilation: bool,
    extra: ExtraDraft,
}

impl Draft {
    fn of(meta: &TrackMeta) -> Self {
        let number = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
        Draft {
            title: meta.title.clone(),
            artist: meta.artist.clone(),
            album_artist: meta.album_artist.clone(),
            album: meta.album.clone(),
            genre: meta.genre.clone(),
            composer: meta.composer.clone(),
            grouping: meta.grouping.clone(),
            comments: meta.comments.clone(),
            year: number(meta.year),
            track: number(meta.track.number),
            track_total: number(meta.track.total),
            disc: number(meta.disc.number),
            disc_total: number(meta.disc.total),
            bpm: number(meta.bpm),
            compilation: meta.compilation,
            extra: ExtraDraft::of(meta),
        }
    }

    /// The fields all `drafts` agree on; others are left empty.
    fn common(drafts: &[Draft]) -> Self {
        let Some(first) = drafts.first() else { return Draft::default() };
        let same = |pick: fn(&Draft) -> &String| -> String {
            if drafts.iter().all(|d| pick(d) == pick(first)) { pick(first).clone() } else { String::new() }
        };
        Draft {
            title: same(|d| &d.title),
            artist: same(|d| &d.artist),
            album_artist: same(|d| &d.album_artist),
            album: same(|d| &d.album),
            genre: same(|d| &d.genre),
            composer: same(|d| &d.composer),
            grouping: same(|d| &d.grouping),
            comments: same(|d| &d.comments),
            year: same(|d| &d.year),
            track: same(|d| &d.track),
            track_total: same(|d| &d.track_total),
            disc: same(|d| &d.disc),
            disc_total: same(|d| &d.disc_total),
            bpm: same(|d| &d.bpm),
            compilation: drafts.iter().all(|d| d.compilation),
            extra: ExtraDraft::common(drafts.iter().map(|d| &d.extra)),
        }
    }

    /// `meta` with every field that differs between `original` and `self` replaced.
    fn apply(&self, original: &Draft, meta: &TrackMeta) -> TrackMeta {
        let mut out = meta.clone();
        let text = |edited: &String, before: &String, field: &mut String| {
            if edited != before {
                *field = edited.trim().to_owned();
            }
        };
        let number = |edited: &String, before: &String, field: &mut Option<u32>| {
            if edited != before {
                *field = edited.trim().parse().ok();
            }
        };
        text(&self.title, &original.title, &mut out.title);
        text(&self.artist, &original.artist, &mut out.artist);
        text(&self.album_artist, &original.album_artist, &mut out.album_artist);
        text(&self.album, &original.album, &mut out.album);
        text(&self.genre, &original.genre, &mut out.genre);
        text(&self.composer, &original.composer, &mut out.composer);
        text(&self.grouping, &original.grouping, &mut out.grouping);
        text(&self.comments, &original.comments, &mut out.comments);
        number(&self.year, &original.year, &mut out.year);
        number(&self.track, &original.track, &mut out.track.number);
        number(&self.track_total, &original.track_total, &mut out.track.total);
        number(&self.disc, &original.disc, &mut out.disc.number);
        number(&self.disc_total, &original.disc_total, &mut out.disc.total);
        number(&self.bpm, &original.bpm, &mut out.bpm);
        if self.compilation != original.compilation {
            out.compilation = self.compilation;
        }
        self.extra.apply(&original.extra, &mut out);
        out
    }
}

#[derive(Clone, Debug)]
pub struct GetInfoDialog {
    tracks: Vec<TrackId>,
    tab: Tab,
    original: Draft,
    draft: Draft,
    /// "Show all fields".
    all_fields: Disclosure,
}

impl GetInfoDialog {
    pub fn new(session: &Session, tracks: Vec<TrackId>) -> Self {
        let drafts: Vec<Draft> =
            tracks.iter().filter_map(|id| session.track(*id)).map(|t| Draft::of(&t.meta)).collect();
        let original = Draft::common(&drafts);
        let tab = if tracks.len() == 1 { Tab::Summary } else { Tab::Details };
        Self { tracks, tab, draft: original.clone(), original, all_fields: Disclosure::default() }
    }

    /// The same dialog, opened on the Details pane.
    pub fn on_details(mut self) -> Self {
        self.tab = Tab::Details;
        self
    }

    /// The same dialog with "Show all fields" open, or closed and scrolled into view.
    pub fn with_all_fields(mut self, open: bool) -> Self {
        self.all_fields = if open { Disclosure::revealed() } else { Disclosure::in_view() };
        self
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        artwork: &mut ArtworkCache,
        actions: &mut Vec<Action>,
    ) -> bool {
        let tracks: Vec<&Track> = self.tracks.iter().filter_map(|id| session.track(*id)).collect();
        let Some(first) = tracks.first().copied() else { return false };
        let title = if tracks.len() == 1 { first.title().to_owned() } else { format!("{} Songs", tracks.len()) };
        let protected =
            tracks.iter().filter(|t| !tunebox_engine::files::may_write(session.preferences(), &t.location)).count();
        let blocked = blocked_fields(&tracks);
        let extra = extra_fields::Context {
            pinned: &session.preferences().info_fields,
            blocked: &blocked,
            several: tracks.len() > 1,
        };
        let (tab, draft, all_fields) = (&mut self.tab, &mut self.draft, &mut self.all_fields);
        let result = window(ctx, &title, WIDTH, "OK", |ui| {
            layout::tab_bar(ui, tab, &TABS);
            layout::scrolling_body(ui, |ui| match tab {
                Tab::Summary => summary(ui, session, &tracks, artwork, actions),
                Tab::Details => details(ui, draft, tracks.len(), protected, &extra, all_fields),
            });
        });
        match result {
            Some(true) => {
                if self.draft != self.original {
                    let edits = tracks
                        .iter()
                        .map(|t| (t.id, self.draft.apply(&self.original, &t.meta)))
                        .filter(|(id, meta)| session.track(*id).is_some_and(|t| t.meta != *meta))
                        .collect();
                    actions.push(Action::EditTracks(edits));
                }
                false
            }
            Some(false) => false,
            None => true,
        }
    }
}

/// The additional fields some of `tracks` have no place for, each with the reason.
fn blocked_fields(tracks: &[&Track]) -> BTreeMap<TagField, String> {
    TagField::ALL
        .into_iter()
        .filter_map(|field| {
            let format = tracks.iter().map(|t| t.file.format).find(|f| !tunebox_engine::can_store(*f, field))?;
            let files = format.kind_label().trim_end_matches(" file");
            Some((field, format!("{files} files have no place for {}.", field.label().to_lowercase())))
        })
        .collect()
}

// ----- Summary -----

fn summary(ui: &mut Ui, session: &Session, tracks: &[&Track], artwork: &mut ArtworkCache, actions: &mut Vec<Action>) {
    let Some(first) = tracks.first().copied() else { return };
    card(ui, "", "", |ui| header(ui, session, tracks, first, artwork, actions));
    if let [track] = tracks {
        card(ui, "File", "", |ui| file_facts(ui, track, session.file_problem(track.id), actions));
    } else {
        let bytes: u64 = tracks.iter().map(|t| t.file.size_bytes).sum();
        let duration = tracks.iter().map(|t| t.file.duration).sum();
        card(ui, "Files", "", |ui| {
            fact_row(ui, "Songs", &tracks.len().to_string());
            fact_row(ui, "Total time", &format_total_time(duration));
            fact_row(ui, "Total size", &format_bytes(bytes));
        });
    }
}

/// Artwork beside the title, artist and album, the rating, and the play history as pills.
fn header(
    ui: &mut Ui,
    session: &Session,
    tracks: &[&Track],
    first: &Track,
    artwork: &mut ArtworkCache,
    actions: &mut Vec<Action>,
) {
    ui.horizontal_top(|ui| {
        artwork_well(ui, session, tracks, first, artwork, actions);
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.add_space(4.0);
            let single = tracks.len() == 1;
            let title = if single { first.title().to_owned() } else { format!("{} songs", tracks.len()) };
            ui.label(RichText::new(title).font(crate::fonts::bold(19.0)));
            ui.label(RichText::new(shared(tracks, |t| t.meta.artist.clone())).size(14.0));
            let album = shared(tracks, |t| t.meta.album.clone());
            let year = shared(tracks, |t| t.meta.year.map(|y| y.to_string()).unwrap_or_default());
            let line = [album, year].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
            ui.label(muted(&line));
            ui.add_space(10.0);
            rating(ui, tracks, actions);
            if single {
                ui.add_space(10.0);
                history(ui, first);
            }
        });
    });
}

/// The cover, which takes a new picture by click, button or drop, with Choose… and Remove below.
fn artwork_well(
    ui: &mut Ui,
    session: &Session,
    tracks: &[&Track],
    first: &Track,
    artwork: &mut ArtworkCache,
    actions: &mut Vec<Action>,
) {
    const SIZE: f32 = 140.0;
    let ids: Vec<TrackId> = tracks.iter().map(|t| t.id).collect();
    let busy = session.is_busy(JobKind::Artwork);
    ui.vertical(|ui| {
        ui.set_width(SIZE);
        let sense = if busy { Sense::hover() } else { Sense::click() };
        let (art, response) = ui.allocate_exact_size(vec2(SIZE, SIZE), sense);
        theme::soft_shadow(ui.painter(), art, 6.0, 10);
        let texture = artwork.cover(session, first);
        artwork::paint(ui, art, texture.as_ref());
        let dragging = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        if !busy && (dragging || response.hovered()) {
            ui.painter().rect_stroke(art, 6.0, Stroke::new(2.0, theme::ACCENT), StrokeKind::Inside);
        }
        if !busy && dragging {
            drop_hint(ui, art);
        }
        if response.on_hover_text("Choose artwork…").clicked() {
            actions.push(Action::ChooseArtwork(ids.clone()));
        }
        if !busy && let Some(image) = dropped_on(ui, art) {
            actions.push(Action::SetArtwork(ids.clone(), image));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if ui.add_enabled(!busy, egui::Button::new("Choose…")).clicked() {
                actions.push(Action::ChooseArtwork(ids.clone()));
            }
            let has_cover = texture.is_some();
            if ui.add_enabled(!busy && has_cover, egui::Button::new("Remove")).clicked() {
                actions.push(Action::RemoveArtwork(ids.clone()));
            }
        });
        let find = ui
            .add_enabled(!busy, egui::Button::new("Find Online…").min_size(vec2(SIZE, 0.0)))
            .on_hover_text("Search MusicBrainz and the Cover Art Archive for this album's cover");
        if find.clicked() {
            actions.push(Action::OpenFindArtwork(ids.clone()));
        }
    });
}

/// Over the well while a file is dragged above the window: where to let go.
fn drop_hint(ui: &Ui, art: Rect) {
    let painter = ui.painter();
    painter.rect_filled(art.shrink(2.0), 5.0, Color32::from_black_alpha(110));
    painter.text(art.center(), Align2::CENTER_CENTER, "Drop to use", FontId::proportional(13.0), Color32::WHITE);
}

/// The file dropped onto `art` this frame. Some systems do not say where the pointer is while
/// a file is dragged from another program; then a drop anywhere counts, since the dialog is
/// the only thing that can take it.
fn dropped_on(ui: &Ui, art: Rect) -> Option<std::path::PathBuf> {
    ui.ctx().input(|i| {
        let file = i.raw.dropped_files.first()?.path().to_path_buf();
        let over = i.pointer.latest_pos().is_none_or(|p| art.expand(12.0).contains(p));
        over.then_some(file)
    })
}

/// `pick` of every track if they all agree, else "Various".
fn shared(tracks: &[&Track], pick: impl Fn(&Track) -> String) -> String {
    let values: Vec<String> = tracks.iter().map(|t| pick(t)).collect();
    match values.first() {
        Some(first) if values.iter().all(|v| v == first) => first.clone(),
        Some(_) => "Various".to_owned(),
        None => String::new(),
    }
}

/// The stars, set straight away as in the song list.
fn rating(ui: &mut Ui, tracks: &[&Track], actions: &mut Vec<Action>) {
    let current = tracks
        .first()
        .map(|t| t.stats.rating)
        .filter(|r| tracks.iter().all(|t| t.stats.rating == *r))
        .unwrap_or(Rating::UNRATED);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(62.0, 18.0), Sense::hover());
        if let Some(rating) = widgets::rating(ui, rect, current, true, false) {
            actions.push(Action::SetRating(tracks.iter().map(|t| t.id).collect(), rating));
        }
        ui.label(muted(if current.is_rated() { "Your rating" } else { "Not rated" }));
    });
}

/// Plays, last played and date added, as small pills.
fn history(ui: &mut Ui, track: &Track) {
    let plays = match track.stats.play_count {
        0 => "Never played".to_owned(),
        1 => "Played once".to_owned(),
        n => format!("Played {n} times"),
    };
    let last = track.stats.last_played.map(|d| format!("Last played {}", d.format("%-d %b %Y")));
    let added = format!("Added {}", track.date_added.format("%-d %b %Y"));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for text in std::iter::once(plays).chain(last).chain(std::iter::once(added)) {
            pill(ui, &text);
        }
    });
}

fn pill(ui: &mut Ui, text: &str) {
    let font = egui::FontId::proportional(11.5);
    let width = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), theme::TEXT).size().x + 18.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    ui.painter().rect_filled(rect, 11.0, Color32::from_rgb(0xEE, 0xF0, 0xF3));
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, font, theme::TEXT_MUTED);
}

/// Format, size, quality and where the file is.
fn file_facts(ui: &mut Ui, track: &Track, problem: Option<String>, actions: &mut Vec<Action>) {
    if let Some(problem) = problem {
        banner(ui, Tone::Warning, Glyph::Missing, &problem);
    }
    fact_row(ui, "Kind", &Column::Kind.text(track));
    fact_row(ui, "Length", &format_clock(track.file.duration));
    fact_row(ui, "Size", &format_bytes(track.file.size_bytes));
    let quality = [Column::BitRate.text(track), Column::SampleRate.text(track), channels(track.file.channels)]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    fact_row(ui, "Quality", &quality);
    fact_row(ui, "Where", &tunebox_platform::display_path(&track.location));
    value_row(ui, "", |ui| {
        if ui.button("Show in Folder").clicked() {
            actions.push(Action::ShowInFolder(track.id));
        }
    });
}

fn channels(count: Option<u8>) -> String {
    match count {
        Some(1) => "Mono".into(),
        Some(2) => "Stereo".into(),
        Some(n) => format!("{n} channels"),
        None => String::new(),
    }
}

// ----- Details -----

fn details(
    ui: &mut Ui,
    draft: &mut Draft,
    count: usize,
    protected: usize,
    extra: &extra_fields::Context<'_>,
    all_fields: &mut Disclosure,
) {
    if count > 1 {
        let text = format!(
            "Editing {count} songs. Fields where they differ are empty and stay as they are unless you type in them."
        );
        banner(ui, Tone::Info, Glyph::Info, &text);
    }
    if protected > 0 {
        let text = if count == 1 {
            "This song is in a read-only library folder: changes are kept in TuneBox's library and the file is not touched."
        } else {
            "Some of these songs are in a read-only library folder: for them, changes are kept in TuneBox's library only."
        };
        banner(ui, Tone::Info, Glyph::Lock, text);
    }
    let several = count > 1;
    card(ui, "Song", "", |ui| {
        if !several {
            text_field(ui, "Name", &mut draft.title, "");
        }
        text_field(ui, "Artist", &mut draft.artist, hint(several));
        text_field(ui, "Composer", &mut draft.composer, hint(several));
        text_field(ui, "Grouping", &mut draft.grouping, hint(several));
    });
    card(ui, "Album", "", |ui| {
        text_field(ui, "Album", &mut draft.album, hint(several));
        text_field(ui, "Album artist", &mut draft.album_artist, hint(several));
        value_row(ui, "Genre", |ui| {
            ui.add(egui::TextEdit::singleline(&mut draft.genre).hint_text(hint(several)).desired_width(200.0));
            ui.add_space(12.0);
            ui.label(RichText::new("Year").size(13.0).color(theme::TEXT_MUTED));
            ui.add(egui::TextEdit::singleline(&mut draft.year).desired_width(60.0));
        });
        value_row(ui, "Track", |ui| {
            if !several {
                ui.add(egui::TextEdit::singleline(&mut draft.track).desired_width(44.0));
            }
            ui.label(muted("of"));
            ui.add(egui::TextEdit::singleline(&mut draft.track_total).desired_width(44.0));
            ui.add_space(18.0);
            ui.label(RichText::new("Disc").size(13.0).color(theme::TEXT_MUTED));
            ui.add(egui::TextEdit::singleline(&mut draft.disc).desired_width(44.0));
            ui.label(muted("of"));
            ui.add(egui::TextEdit::singleline(&mut draft.disc_total).desired_width(44.0));
            ui.add_space(18.0);
            ui.label(RichText::new("BPM").size(13.0).color(theme::TEXT_MUTED));
            ui.add(egui::TextEdit::singleline(&mut draft.bpm).desired_width(50.0));
        });
        ui.add_space(6.0);
        switch_row(
            ui,
            "Part of a compilation",
            "Filed under Compilations rather than the artist.",
            &mut draft.compilation,
        );
    });
    extra_fields::pinned(ui, &mut draft.extra, extra);
    card(ui, "Comments", "", |ui| {
        ui.add(egui::TextEdit::multiline(&mut draft.comments).desired_rows(3).desired_width(f32::INFINITY));
    });
    extra_fields::the_rest(ui, &mut draft.extra, extra, all_fields);
}

/// The hint in an empty field when editing several songs: left empty, it is not changed.
fn hint(several: bool) -> &'static str {
    if several { "Keep as is" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunebox_engine::model::NumberOf;

    #[test]
    fn editing_several_tracks_changes_only_edited_fields() {
        let a = TrackMeta { title: "A".into(), artist: "One".into(), album: "X".into(), ..TrackMeta::default() };
        let b = TrackMeta {
            title: "B".into(),
            artist: "Two".into(),
            album: "X".into(),
            track: NumberOf::new(2, 9),
            ..TrackMeta::default()
        };
        let original = Draft::common(&[Draft::of(&a), Draft::of(&b)]);
        assert_eq!(original.artist, "", "differing values start empty");
        assert_eq!(original.album, "X");
        let mut draft = original.clone();
        draft.genre = "Jazz".into();
        draft.album = "Y".into();
        let edited = draft.apply(&original, &b);
        assert_eq!((edited.title.as_str(), edited.artist.as_str()), ("B", "Two"));
        assert_eq!((edited.genre.as_str(), edited.album.as_str()), ("Jazz", "Y"));
        assert_eq!(edited.track, NumberOf::new(2, 9));
    }
}
