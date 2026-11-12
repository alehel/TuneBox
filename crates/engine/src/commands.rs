//! The command registry: every user-facing action, by id.
//!
//! Menus, keyboard shortcuts and the command line all read this table, so an action is named
//! the same and bound to the same key everywhere. The frontend decides *how* to carry each one
//! out; the registry says what it is called, where it lives and when it is available.
//!
//! Most shortcuts work anywhere in the window ([`Scope::Window`]). The audiobook page has keys of
//! its own ([`Scope::BookPage`]): single letters, so a long book is edited without the mouse.
//! They act only there, and there they come first: Delete removes a track from the book.

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
    GetCdCover,
    SyncDevice,
    SetUpDrive,
    Eject,
    ShowInFolder,
    // Edit
    SelectAll,
    Delete,
    GetInfo,
    ChooseArtwork,
    RemoveArtwork,
    FindAlbumArtwork,
    Preferences,
    Find,
    // View
    ShowBrowser,
    AsList,
    AsAlbums,
    Visualizer,
    FullScreenVisualizer,
    ReloadVisualizerPlugins,
    ShowVisualizerPluginsFolder,
    // Controls
    PlayPause,
    NextTrack,
    PreviousTrack,
    VolumeUp,
    VolumeDown,
    Shuffle,
    Repeat,
    // Audiobook
    ImportAsAudiobook,
    RipDisc,
    FinishRipping,
    RipAnotherDisc,
    BuildAudiobook,
    SplitChapter,
    JoinChapters,
    RenameChapter,
    RemoveFromBook,
    TrimTrack,
    AcceptSuggestion,
    AcceptAllSuggestions,
    PlayAcrossStart,
    PlayEnd,
    UndoBookEdit,
    RedoBookEdit,
    PreviousChapter,
    NextChapter,
    FoldChapter,
    UnfoldChapter,
    FoldAllChapters,
    UnfoldAllChapters,
    GroupPerDisc,
    GroupPerTrack,
    GroupFromNames,
    AudiobookShortcuts,
    DiscardAudiobook,
    // Tasks
    RenameFiles,
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
    /// The audiobook in progress; shown while its page is.
    Audiobook,
    /// Batch work on the selected songs.
    Tasks,
    Help,
}

impl Menu {
    pub const ALL: [Menu; 7] =
        [Menu::File, Menu::Edit, Menu::View, Menu::Controls, Menu::Audiobook, Menu::Tasks, Menu::Help];

    /// Whether the menu bar shows this menu now.
    pub fn is_shown(self, context: Context) -> bool {
        self != Menu::Audiobook || context.book_page
    }

    pub fn title(self) -> &'static str {
        match self {
            Menu::File => "File",
            Menu::Edit => "Edit",
            Menu::View => "View",
            Menu::Controls => "Controls",
            Menu::Audiobook => "Audiobook",
            Menu::Tasks => "Tasks",
            Menu::Help => "Help",
        }
    }
}

/// A key that triggers a command. `command` is ⌘ on macOS and Ctrl elsewhere; `alt` is ⌥ on
/// macOS and Alt elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

/// Where a shortcut works.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Anywhere in the window.
    Window,
    /// On the audiobook page, before the window's shortcuts.
    BookPage,
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
    Return,
    Question,
}

impl Shortcut {
    const fn cmd(key: Key) -> Self {
        Self { command: true, shift: false, alt: false, key }
    }

    const fn cmd_shift(key: Key) -> Self {
        Self { command: true, shift: true, alt: false, key }
    }

    const fn cmd_alt(key: Key) -> Self {
        Self { command: true, shift: false, alt: true, key }
    }

    const fn plain(key: Key) -> Self {
        Self { command: false, shift: false, alt: false, key }
    }

    const fn shift(key: Key) -> Self {
        Self { command: false, shift: true, alt: false, key }
    }

    const fn alt(key: Key) -> Self {
        Self { command: false, shift: false, alt: true, key }
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
            // Written out on the Mac too: no bundled font draws "↩".
            Key::Return => {
                if mac {
                    "Return".into()
                } else {
                    "Enter".into()
                }
            }
            Key::Question => "?".into(),
        };
        let mut parts: Vec<String> = Vec::new();
        if mac {
            let mut prefix = String::new();
            if self.alt {
                prefix.push('⌥');
            }
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
        if self.alt {
            parts.push("Alt".into());
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
    /// A disc is in the drive and its names came from MusicBrainz.
    LookedUpDisc,
    /// A managed player is selected.
    Device,
    /// A disc or a player is selected.
    Ejectable,
    /// Library folders are listed in Preferences.
    LibraryFolders,
    /// Exactly one track is selected, and its file is missing.
    MissingSelection,
    /// The audiobook page is shown.
    BookPage,
    /// The audiobook page is shown with tracks selected on it.
    BookSelection,
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
    pub scope: Scope,
    pub needs: Needs,
}

const fn spec(id: CommandId, label: &'static str, menu: Menu, shortcut: Option<Shortcut>, needs: Needs) -> CommandSpec {
    CommandSpec { id, label, menu, separator_before: false, shortcut, scope: Scope::Window, needs }
}

/// A command of the audiobook page: in the Audiobook menu, its key working on the page only.
const fn book(id: CommandId, label: &'static str, shortcut: Option<Shortcut>, needs: Needs) -> CommandSpec {
    CommandSpec { id, label, menu: Menu::Audiobook, separator_before: false, shortcut, scope: Scope::BookPage, needs }
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
    spec(CommandId::GetCdCover, "Get CD Cover", Menu::File, None, Needs::LookedUpDisc),
    spec(CommandId::ImportAsAudiobook, "Import as Audiobook…", Menu::File, None, Needs::Disc),
    after_separator(spec(CommandId::SyncDevice, "Sync Device", Menu::File, None, Needs::Device)),
    spec(CommandId::SetUpDrive, "Set Up Drive for Syncing…", Menu::File, None, Needs::Nothing),
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
    spec(CommandId::FindAlbumArtwork, "Find Album Artwork…", Menu::Edit, None, Needs::Selection),
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
    spec(
        CommandId::FullScreenVisualizer,
        "Full Screen Visualizer",
        Menu::View,
        Some(Shortcut::cmd_shift(Key::Letter('f'))),
        Needs::Nothing,
    ),
    spec(CommandId::ReloadVisualizerPlugins, "Reload Visualizer Plug-ins", Menu::View, None, Needs::Nothing),
    spec(CommandId::ShowVisualizerPluginsFolder, "Show Visualizer Plug-ins Folder", Menu::View, None, Needs::Nothing),
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
    book(CommandId::RipDisc, "Rip Disc", Some(Shortcut::cmd(Key::Letter('r'))), Needs::BookPage),
    book(CommandId::FinishRipping, "That Was the Last Disc", Some(Shortcut::cmd(Key::Letter('d'))), Needs::BookPage),
    book(CommandId::RipAnotherDisc, "Rip Another Disc", None, Needs::BookPage),
    book(CommandId::BuildAudiobook, "Build Audiobook…", Some(Shortcut::cmd_shift(Key::Letter('b'))), Needs::BookPage),
    after_separator(book(
        CommandId::SplitChapter,
        "Split Chapter Here",
        Some(Shortcut::plain(Key::Letter('s'))),
        Needs::BookSelection,
    )),
    book(
        CommandId::JoinChapters,
        "Join into One Chapter",
        Some(Shortcut::plain(Key::Letter('j'))),
        Needs::BookSelection,
    ),
    book(CommandId::RenameChapter, "Rename Chapter", Some(Shortcut::plain(Key::Return)), Needs::BookSelection),
    after_separator(book(
        CommandId::RemoveFromBook,
        "Remove from Book or Restore",
        Some(Shortcut::plain(Key::Delete)),
        Needs::BookSelection,
    )),
    book(CommandId::TrimTrack, "Trim…", Some(Shortcut::plain(Key::Letter('t'))), Needs::BookSelection),
    book(
        CommandId::AcceptSuggestion,
        "Accept Suggestion",
        Some(Shortcut::plain(Key::Letter('a'))),
        Needs::BookSelection,
    ),
    book(
        CommandId::AcceptAllSuggestions,
        "Accept All Suggestions",
        Some(Shortcut::shift(Key::Letter('a'))),
        Needs::BookPage,
    ),
    after_separator(book(
        CommandId::PlayAcrossStart,
        "Play Across Start",
        Some(Shortcut::plain(Key::Letter('p'))),
        Needs::BookSelection,
    )),
    book(CommandId::PlayEnd, "Play End", Some(Shortcut::plain(Key::Letter('e'))), Needs::BookSelection),
    after_separator(book(CommandId::UndoBookEdit, "Undo", Some(Shortcut::cmd(Key::Letter('z'))), Needs::BookPage)),
    book(CommandId::RedoBookEdit, "Redo", Some(Shortcut::cmd_shift(Key::Letter('z'))), Needs::BookPage),
    after_separator(book(
        CommandId::PreviousChapter,
        "Previous Chapter",
        Some(Shortcut::alt(Key::Up)),
        Needs::BookPage,
    )),
    book(CommandId::NextChapter, "Next Chapter", Some(Shortcut::alt(Key::Down)), Needs::BookPage),
    book(CommandId::FoldChapter, "Fold Chapter", Some(Shortcut::plain(Key::Left)), Needs::BookSelection),
    book(CommandId::UnfoldChapter, "Unfold Chapter", Some(Shortcut::plain(Key::Right)), Needs::BookSelection),
    book(CommandId::FoldAllChapters, "Fold All Chapters", Some(Shortcut::alt(Key::Left)), Needs::BookPage),
    book(CommandId::UnfoldAllChapters, "Unfold All Chapters", Some(Shortcut::alt(Key::Right)), Needs::BookPage),
    after_separator(book(
        CommandId::GroupPerDisc,
        "One Chapter per Disc",
        Some(Shortcut::cmd_alt(Key::Letter('1'))),
        Needs::BookPage,
    )),
    book(CommandId::GroupPerTrack, "One Chapter per Track", Some(Shortcut::cmd_alt(Key::Letter('2'))), Needs::BookPage),
    book(
        CommandId::GroupFromNames,
        "Chapters from Track Names",
        Some(Shortcut::cmd_alt(Key::Letter('3'))),
        Needs::BookPage,
    ),
    after_separator(book(
        CommandId::AudiobookShortcuts,
        "Keyboard Shortcuts",
        Some(Shortcut::plain(Key::Question)),
        Needs::BookPage,
    )),
    book(CommandId::DiscardAudiobook, "Discard Audiobook…", None, Needs::BookPage),
    spec(CommandId::RenameFiles, "Rename Files from Tags…", Menu::Tasks, None, Needs::Selection),
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
    /// The audiobook page is shown.
    pub book_page: bool,
    /// Tracks selected on the audiobook page.
    pub book_selection: usize,
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
            Needs::LookedUpDisc => self.cd.as_ref().is_some_and(|cd| cd.info.release_id.is_some()),
            Needs::Device => context.device_selected,
            Needs::Ejectable => context.device_selected || context.cd_selected,
            Needs::LibraryFolders => !self.prefs.library_folders.is_empty(),
            Needs::MissingSelection => context.selected_tracks == 1 && context.selected_missing == 1,
            Needs::BookPage => context.book_page && self.audiobook.is_some(),
            Needs::BookSelection => context.book_page && self.audiobook.is_some() && context.book_selection > 0,
        };
        let busy = match id {
            CommandId::ImportCd => self.jobs.is_running(JobKind::Rip),
            CommandId::GetTrackNames => self.jobs.is_running(JobKind::LookupNames) || !cfg!(feature = "musicbrainz"),
            CommandId::GetCdCover => self.jobs.is_running(JobKind::FetchCover) || !cfg!(feature = "musicbrainz"),
            CommandId::FindAlbumArtwork => self.jobs.is_running(JobKind::FindArtwork) || !cfg!(feature = "musicbrainz"),
            CommandId::SyncDevice => self.jobs.is_running(JobKind::Sync),
            CommandId::ReloadVisualizerPlugins => self.jobs.is_running(JobKind::VisualizerPlugins),
            CommandId::RefreshFromFiles => {
                self.jobs.is_running(JobKind::Refresh) || self.jobs.is_running(JobKind::Rename)
            }
            CommandId::ChooseArtwork | CommandId::RemoveArtwork => {
                self.jobs.is_running(JobKind::Artwork) || self.jobs.is_running(JobKind::Rename)
            }
            CommandId::AddToLibrary | CommandId::ScanLibraryFolders => {
                self.jobs.is_running(JobKind::Import) || self.jobs.is_running(JobKind::Rename)
            }
            // One thing at a time touches where files are, or writes into them.
            CommandId::RenameFiles => [JobKind::Rename, JobKind::Import, JobKind::Refresh, JobKind::Artwork]
                .into_iter()
                .any(|kind| self.jobs.is_running(kind)),
            CommandId::ImportAsAudiobook => self.audiobook.is_some() || self.jobs.is_running(JobKind::Rip),
            CommandId::RipDisc => !self.can_rip_audiobook_disc(),
            CommandId::FinishRipping => {
                self.audiobook().is_none_or(|b| b.ripping_done) || self.jobs.is_running(JobKind::RipBookDisc)
            }
            CommandId::RipAnotherDisc => {
                self.audiobook().is_none_or(|b| !b.ripping_done) || self.jobs.is_running(JobKind::BuildBook)
            }
            CommandId::BuildAudiobook => {
                self.audiobook().is_none_or(|b| !b.ripping_done) || self.jobs.is_running(JobKind::BuildBook)
            }
            CommandId::AcceptAllSuggestions => self.audiobook_suggestions().is_empty(),
            CommandId::UndoBookEdit => !self.can_undo_audiobook(),
            CommandId::RedoBookEdit => !self.can_redo_audiobook(),
            CommandId::DiscardAudiobook => {
                self.jobs.is_running(JobKind::RipBookDisc) || self.jobs.is_running(JobKind::BuildBook)
            }
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
        for scope in [Scope::Window, Scope::BookPage] {
            let shortcuts: Vec<_> = COMMANDS.iter().filter(|c| c.scope == scope).filter_map(|c| c.shortcut).collect();
            let unique: HashSet<_> = shortcuts.iter().collect();
            assert_eq!(unique.len(), shortcuts.len(), "{scope:?}");
        }
    }

    #[test]
    fn the_book_page_shadows_only_delete() {
        let window: Vec<Shortcut> =
            COMMANDS.iter().filter(|c| c.scope == Scope::Window).filter_map(|c| c.shortcut).collect();
        for spec in COMMANDS.iter().filter(|c| c.scope == Scope::BookPage) {
            if let Some(shortcut) = spec.shortcut
                && window.contains(&shortcut)
            {
                assert_eq!(shortcut.key, Key::Delete, "{:?} hides a window shortcut", spec.id);
            }
            assert_eq!(spec.menu, Menu::Audiobook);
        }
    }

    #[test]
    fn option_and_return_have_labels() {
        assert_eq!(Shortcut::cmd_alt(Key::Letter('1')).label(true), "⌥⌘1");
        assert_eq!(Shortcut::cmd_alt(Key::Letter('1')).label(false), "Ctrl+Alt+1");
        assert_eq!(Shortcut::alt(Key::Up).label(true), "⌥↑");
        assert_eq!(Shortcut::plain(Key::Return).label(false), "Enter");
        assert_eq!(Shortcut::shift(Key::Letter('a')).label(true), "⇧A");
    }

    #[test]
    fn labels_per_platform() {
        let shortcut = Shortcut::cmd_shift(Key::Letter('n'));
        assert_eq!(shortcut.label(true), "⇧⌘N");
        assert_eq!(shortcut.label(false), "Ctrl+Shift+N");
        assert_eq!(Shortcut::plain(Key::Space).label(false), "Space");
    }
}
