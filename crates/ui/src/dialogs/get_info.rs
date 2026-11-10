//! Get Info: a track's details, and editing them for one track or many at once.

use crate::actions::Action;
use crate::artwork::{self, ArtworkCache};
use crate::dialogs::window;
use egui::{Grid, RichText, Ui, vec2};
use tunebox_engine::Session;
use tunebox_engine::model::{Column, Track, TrackId, TrackMeta, format_bytes, format_clock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Summary,
    Info,
}

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
        out
    }
}

#[derive(Clone, Debug)]
pub struct GetInfoDialog {
    tracks: Vec<TrackId>,
    tab: Tab,
    original: Draft,
    draft: Draft,
}

impl GetInfoDialog {
    pub fn new(session: &Session, tracks: Vec<TrackId>) -> Self {
        let drafts: Vec<Draft> =
            tracks.iter().filter_map(|id| session.track(*id)).map(|t| Draft::of(&t.meta)).collect();
        let original = Draft::common(&drafts);
        let tab = if tracks.len() == 1 { Tab::Summary } else { Tab::Info };
        Self { tracks, tab, draft: original.clone(), original }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        artwork: &mut ArtworkCache,
        actions: &mut Vec<Action>,
    ) -> bool {
        let single = (self.tracks.len() == 1).then(|| self.tracks.first().and_then(|id| session.track(*id))).flatten();
        let title = match single {
            Some(track) => track.title().to_owned(),
            None => format!("{} songs", self.tracks.len()),
        };
        let (tab, draft) = (&mut self.tab, &mut self.draft);
        let result = window(ctx, &title, 520.0, "OK", |ui| {
            if single.is_some() {
                ui.horizontal(|ui| {
                    ui.selectable_value(tab, Tab::Summary, "Summary");
                    ui.selectable_value(tab, Tab::Info, "Info");
                });
                ui.separator();
            }
            match (*tab, single) {
                (Tab::Summary, Some(track)) => summary(ui, track, artwork),
                _ => info(ui, draft, single.is_none()),
            }
        });
        match result {
            Some(true) => {
                if self.draft != self.original {
                    let edits = self
                        .tracks
                        .iter()
                        .filter_map(|id| session.track(*id))
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

fn summary(ui: &mut Ui, track: &Track, artwork: &mut ArtworkCache) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(150.0, 150.0), egui::Sense::hover());
        let texture = artwork.get(track);
        artwork::paint(ui, rect, texture.as_ref());
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(track.title()).size(16.0).strong());
            ui.label(&track.meta.artist);
            ui.label(&track.meta.album);
            ui.label(format_clock(track.file.duration));
        });
    });
    ui.add_space(8.0);
    Grid::new("summary").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        let mut row = |label: &str, value: String| {
            ui.label(RichText::new(label).weak());
            ui.label(value);
            ui.end_row();
        };
        row("Kind", Column::Kind.text(track));
        row("Size", format_bytes(track.file.size_bytes));
        row("Bit Rate", Column::BitRate.text(track));
        row("Sample Rate", Column::SampleRate.text(track));
        row("Date Added", Column::DateAdded.text(track));
        row("Play Count", track.stats.play_count.to_string());
        row("Last Played", Column::LastPlayed.text(track));
        ui.label(RichText::new("Where").weak());
        ui.add(egui::Label::new(tunebox_platform::display_path(&track.location)).wrap());
        ui.end_row();
    });
}

fn info(ui: &mut Ui, draft: &mut Draft, several: bool) {
    if several {
        ui.label(RichText::new("Fields left empty differ between the songs and are not changed.").weak().size(11.0));
        ui.add_space(4.0);
    }
    Grid::new("info").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        let text = |ui: &mut Ui, label: &str, value: &mut String| {
            ui.label(label);
            ui.add(egui::TextEdit::singleline(value).desired_width(380.0));
            ui.end_row();
        };
        if !several {
            text(ui, "Name", &mut draft.title);
        }
        text(ui, "Artist", &mut draft.artist);
        text(ui, "Album Artist", &mut draft.album_artist);
        text(ui, "Album", &mut draft.album);
        text(ui, "Genre", &mut draft.genre);
        text(ui, "Composer", &mut draft.composer);
        text(ui, "Grouping", &mut draft.grouping);
        text(ui, "Comments", &mut draft.comments);
        ui.label("Year");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut draft.year).desired_width(60.0));
            ui.label("BPM");
            ui.add(egui::TextEdit::singleline(&mut draft.bpm).desired_width(50.0));
        });
        ui.end_row();
        ui.label("Track");
        of_pair(ui, &mut draft.track, &mut draft.track_total, several);
        ui.end_row();
        ui.label("Disc");
        of_pair(ui, &mut draft.disc, &mut draft.disc_total, false);
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut draft.compilation, "Part of a compilation");
        ui.end_row();
    });
}

fn of_pair(ui: &mut Ui, number: &mut String, total: &mut String, total_only: bool) {
    ui.horizontal(|ui| {
        if !total_only {
            ui.add(egui::TextEdit::singleline(number).desired_width(40.0));
        }
        ui.label("of");
        ui.add(egui::TextEdit::singleline(total).desired_width(40.0));
    });
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
