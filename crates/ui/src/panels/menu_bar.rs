//! The menu bar, generated from the engine's command registry.

use crate::actions::Action;
use crate::panels::{PanelContext, widgets};
use crate::state::ViewMode;
use tunebox_engine::RepeatMode;
use tunebox_engine::commands::{COMMANDS, CommandId, Context, Menu};

pub fn show(ui: &mut egui::Ui, cx: &mut PanelContext, context: Context) {
    egui::MenuBar::new().ui(ui, |ui| {
        for menu in Menu::ALL {
            ui.menu_button(menu.title(), |ui| {
                ui.set_min_width(220.0);
                for spec in COMMANDS.iter().filter(|c| c.menu == menu) {
                    if spec.separator_before {
                        ui.separator();
                    }
                    let shortcut = spec.shortcut.map(|s| s.label(cfg!(target_os = "macos"))).unwrap_or_default();
                    let enabled = cx.session.is_enabled(spec.id, context);
                    if widgets::menu_item(ui, spec.label, &shortcut, is_on(spec.id, cx), enabled).clicked() {
                        cx.act(Action::Run(spec.id));
                        ui.close();
                    }
                }
            });
        }
    });
}

/// Toggle items show a check mark when on.
fn is_on(id: CommandId, cx: &PanelContext) -> bool {
    match id {
        CommandId::Shuffle => cx.session.shuffle(),
        CommandId::Repeat => cx.session.repeat() != RepeatMode::Off,
        CommandId::ShowBrowser => cx.session.preferences().show_browser,
        CommandId::Visualizer => cx.view.show_visualizer,
        CommandId::AsList => cx.view.view_mode == ViewMode::List,
        CommandId::AsAlbums => cx.view.view_mode == ViewMode::Albums,
        _ => false,
    }
}
