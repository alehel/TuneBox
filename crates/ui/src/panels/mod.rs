//! The parts of the main window. Each panel draws from the session and the view state and
//! queues [`Action`]s; none of them changes the session itself.

pub mod album_grid;
pub mod browser;
pub mod cd_view;
pub mod device_view;
pub mod lcd;
pub mod menu_bar;
pub mod sidebar;
pub mod status_bar;
pub mod toasts;
pub mod toolbar;
pub mod track_list;
pub mod widgets;

use crate::actions::Action;
use crate::artwork::ArtworkCache;
use crate::state::ViewState;
use tunebox_engine::Session;

/// What every panel gets.
pub struct PanelContext<'a> {
    pub session: &'a Session,
    pub view: &'a mut ViewState,
    pub artwork: &'a mut ArtworkCache,
    pub actions: &'a mut Vec<Action>,
}

impl PanelContext<'_> {
    pub fn act(&mut self, action: Action) {
        self.actions.push(action);
    }
}
