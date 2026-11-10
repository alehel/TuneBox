//! Things the listener asked for while the window was drawn. Panels only read the session and
//! queue actions; the app applies them once drawing is done, so the session has one writer and
//! panels never fight over borrows.

use crate::dialogs::Dialog;
use crate::state::SourceItem;
use std::path::PathBuf;
use std::time::Duration;
use tunebox_engine::commands::CommandId;
use tunebox_engine::model::{Column, PlaylistId, Rating, SmartRules, TrackId, TrackMeta};
use tunebox_engine::{DiscInfo, JobId, Preferences, SyncSettings};

#[derive(Clone, Debug)]
pub enum Action {
    /// A menu item, shortcut or button bound to a registry command.
    Run(CommandId),
    SelectSource(SourceItem),
    PlayTracks {
        tracks: Vec<TrackId>,
        start: usize,
    },
    SetRating(Vec<TrackId>, Rating),
    SetChecked(Vec<TrackId>, bool),
    SortBy(Column),
    Seek(Duration),
    SetVolume(f32),
    AddToPlaylist(PlaylistId, Vec<TrackId>),
    NewPlaylistWith(Vec<TrackId>),
    RenamePlaylist(PlaylistId, String),
    DeletePlaylist(PlaylistId),
    EditSmartPlaylist(PlaylistId),
    RegisterDevice {
        root: PathBuf,
        name: String,
    },
    SetDeviceSettings {
        root: PathBuf,
        settings: SyncSettings,
    },
    SyncDevice(PathBuf),
    SetCdTrackIncluded(usize, bool),
    SetDiscInfo(Box<DiscInfo>),
    OpenDialog(Box<Dialog>),
    CancelJob(JobId),
    ShowInFolder(TrackId),
    EditTracks(Vec<(TrackId, TrackMeta)>),
    RemoveTracks {
        tracks: Vec<TrackId>,
        delete_files: bool,
    },
    SetPreferences(Box<Preferences>),
    SaveSmartPlaylist {
        id: Option<PlaylistId>,
        name: String,
        rules: SmartRules,
    },
}
