//! "Find Album Artwork…": the releases MusicBrainz knows for an album, each with its cover from
//! the Cover Art Archive and what tells the editions apart. The listener picks one; nothing is
//! applied until they choose "Use Cover".

use super::layout::muted;
use super::{Dialog, window};
use crate::actions::Action;
use crate::artwork::{self, ArtworkCache};
use crate::theme;
use egui::{Rect, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use tunebox_engine::model::TrackId;
use tunebox_engine::{CoverCandidate, CoverOf, SearchState, Session, Thumbnail};

const WIDTH: f32 = 560.0;
const THUMB: f32 = 64.0;
const ROW: f32 = THUMB + 12.0;

#[derive(Clone, Debug)]
pub struct FindArtworkDialog {
    tracks: Vec<TrackId>,
    artist: String,
    album: String,
    /// The release the listener picked, by id.
    chosen: Option<String>,
    /// The dialog this one was opened from (Get Info), shown again when it closes.
    return_to: Option<Box<Dialog>>,
}

impl FindArtworkDialog {
    /// The dialog for `tracks`, searching for `artist` and `album`.
    pub fn new(tracks: Vec<TrackId>, artist: String, album: String, return_to: Option<Box<Dialog>>) -> Self {
        Self { tracks, artist, album, chosen: None, return_to }
    }

    /// The search to start with the dialog.
    pub fn search_action(&self) -> Action {
        Action::FindArtwork { tracks: self.tracks.clone(), artist: self.artist.clone(), album: self.album.clone() }
    }

    pub fn album_is_empty(&self) -> bool {
        self.album.trim().is_empty()
    }

    /// The same dialog with a release picked (for the screenshots).
    pub fn with_choice(mut self, release_id: &str) -> Self {
        self.chosen = Some(release_id.to_owned());
        self
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        artwork: &mut ArtworkCache,
        actions: &mut Vec<Action>,
    ) -> bool {
        let state = session.artwork_search().map(|s| s.state.clone());
        let mut use_now = false;
        let result = window(ctx, "Find Album Artwork", WIDTH, "Use Cover", |ui| {
            if self.search_row(ui) {
                self.chosen = None;
                actions.push(self.search_action());
            }
            ui.add_space(10.0);
            let rect = ui.available_rect_before_wrap();
            ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(1.0, theme::GRID_LINE));
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(ROW * 5.0).auto_shrink([false, true]).show(ui, |ui| match &state {
                None => status(ui, "Type the album's title, then choose Search.", false),
                Some(SearchState::Searching) => status(ui, "Searching MusicBrainz…", true),
                Some(SearchState::Failed(message)) => status(ui, message, false),
                Some(SearchState::Found(found)) if found.is_empty() => status(
                    ui,
                    "MusicBrainz knows no release by that name. Check the spelling, or try the title alone.",
                    false,
                ),
                Some(SearchState::Found(found)) => {
                    for candidate in found {
                        let picked = self.chosen.as_deref() == Some(candidate.release.id.as_str());
                        let response = candidate_row(ui, candidate, picked, artwork);
                        if response.clicked() {
                            self.chosen = Some(candidate.release.id.clone());
                        }
                        if response.double_clicked() {
                            use_now = true;
                        }
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(
                muted(
                    "Releases from MusicBrainz (it is sent the artist and album), covers from the Cover Art Archive.",
                )
                .size(11.0),
            );
        });
        let chosen = match (result, use_now) {
            (Some(true), _) | (_, true) => self.chosen.clone(),
            (Some(false), _) => None,
            (None, false) => return true,
        };
        if let Some(release) = chosen {
            actions.push(Action::UseFoundArtwork(release));
        } else if result != Some(false) && !use_now {
            // "Use Cover" with nothing picked: stay open.
            return true;
        }
        actions.push(Action::CloseArtworkSearch);
        if let Some(previous) = self.return_to.take() {
            actions.push(Action::OpenDialog(previous));
        }
        false
    }

    /// Artist and album fields with a Search button. Returns whether to search again.
    fn search_row(&mut self, ui: &mut Ui) -> bool {
        let mut search = false;
        ui.horizontal(|ui| {
            ui.label(muted("Artist"));
            let artist = ui.add(egui::TextEdit::singleline(&mut self.artist).desired_width(170.0).hint_text("Any"));
            ui.label(muted("Album"));
            let album = ui.add(egui::TextEdit::singleline(&mut self.album).desired_width(190.0));
            let entered = (artist.lost_focus() || album.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter));
            search = ui.button("Search").clicked() || entered;
        });
        search && !self.album.trim().is_empty()
    }
}

fn status(ui: &mut Ui, text: &str, busy: bool) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        if busy {
            ui.spinner();
        }
        ui.label(muted(text));
    });
    ui.add_space(24.0);
}

/// One release: its thumbnail, title and artist, and the details that tell it apart.
fn candidate_row(ui: &mut Ui, candidate: &CoverCandidate, picked: bool, artwork: &mut ArtworkCache) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    let painter = ui.painter();
    if picked {
        painter.rect_filled(rect, 6.0, theme::ACCENT.gamma_multiply(0.14));
        painter.rect_stroke(rect, 6.0, Stroke::new(1.5, theme::ACCENT), StrokeKind::Inside);
    } else if response.hovered() {
        painter.rect_filled(rect, 6.0, theme::ROW_ALT);
    }
    let thumb = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(THUMB, THUMB));
    let release = &candidate.release;
    let note = match &candidate.thumbnail {
        Thumbnail::Found { data, from } => {
            let texture = artwork.picture(ui.ctx(), &format!("thumb:{}", release.id), data);
            artwork::paint(ui, thumb, texture.as_ref());
            (*from == CoverOf::ReleaseGroup).then_some("No cover of its own: shows the album's, from another edition")
        }
        Thumbnail::NotFetched | Thumbnail::Fetching => {
            artwork::paint_placeholder(ui.painter(), thumb);
            None
        }
        Thumbnail::NoneFound => {
            artwork::paint_placeholder(ui.painter(), thumb);
            Some("No cover in the Cover Art Archive")
        }
        Thumbnail::Failed(_) => {
            artwork::paint_placeholder(ui.painter(), thumb);
            Some("The cover could not be shown")
        }
    };
    let text = Rect::from_min_max(thumb.right_top() + vec2(12.0, 2.0), rect.max - vec2(8.0, 4.0));
    let mut child =
        ui.new_child(egui::UiBuilder::new().max_rect(text).layout(egui::Layout::top_down(egui::Align::Min)));
    child.spacing_mut().item_spacing.y = 2.0;
    let title = if release.artist.is_empty() {
        release.title.clone()
    } else {
        format!("{} — {}", release.title, release.artist)
    };
    child.add(egui::Label::new(RichText::new(title).font(crate::fonts::bold(13.0)).color(theme::TEXT)).truncate());
    child.add(egui::Label::new(RichText::new(release.details()).size(11.5).color(theme::TEXT)).truncate());
    if let Some(note) = note {
        child.add(egui::Label::new(muted(note).size(11.0)).truncate());
    }
    response
}
