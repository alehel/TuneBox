//! Keyboard shortcuts, read from the command registry.
//!
//! On the audiobook page its own keys are looked for first, so Delete removes a track from the
//! book there; elsewhere they do nothing.

use egui::{Key, KeyboardShortcut, Modifiers};
use tunebox_engine::commands::{COMMANDS, CommandId, CommandSpec, Key as CommandKey, Scope, Shortcut};

/// The command whose shortcut was pressed this frame, if any. Shortcuts without ⌘/Ctrl (Space,
/// Delete, the audiobook page's letters) are ignored while a text field has focus, so typing
/// works.
pub fn pressed(ctx: &egui::Context, book_page: bool) -> Option<CommandId> {
    let typing = ctx.egui_wants_keyboard_input();
    for (spec, shortcut) in in_order(book_page) {
        if typing && !shortcut.command {
            continue;
        }
        let mut modifiers = Modifiers::NONE;
        if shortcut.command {
            modifiers |= Modifiers::COMMAND;
        }
        if shortcut.shift {
            modifiers |= Modifiers::SHIFT;
        }
        if shortcut.alt {
            modifiers |= Modifiers::ALT;
        }
        for key in keys(shortcut.key) {
            if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key))) {
                return Some(spec.id);
            }
        }
    }
    None
}

/// The shortcuts to look for: the audiobook page's first when it is shown, and within each
/// scope those with more of Shift and Option first. egui matches ⌘F when ⌘⇧F is pressed, and ←
/// when ⌥← is (it ignores an extra Shift or Option), so ⌘⇧F has to be looked for first or Find
/// would win.
fn in_order(book_page: bool) -> impl Iterator<Item = (&'static CommandSpec, Shortcut)> {
    let scopes: &[Scope] = if book_page { &[Scope::BookPage, Scope::Window] } else { &[Scope::Window] };
    scopes.iter().flat_map(|scope| {
        let mut in_scope: Vec<(&'static CommandSpec, Shortcut)> = COMMANDS
            .iter()
            .filter(|spec| spec.scope == *scope)
            .filter_map(|spec| spec.shortcut.map(|shortcut| (spec, shortcut)))
            .collect();
        in_scope.sort_by_key(|(_, s)| std::cmp::Reverse(u8::from(s.shift) + u8::from(s.alt)));
        in_scope
    })
}

/// The egui keys for a registry key. Delete answers to both Backspace and Delete.
fn keys(key: CommandKey) -> Vec<Key> {
    match key {
        CommandKey::Letter(c) => Key::from_name(&c.to_ascii_uppercase().to_string()).into_iter().collect(),
        CommandKey::Comma => vec![Key::Comma],
        CommandKey::Space => vec![Key::Space],
        CommandKey::Delete => vec![Key::Backspace, Key::Delete],
        CommandKey::Left => vec![Key::ArrowLeft],
        CommandKey::Right => vec![Key::ArrowRight],
        CommandKey::Up => vec![Key::ArrowUp],
        CommandKey::Down => vec![Key::ArrowDown],
        CommandKey::Return => vec![Key::Enter],
        CommandKey::Question => vec![Key::Questionmark],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registry_shortcut_maps_to_a_key() {
        for spec in COMMANDS {
            if let Some(shortcut) = spec.shortcut {
                assert!(!keys(shortcut.key).is_empty(), "{:?}", spec.id);
            }
        }
    }

    #[test]
    fn shift_shortcuts_are_tried_before_their_plain_twins() {
        let order: Vec<CommandId> = in_order(true).map(|(spec, _)| spec.id).collect();
        let position = |id| order.iter().position(|c| *c == id);
        assert!(position(CommandId::FullScreenVisualizer) < position(CommandId::Find));
        assert!(position(CommandId::NewSmartPlaylist) < position(CommandId::NewPlaylist));
        assert!(position(CommandId::AcceptAllSuggestions) < position(CommandId::AcceptSuggestion));
        assert!(position(CommandId::FoldAllChapters) < position(CommandId::FoldChapter));
        assert_eq!(order.len(), COMMANDS.iter().filter(|c| c.shortcut.is_some()).count());
    }

    #[test]
    fn the_book_pages_keys_come_first_there_and_nowhere_else() {
        let book: Vec<CommandId> = in_order(true).map(|(spec, _)| spec.id).collect();
        let position = |id| book.iter().position(|c| *c == id);
        assert!(position(CommandId::RemoveFromBook) < position(CommandId::Delete));
        let window: Vec<CommandId> = in_order(false).map(|(spec, _)| spec.id).collect();
        assert!(!window.contains(&CommandId::SplitChapter));
        assert!(window.contains(&CommandId::Delete));
    }
}
