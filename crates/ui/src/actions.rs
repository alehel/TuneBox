//! Things the listener asked for while the window was drawn. Panels only read the session and
//! queue actions; the app applies them once drawing is done, so the session has one writer and
//! panels never fight over borrows.

use crate::dialogs::Dialog;
use crate::state::SourceItem;
use std::path::PathBuf;
use std::time::Duration;
use tunebox_engine::commands::CommandId;
use tunebox_engine::model::{Column, PlaylistId, Rating, SmartRules, TrackId, TrackMeta};
use tunebox_engine::{DiscInfo, JobId, LibraryOnlyEdits, Notice, Preferences, SyncSettings};

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
    /// Tell the listener something (a visualizer plug-in that was turned off).
    Notify(Notice),
    EditTracks(Vec<(TrackId, TrackMeta)>),
    RemoveTracks {
        tracks: Vec<TrackId>,
        delete_files: bool,
    },
    RefreshFromFiles(Vec<TrackId>, LibraryOnlyEdits),
    /// Ask for a picture and make it the cover of these songs.
    ChooseArtwork(Vec<TrackId>),
    /// Make this image file the cover of these songs (a picture dropped on the artwork).
    SetArtwork(Vec<TrackId>, PathBuf),
    RemoveArtwork(Vec<TrackId>),
    /// Rename files as the dialog's preview shows (the dialog comes back if that fails).
    RenameFiles(Box<crate::dialogs::RenameFilesDialog>),
    /// Ask where a missing song's file is now, and link the song to it.
    Locate(TrackId),
    SetPreferences(Box<Preferences>),
    /// Import these files and folders.
    AddToLibrary(Vec<PathBuf>),
    SaveSmartPlaylist {
        id: Option<PlaylistId>,
        name: String,
        rules: SmartRules,
    },
}
