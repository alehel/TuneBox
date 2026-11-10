//! The command registry: every user-facing action, by id.
//!
//! Menus, keyboard shortcuts and the command line all read this table, so an action is named
//! the same and bound to the same key everywhere. The frontend decides *how* to carry each one
//! out; the registry says what it is called, where it lives and when it is available.

use crate::jobs::JobKind;
use crate::session::Session;

/// Every action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandId {
    // File
    NewPlaylist,
    NewPlaylistFromSelection,
    NewSmartPlaylist,
    AddToLibrary,
    ScanLibraryFolders,
    RefreshFromFiles,
    Locate,
    ImportCd,
    GetTrackNames,
    SyncDevice,
    Eject,
    ShowInFolder,
    // Edit
    SelectAll,
    Delete,
    GetInfo,
    ChooseArtwork,
    RemoveArtwork,
    Preferences,
    Find,
    // View
    ShowBrowser,
    AsList,
    AsAlbums,
    Visualizer,
    // Controls
    PlayPause,
    NextTrack,
    PreviousTrack,
    VolumeUp,
    VolumeDown,
    Shuffle,
    Repeat,
    // App
    About,
    Quit,
}

/// The menus, in menu-bar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Menu {
    File,
    Edit,
    View,
    Controls,
    Help,
}

impl Menu {
    pub const ALL: [Menu; 5] = [Menu::File, Menu::Edit, Menu::View, Menu::Controls, Menu::Help];

    pub fn title(self) -> &'static str {
        match self {
            Menu::File => "File",
            Menu::Edit => "Edit",
            Menu::View => "View",
            Menu::Controls => "Controls",
            Menu::Help => "Help",
        }
    }
}

/// A key that triggers a command. `command` is ⌘ on macOS and Ctrl elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub command: bool,
    pub shift: bool,
    pub key: Key,
}

/// The keys shortcuts use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Letter(char),
    Comma,
    Space,
    Delete,
    Left,
    Right,
    Up,
    Down,
}

impl Shortcut {
    const fn cmd(key: Key) -> Self {
        Self { command: true, shift: false, key }
    }

    const fn cmd_shift(key: Key) -> Self {
        Self { command: true, shift: true, key }
    }

    const fn plain(key: Key) -> Self {
        Self { command: false, shift: false, key }
    }

    /// How a menu shows it: "⇧⌘N" on macOS, "Ctrl+Shift+N" elsewhere.
    pub fn label(&self, mac: bool) -> String {
        let key = match self.key {
            Key::Letter(c) => c.to_ascii_uppercase().to_string(),
            Key::Comma => ",".into(),
            Key::Space => "Space".into(),
            Key::Delete => {
                if mac {
                    "⌫".into()
                } else {
                    "Delete".into()
                }
            }
            Key::Left => "←".into(),
            Key::Right => "→".into(),
            Key::Up => "↑".into(),
            Key::Down => "↓".into(),
        };
        let mut parts: Vec<String> = Vec::new();
        if mac {
            let mut prefix = String::new();
            if self.shift {
                prefix.push('⇧');
            }
            if self.command {
                prefix.push('⌘');
            }
            return format!("{prefix}{key}");
        }
        if self.command {
            parts.push("Ctrl".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        parts.push(key);
        parts.join("+")
    }
}

/// When a command can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Needs {
    Nothing,
    /// Some tracks are selected.
    Selection,
    /// Exactly one track is selected.
    SingleSelection,
    /// A disc is in the drive.
    Disc,
    /// A managed player is selected.
    Device,
    /// A disc or a player is selected.
    Ejectable,
    /// Library folders are listed in Preferences.
    LibraryFolders,
    /// Exactly one track is selected, and its file is missing.
    MissingSelection,
}

/// One row of the registry.
#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    pub id: CommandId,
    pub label: &'static str,
    pub menu: Menu,
    /// Draws a separator above this item in its menu.
    pub separator_before: bool,
    pub shortcut: Option<Shortcut>,
    pub needs: Needs,
}

const fn spec(id: CommandId, label: &'static str, menu: Menu, shortcut: Option<Shortcut>, needs: Needs) -> CommandSpec {
    CommandSpec { id, label, menu, separator_before: false, shortcut, needs }
}

const fn after_separator(mut spec: CommandSpec) -> CommandSpec {
    spec.separator_before = true;
    spec
}

/// Every command, in menu order.
pub const COMMANDS: &[CommandSpec] = &[
    spec(CommandId::NewPlaylist, "New Playlist", Menu::File, Some(Shortcut::cmd(Key::Letter('n'))), Needs::Nothing),
    spec(CommandId::NewPlaylistFromSelection, "New Playlist from Selection", Menu::File, None, Needs::Selection),
    spec(
        CommandId::NewSmartPlaylist,
        "New Smart Playlist…",
        Menu::File,
        Some(Shortcut::cmd_shift(Key::Letter('n'))),
        Needs::Nothing,
    ),
    after_separator(spec(
        CommandId::AddToLibrary,
        "Add to Library…",
        Menu::File,
        Some(Shortcut::cmd(Key::Letter('o'))),
        Needs::Nothing,
    )),
    spec(CommandId::ScanLibraryFolders, "Scan Library Folders", Menu::File, None, Needs::LibraryFolders),
    spec(CommandId::RefreshFromFiles, "Refresh from Files", Menu::File, None, Needs::Selection),
    spec(CommandId::Locate, "Locate…", Menu::File, None, Needs::MissingSelection),
    spec(CommandId::ImportCd, "Import CD", Menu::File, None, Needs::Disc),
    spec(CommandId::GetTrackNames, "Get Track Names", Menu::File, None, Needs::Disc),
    after_separator(spec(CommandId::SyncDevice, "Sync Device", Menu::File, None, Needs::Device)),
    spec(CommandId::Eject, "Eject", Menu::File, Some(Shortcut::cmd(Key::Letter('e'))), Needs::Ejectable),
    after_separator(spec(
        CommandId::ShowInFolder,
        "Show in Folder",
        Menu::File,
        Some(Shortcut::cmd_shift(Key::Letter('r'))),
        Needs::SingleSelection,
    )),
    spec(CommandId::Quit, "Quit TuneBox", Menu::File, Some(Shortcut::cmd(Key::Letter('q'))), Needs::Nothing),
    spec(CommandId::SelectAll, "Select All", Menu::Edit, Some(Shortcut::cmd(Key::Letter('a'))), Needs::Nothing),
    spec(CommandId::Delete, "Delete", Menu::Edit, Some(Shortcut::plain(Key::Delete)), Needs::Selection),
    spec(CommandId::GetInfo, "Get Info", Menu::Edit, Some(Shortcut::cmd(Key::Letter('i'))), Needs::Selection),
    spec(CommandId::ChooseArtwork, "Choose Artwork…", Menu::Edit, None, Needs::Selection),
    spec(CommandId::RemoveArtwork, "Remove Artwork", Menu::Edit, None, Needs::Selection),
    spec(CommandId::Find, "Find", Menu::Edit, Some(Shortcut::cmd(Key::Letter('f'))), Needs::Nothing),
    after_separator(spec(
        CommandId::Preferences,
        "Preferences…",
        Menu::Edit,
        Some(Shortcut::cmd(Key::Comma)),
        Needs::Nothing,
    )),
    spec(CommandId::AsList, "as List", Menu::View, Some(Shortcut::cmd(Key::Letter('1'))), Needs::Nothing),
    spec(CommandId::AsAlbums, "as Album Grid", Menu::View, Some(Shortcut::cmd(Key::Letter('2'))), Needs::Nothing),
    after_separator(spec(
        CommandId::Visualizer,
        "Visualizer",
        Menu::View,
        Some(Shortcut::cmd(Key::Letter('t'))),
        Needs::Nothing,
    )),
    after_separator(spec(
        CommandId::ShowBrowser,
        "Column Browser",
        Menu::View,
        Some(Shortcut::cmd(Key::Letter('b'))),
        Needs::Nothing,
    )),
    spec(CommandId::PlayPause, "Play / Pause", Menu::Controls, Some(Shortcut::plain(Key::Space)), Needs::Nothing),
    spec(CommandId::NextTrack, "Next", Menu::Controls, Some(Shortcut::cmd(Key::Right)), Needs::Nothing),
    spec(CommandId::PreviousTrack, "Previous", Menu::Controls, Some(Shortcut::cmd(Key::Left)), Needs::Nothing),
    after_separator(spec(
        CommandId::VolumeUp,
        "Increase Volume",
        Menu::Controls,
        Some(Shortcut::cmd(Key::Up)),
        Needs::Nothing,
    )),
    spec(CommandId::VolumeDown, "Decrease Volume", Menu::Controls, Some(Shortcut::cmd(Key::Down)), Needs::Nothing),
    after_separator(spec(CommandId::Shuffle, "Shuffle", Menu::Controls, None, Needs::Nothing)),
    spec(CommandId::Repeat, "Repeat", Menu::Controls, None, Needs::Nothing),
    spec(CommandId::About, "About TuneBox", Menu::Help, None, Needs::Nothing),
];

/// The registry entry for `id`.
pub fn spec_of(id: CommandId) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// What the frontend knows about its own state, for deciding what is enabled.
#[derive(Clone, Copy, Debug, Default)]
pub struct Context {
    pub selected_tracks: usize,
    /// How many of the selected tracks have missing files.
    pub selected_missing: usize,
    pub device_selected: bool,
    pub cd_selected: bool,
}

impl Session {
    /// Whether `id` can run now.
    pub fn is_enabled(&self, id: CommandId, context: Context) -> bool {
        let Some(spec) = spec_of(id) else { return false };
        let ready = match spec.needs {
            Needs::Nothing => true,
            Needs::Selection => context.selected_tracks > 0,
            Needs::SingleSelection => context.selected_tracks == 1,
            Needs::Disc => self.cd.is_some(),
            Needs::Device => context.device_selected,
            Needs::Ejectable => context.device_selected || context.cd_selected,
            Needs::LibraryFolders => !self.prefs.library_folders.is_empty(),
            Needs::MissingSelection => context.selected_tracks == 1 && context.selected_missing == 1,
        };
        let busy = match id {
            CommandId::ImportCd => self.jobs.is_running(JobKind::Rip),
            CommandId::GetTrackNames => self.jobs.is_running(JobKind::LookupNames) || !cfg!(feature = "musicbrainz"),
            CommandId::SyncDevice => self.jobs.is_running(JobKind::Sync),
            CommandId::RefreshFromFiles => self.jobs.is_running(JobKind::Refresh),
            CommandId::ChooseArtwork | CommandId::RemoveArtwork => self.jobs.is_running(JobKind::Artwork),
            CommandId::AddToLibrary | CommandId::ScanLibraryFolders => self.jobs.is_running(JobKind::Import),
            _ => false,
        };
        ready && !busy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_and_shortcuts_are_unique() {
        let ids: HashSet<_> = COMMANDS.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), COMMANDS.len());
        let shortcuts: Vec<_> = COMMANDS.iter().filter_map(|c| c.shortcut).collect();
        let unique: HashSet<_> = shortcuts.iter().collect();
        assert_eq!(unique.len(), shortcuts.len());
    }

    #[test]
    fn labels_per_platform() {
        let shortcut = Shortcut::cmd_shift(Key::Letter('n'));
        assert_eq!(shortcut.label(true), "⇧⌘N");
        assert_eq!(shortcut.label(false), "Ctrl+Shift+N");
        assert_eq!(Shortcut::plain(Key::Space).label(false), "Space");
    }
}
