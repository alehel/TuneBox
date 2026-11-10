//! A player's page: what it is, what to sync to it, how full it is.

use crate::actions::Action;
use crate::glyphs::{self, Glyph};
use crate::panels::PanelContext;
use crate::theme;
use egui::{Align2, Color32, FontId, Rect, RichText, Stroke, Ui, pos2, vec2};
use std::path::Path;
use tunebox_engine::model::{PlaylistId, format_bytes};
use tunebox_engine::{DeviceEntry, JobKind, SyncMode, SyncSettings};

pub fn show(ui: &mut Ui, cx: &mut PanelContext, root: &Path) {
    let Some(entry) = cx.session.device(root).cloned() else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("The player was disconnected.").color(theme::TEXT_MUTED).size(16.0))
        });
        return;
    };
    let rect = ui.max_rect();
    theme::vertical_gradient(
        ui.painter(),
        rect,
        Color32::from_rgb(0xF3, 0xF3, 0xF3),
        Color32::from_rgb(0xE2, 0xE2, 0xE2),
    );
    let content = Rect::from_center_size(
        pos2(rect.center().x, rect.top() + rect.height() / 2.0),
        vec2(rect.width().min(760.0) - 40.0, rect.height() - 30.0),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
        summary(ui, &entry);
        ui.add_space(14.0);
        match &entry.device {
            Some(device) => {
                sync_settings(ui, cx, root, &device.manifest.settings);
                ui.add_space(16.0);
                capacity(ui, cx, &entry);
            }
            None => set_up(ui, cx, root, entry.name()),
        }
    });
}

fn summary(ui: &mut Ui, entry: &DeviceEntry) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), egui::Sense::hover());
    let painter = ui.painter();
    glyphs::paint(
        painter,
        Rect::from_min_size(rect.min + vec2(10.0, 8.0), vec2(104.0, 104.0)),
        Glyph::Player,
        Color32::from_rgb(0x4A, 0x4F, 0x57),
    );
    let left = rect.left() + 140.0;
    painter.text(
        pos2(left, rect.top() + 22.0),
        Align2::LEFT_CENTER,
        entry.name(),
        crate::fonts::bold(22.0),
        theme::TEXT,
    );
    let lines = [
        ("Capacity:", format_bytes(entry.volume.total_bytes)),
        ("Available:", format_bytes(entry.volume.free_bytes)),
        ("Location:", tunebox_platform::display_path(&entry.volume.root)),
        (
            "Kind:",
            if entry.device.is_some() { "Music player, synced by TuneBox".into() } else { "Music player".into() },
        ),
    ];
    for (i, (label, value)) in lines.iter().enumerate() {
        let y = rect.top() + 52.0 + i as f32 * 17.0;
        painter.text(pos2(left + 70.0, y), Align2::RIGHT_CENTER, *label, FontId::proportional(12.0), theme::TEXT_MUTED);
        painter.text(pos2(left + 78.0, y), Align2::LEFT_CENTER, value, FontId::proportional(12.0), theme::TEXT);
    }
}

fn set_up(ui: &mut Ui, cx: &mut PanelContext, root: &Path, default_name: &str) {
    group(ui, "Set Up", |ui| {
        ui.label("TuneBox has not synced to this player before. Give it a name to start syncing music to it.");
        ui.add_space(6.0);
        let id = ui.scope_id().with(("device-name", root));
        let mut name: String = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| default_name.to_owned());
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0));
            if crate::panels::widgets::primary_button(
                ui,
                "Set Up for Syncing",
                !name.trim().is_empty(),
                vec2(160.0, 28.0),
            )
            .clicked()
            {
                cx.act(Action::RegisterDevice { root: root.to_path_buf(), name: name.trim().to_owned() });
            }
        });
        ui.data_mut(|d| d.insert_temp(id, name));
    });
}

fn sync_settings(ui: &mut Ui, cx: &mut PanelContext, root: &Path, current: &SyncSettings) {
    let mut settings = current.clone();
    group(ui, "Music", |ui| {
        let mut everything = matches!(settings.mode, SyncMode::Everything);
        ui.horizontal(|ui| {
            ui.radio_value(&mut everything, true, "Entire music library");
            ui.radio_value(&mut everything, false, "Selected playlists");
        });
        if everything {
            settings.mode = SyncMode::Everything;
        } else {
            let mut chosen: Vec<PlaylistId> = match &settings.mode {
                SyncMode::Playlists(ids) => ids.clone(),
                SyncMode::Everything => Vec::new(),
            };
            egui::Frame::new().fill(Color32::WHITE).stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(6).show(
                ui,
                |ui| {
                    ui.set_min_width(ui.available_width());
                    egui::ScrollArea::vertical().max_height(210.0).show(ui, |ui| {
                        for playlist in cx.session.library().playlists() {
                            let mut on = chosen.contains(&playlist.id);
                            let count = cx.session.source_tracks(&tunebox_engine::Source::Playlist(playlist.id)).len();
                            if ui.checkbox(&mut on, format!("{}  ({count})", playlist.name)).changed() {
                                if on {
                                    chosen.push(playlist.id);
                                } else {
                                    chosen.retain(|id| *id != playlist.id);
                                }
                            }
                        }
                    });
                },
            );
            settings.mode = SyncMode::Playlists(chosen);
        }
        ui.add_space(4.0);
        ui.checkbox(&mut settings.only_checked, "Sync only checked songs");
        ui.checkbox(&mut settings.write_playlists, "Copy playlists to the player (M3U8)");
    });
    if settings != *current {
        cx.act(Action::SetDeviceSettings { root: root.to_path_buf(), settings });
    }
}

/// The capacity bar: music TuneBox put there, everything else, and free space.
fn capacity(ui: &mut Ui, cx: &mut PanelContext, entry: &DeviceEntry) {
    let total = entry.volume.total_bytes.max(1);
    let audio = entry.device.as_ref().map_or(0, |d| d.music_bytes()).min(total);
    let used = total.saturating_sub(entry.volume.free_bytes);
    let other = used.saturating_sub(audio);
    let free = entry.volume.free_bytes.min(total);

    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), egui::Sense::hover());
    let bar = Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(rect.width() - 120.0, 22.0));
    let painter = ui.painter();
    painter.rect_filled(bar, 11.0, theme::CAPACITY_FREE);
    let mut x = bar.left();
    for (bytes, color) in [(audio, theme::CAPACITY_AUDIO), (other, theme::CAPACITY_OTHER)] {
        let width = bar.width() * bytes as f32 / total as f32;
        if width > 0.5 {
            let segment = Rect::from_min_size(pos2(x, bar.top()), vec2(width, bar.height()));
            theme::vertical_gradient(painter, segment.shrink2(vec2(0.0, 1.0)), color.gamma_multiply(1.15), color);
            x += width;
        }
    }
    painter.rect_stroke(bar, 11.0, Stroke::new(1.0, theme::BORDER_DARK), egui::StrokeKind::Inside);
    let legend = [
        ("Audio", audio, theme::CAPACITY_AUDIO),
        ("Other", other, theme::CAPACITY_OTHER),
        ("Free", free, theme::CAPACITY_FREE),
    ];
    for (i, (label, bytes, color)) in legend.into_iter().enumerate() {
        let at = pos2(bar.left() + 8.0 + i as f32 * 130.0, bar.bottom() + 16.0);
        painter.circle(at, 5.0, color, Stroke::new(1.0, theme::BORDER));
        painter.text(
            at + vec2(10.0, 0.0),
            Align2::LEFT_CENTER,
            format!("{label}  {}", format_bytes(bytes)),
            FontId::proportional(11.5),
            theme::TEXT,
        );
    }
    let button = Rect::from_min_size(pos2(bar.right() + 16.0, bar.top() - 2.0), vec2(104.0, 26.0));
    let busy = cx.session.is_busy(JobKind::Sync);
    let label = if busy { "Syncing…" } else { "Sync" };
    let response =
        ui.interact(button, ui.scope_id().with("sync"), if busy { egui::Sense::hover() } else { egui::Sense::click() });
    crate::panels::widgets::paint_primary(ui, button, label, !busy, &response);
    if response.clicked() {
        cx.act(Action::SyncDevice(entry.volume.root.clone()));
    }
}

fn group(ui: &mut Ui, title: &str, contents: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(title).strong().size(13.0));
    egui::Frame::new()
        .fill(Color32::from_rgb(0xF9, 0xF9, 0xF9))
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(6)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            contents(ui);
        });
}
