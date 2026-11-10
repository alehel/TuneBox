//! The source list: Library, Devices and Playlists, and the artwork pane below them.

use crate::actions::Action;
use crate::artwork;
use crate::dialogs::Dialog;
use crate::glyphs::{self, Glyph};
use crate::panels::PanelContext;
use crate::state::SourceItem;
use crate::theme;
use egui::{Color32, Rect, Sense, Ui, pos2, vec2};
use tunebox_engine::commands::CommandId;
use tunebox_engine::model::TrackId;

const ITEM_HEIGHT: f32 = 26.0;

/// Something a source-list row can do besides being selected.
enum RowExtra {
    None,
    Eject,
}

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, theme::SIDEBAR_BG);
    ui.painter().vline(rect.right() - 0.5, rect.y_range(), egui::Stroke::new(1.0, theme::BORDER));

    let artwork_height = if cx.view.show_artwork { (rect.width() + 12.0).min(250.0) } else { 0.0 };
    let list_rect = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - artwork_height));
    ui.scope_builder(egui::UiBuilder::new().max_rect(list_rect), |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(6.0);
            sources(ui, cx);
            ui.add_space(12.0);
        });
    });
    if cx.view.show_artwork {
        let pane = Rect::from_min_max(
            pos2(rect.left(), rect.bottom() - artwork_height),
            pos2(rect.right() - 1.0, rect.bottom()),
        );
        artwork_pane(ui, pane, cx);
    }
}

fn sources(ui: &mut Ui, cx: &mut PanelContext) {
    heading(ui, "LIBRARY");
    item(ui, cx, SourceItem::Music, (Glyph::Note, theme::ICON_LIBRARY), "Music", RowExtra::None);

    let devices: Vec<(SourceItem, String)> =
        cx.session.devices().iter().map(|d| (SourceItem::Device(d.volume.root.clone()), d.name().to_owned())).collect();
    let disc = cx.session.cd().map(|cd| cd.title().to_owned());
    if !devices.is_empty() || disc.is_some() {
        heading(ui, "DEVICES");
        for (source, name) in devices {
            item(ui, cx, source, (Glyph::Player, theme::ICON_DEVICE), &name, RowExtra::Eject);
        }
        if let Some(title) = disc {
            item(ui, cx, SourceItem::Cd, (Glyph::Disc, theme::ICON_DISC), &title, RowExtra::Eject);
        }
    }

    heading(ui, "PLAYLISTS");
    let playlists: Vec<_> =
        cx.session.library().playlists().iter().map(|p| (p.id, p.name.clone(), p.is_smart())).collect();
    // Smart playlists first, as the classic source list groups them.
    for smart_first in [true, false] {
        for (id, name, smart) in playlists.iter().filter(|p| p.2 == smart_first) {
            let glyph = if *smart {
                (Glyph::SmartPlaylist, theme::ICON_SMART)
            } else {
                (Glyph::Playlist, theme::ICON_PLAYLIST)
            };
            let response = item(ui, cx, SourceItem::Playlist(*id), glyph, name, RowExtra::None);
            playlist_menu(&response, cx, *id, *smart, name);
            if !smart && let Some(tracks) = response.dnd_release_payload::<Vec<TrackId>>() {
                cx.act(Action::AddToPlaylist(*id, (*tracks).clone()));
            }
        }
    }
}

fn heading(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    theme::spaced_caps(ui.painter(), pos2(rect.left() + 14.0, rect.bottom() - 6.0), text, theme::SIDEBAR_HEADING);
}

/// One source: an icon in its own colour and a name, on a rounded, inset pill when selected.
fn item(
    ui: &mut Ui,
    cx: &mut PanelContext,
    source: SourceItem,
    (glyph, icon_color): (Glyph, Color32),
    label: &str,
    extra: RowExtra,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ITEM_HEIGHT), Sense::click());
    let pill = rect.shrink2(vec2(7.0, 1.5));
    let selected = cx.view.source == source;
    let dropping = response.dnd_hover_payload::<Vec<TrackId>>().is_some();
    let painter = ui.painter();
    if selected {
        theme::selection_background(painter, pill);
    } else if dropping {
        painter.rect_filled(pill, 6.0, theme::ACCENT.gamma_multiply(0.15));
        painter.rect_stroke(pill, 6.0, egui::Stroke::new(1.5, theme::ACCENT), egui::StrokeKind::Inside);
    } else if response.hovered() {
        painter.rect_filled(pill, 6.0, theme::SIDEBAR_HOVER);
    }
    let (text_color, icon_color) = if selected { (Color32::WHITE, Color32::WHITE) } else { (theme::TEXT, icon_color) };
    let icon = Rect::from_center_size(pos2(pill.left() + 17.0, rect.center().y), vec2(15.0, 15.0));
    glyphs::paint(painter, icon, glyph, icon_color);
    let text_width = pill.width() - 56.0;
    theme::text_in_row(painter, pos2(pill.left() + 34.0, rect.center().y), label, theme::BODY, text_color, text_width);

    if let RowExtra::Eject = extra {
        let eject = Rect::from_center_size(pos2(pill.right() - 14.0, rect.center().y), vec2(11.0, 11.0));
        let eject_response = ui.interact(eject.expand(4.0), ui.scope_id().with(("eject", label)), Sense::click());
        let color = match (selected, eject_response.hovered()) {
            (true, _) => Color32::WHITE,
            (false, true) => theme::GLYPH_PRESSED,
            (false, false) => theme::TEXT_MUTED,
        };
        glyphs::paint(ui.painter(), eject, Glyph::Eject, color);
        if eject_response.on_hover_text("Eject").clicked() {
            cx.act(Action::SelectSource(source.clone()));
            cx.act(Action::Run(CommandId::Eject));
            return response;
        }
    }
    if response.clicked() || response.secondary_clicked() {
        cx.act(Action::SelectSource(source));
    }
    response
}

fn playlist_menu(
    response: &egui::Response,
    cx: &mut PanelContext,
    id: tunebox_engine::model::PlaylistId,
    smart: bool,
    name: &str,
) {
    response.context_menu(|ui| {
        if smart && ui.button("Edit Smart Playlist…").clicked() {
            cx.act(Action::EditSmartPlaylist(id));
            ui.close();
        }
        if ui.button("Rename…").clicked() {
            cx.act(Action::OpenDialog(Box::new(Dialog::rename_playlist(id, name))));
            ui.close();
        }
        if ui.button("Delete").clicked() {
            cx.act(Action::DeletePlaylist(id));
            ui.close();
        }
    });
}

/// The artwork of the selected track, or of what is playing when nothing is selected: a
/// rounded picture with a soft shadow, captioned above.
fn artwork_pane(ui: &mut Ui, rect: Rect, cx: &mut PanelContext) {
    let painter = ui.painter();
    painter.hline(rect.shrink2(vec2(12.0, 0.0)).x_range(), rect.top() + 0.5, egui::Stroke::new(1.0, theme::BORDER));
    let selected = cx.view.selected_tracks().first().copied();
    let (caption, track) = match (selected, cx.session.now_playing()) {
        (Some(id), _) => ("Selected Item", Some(id)),
        (None, Some(now)) => ("Now Playing", Some(now.track)),
        (None, None) => ("Nothing Selected", None),
    };
    theme::spaced_caps(painter, pos2(rect.left() + 14.0, rect.top() + 22.0), caption, theme::SIDEBAR_HEADING);
    let side = (rect.width() - 28.0).min(rect.height() - 40.0).max(0.0);
    let square = Rect::from_min_size(pos2(rect.center().x - side / 2.0, rect.top() + 30.0), vec2(side, side));
    theme::soft_shadow(painter, square, 6.0, 10);
    match track.and_then(|id| cx.session.track(id)).and_then(|t| cx.artwork.get(t)) {
        Some(texture) => {
            egui::Image::from_texture(egui::load::SizedTexture::from_handle(&texture))
                .corner_radius(6)
                .paint_at(ui, square);
        }
        None => artwork::paint_placeholder(ui.painter(), square),
    }
}
