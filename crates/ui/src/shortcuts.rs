//! Keyboard shortcuts, read from the command registry.

use egui::{Key, KeyboardShortcut, Modifiers};
use tunebox_engine::commands::{COMMANDS, CommandId, CommandSpec, Key as CommandKey, Shortcut};

/// The command whose shortcut was pressed this frame, if any. Shortcuts without a modifier
/// (Space, Delete) are ignored while a text field has focus, so typing works.
pub fn pressed(ctx: &egui::Context) -> Option<CommandId> {
    let typing = ctx.egui_wants_keyboard_input();
    for (spec, shortcut) in shift_first() {
        if typing && !shortcut.command {
            continue;
        }
        let modifiers = match (shortcut.command, shortcut.shift) {
            (true, true) => Modifiers::COMMAND | Modifiers::SHIFT,
            (true, false) => Modifiers::COMMAND,
            (false, true) => Modifiers::SHIFT,
            (false, false) => Modifiers::NONE,
        };
        for key in keys(shortcut.key) {
            if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key))) {
                return Some(spec.id);
            }
        }
    }
    None
}

/// The registry's shortcuts, those with Shift before those without. egui matches ⌘F when ⌘⇧F
/// is pressed (it ignores an extra Shift), so ⌘⇧F has to be looked for first or Find would win.
fn shift_first() -> impl Iterator<Item = (&'static CommandSpec, Shortcut)> {
    let with_shortcut = || COMMANDS.iter().filter_map(|spec| spec.shortcut.map(|shortcut| (spec, shortcut)));
    with_shortcut().filter(|(_, s)| s.shift).chain(with_shortcut().filter(|(_, s)| !s.shift))
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
        let order: Vec<CommandId> = shift_first().map(|(spec, _)| spec.id).collect();
        let position = |id| order.iter().position(|c| *c == id);
        assert!(position(CommandId::FullScreenVisualizer) < position(CommandId::Find));
        assert!(position(CommandId::NewSmartPlaylist) < position(CommandId::NewPlaylist));
        assert_eq!(order.len(), COMMANDS.iter().filter(|c| c.shortcut.is_some()).count());
    }
}
