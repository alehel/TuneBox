//! Keyboard shortcuts, read from the command registry.

use egui::{Key, KeyboardShortcut, Modifiers};
use tunebox_engine::commands::{COMMANDS, CommandId, Key as CommandKey};

/// The command whose shortcut was pressed this frame, if any. Shortcuts without a modifier
/// (Space, Delete) are ignored while a text field has focus, so typing works.
pub fn pressed(ctx: &egui::Context) -> Option<CommandId> {
    let typing = ctx.egui_wants_keyboard_input();
    for spec in COMMANDS {
        let Some(shortcut) = spec.shortcut else { continue };
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
}
