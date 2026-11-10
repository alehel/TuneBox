//! The source list: Library, Devices and Playlists, and the artwork pane below them.

use crate::actions::Action;
use crate::artwork;
use crate::dialogs::Dialog;
use crate::glyphs::{self, Glyph};
use crate::panels::PanelContext;
use crate::state::SourceItem;
use crate::theme;
use egui::{Color32, FontId, Rect, Sense, Ui, pos2, vec2};
use tunebox_engine::commands::CommandId;
use tunebox_engine::model::TrackId;

const ITEM_HEIGHT: f32 = 22.0;

/// Something a source-list row can do besides being selected.
enum RowExtra {
    None,
    Eject,
}

pub fn show(ui: &mut Ui, cx: &mut PanelContext) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, theme::SIDEBAR_BG);
    ui.painter().vline(rect.right() - 0.5, rect.y_range(), egui::Stroke::new(1.0, theme::BORDER));

    let artwork_height = if cx.view.show_artwork { rect.width().min(220.0) } else { 0.0 };
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
    item(ui, cx, SourceItem::Music, Glyph::Note, "Music", RowExtra::None);

    let devices: Vec<(SourceItem, String)> =
        cx.session.devices().iter().map(|d| (SourceItem::Device(d.volume.root.clone()), d.name().to_owned())).collect();
    let disc = cx.session.cd().map(|cd| cd.title().to_owned());
    if !devices.is_empty() || disc.is_some() {
        heading(ui, "DEVICES");
        for (source, name) in devices {
            item(ui, cx, source, Glyph::Player, &name, RowExtra::Eject);
        }
        if let Some(title) = disc {
            item(ui, cx, SourceItem::Cd, Glyph::Disc, &title, RowExtra::Eject);
        }
    }

    heading(ui, "PLAYLISTS");
    let playlists: Vec<_> =
        cx.session.library().playlists().iter().map(|p| (p.id, p.name.clone(), p.is_smart())).collect();
    // Smart playlists first, as the classic source list groups them.
    for smart_first in [true, false] {
        for (id, name, smart) in playlists.iter().filter(|p| p.2 == smart_first) {
            let glyph = if *smart { Glyph::SmartPlaylist } else { Glyph::Playlist };
            let response = item(ui, cx, SourceItem::Playlist(*id), glyph, name, RowExtra::None);
            playlist_menu(&response, cx, *id, *smart, name);
            if !smart && let Some(tracks) = response.dnd_release_payload::<Vec<TrackId>>() {
                cx.act(Action::AddToPlaylist(*id, (*tracks).clone()));
            }
        }
    }
}

fn heading(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 10.0, rect.bottom() - 6.0),
        egui::Align2::LEFT_BOTTOM,
        text,
        FontId::proportional(10.5),
        theme::SIDEBAR_HEADING,
    );
}

fn item(
    ui: &mut Ui,
    cx: &mut PanelContext,
    source: SourceItem,
    glyph: Glyph,
    label: &str,
    extra: RowExtra,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ITEM_HEIGHT), Sense::click());
    let selected = cx.view.source == source;
    let dropping = response.dnd_hover_payload::<Vec<TrackId>>().is_some();
    let painter = ui.painter();
    if selected {
        theme::selection_background(painter, rect);
    } else if dropping {
        painter.rect_stroke(rect.shrink(1.0), 4.0, egui::Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
    }
    let (text_color, icon_color) =
        if selected { (Color32::WHITE, Color32::WHITE) } else { (theme::TEXT, theme::SIDEBAR_ICON) };
    let icon = Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), vec2(14.0, 14.0));
    glyphs::paint(painter, icon, glyph, icon_color);
    let text_width = rect.width() - 64.0;
    theme::text_in_row(painter, pos2(rect.left() + 40.0, rect.center().y), label, theme::BODY, text_color, text_width);

    if let RowExtra::Eject = extra {
        let eject = Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(12.0, 12.0));
        let eject_response = ui.interact(eject.expand(3.0), ui.scope_id().with(("eject", label)), Sense::click());
        glyphs::paint(ui.painter(), eject, Glyph::Eject, icon_color);
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

/// The artwork of the selected track, or of what is playing when nothing is selected.
fn artwork_pane(ui: &mut Ui, rect: Rect, cx: &mut PanelContext) {
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, Color32::from_rgb(0x2A, 0x2A, 0x2A));
    let selected = cx.view.selected_tracks().first().copied();
    let (caption, track) = match (selected, cx.session.now_playing()) {
        (Some(id), _) => ("Selected Item", Some(id)),
        (None, Some(now)) => ("Now Playing", Some(now.track)),
        (None, None) => ("Nothing Selected", None),
    };
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 18.0));
    theme::vertical_gradient(painter, header, Color32::from_rgb(0xE6, 0xE6, 0xE6), Color32::from_rgb(0xC4, 0xC4, 0xC4));
    painter.text(header.center(), egui::Align2::CENTER_CENTER, caption, FontId::proportional(10.5), theme::TEXT);
    let image = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
    let square = Rect::from_center_size(
        image.center(),
        vec2(image.height().min(image.width()), image.height().min(image.width())),
    );
    match track.and_then(|id| cx.session.track(id)) {
        Some(track) => {
            let texture = cx.artwork.get(track);
            artwork::paint(ui.painter(), square, texture.as_ref());
        }
        None => artwork::paint_placeholder(ui.painter(), square),
    }
}
