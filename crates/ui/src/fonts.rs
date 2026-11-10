//! The interface typeface: Lato 2.015 (OFL-1.1, `assets/fonts/`), Regular for text and Bold for
//! titles. Lato covers Latin with its accents, Greek, Cyrillic and Vietnamese. Behind it, Noto
//! Sans Symbols 2 (OFL-1.1) draws the symbols Lato lacks, such as the macOS shortcut keys ⌘ ⇧ ⌫,
//! and egui's built-in fonts come last. Chinese, Japanese and Korean are not covered yet (a
//! system CJK font loaded at run time is the planned fallback).

use egui::{FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

const REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Lato-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../../assets/fonts/Lato-Bold.ttf");
const SYMBOLS: &[u8] = include_bytes!("../../../assets/fonts/NotoSansSymbols2-Regular.ttf");

/// The family name of the bold face.
const BOLD_FAMILY: &str = "bold";

/// Installs Lato in front of egui's own fonts.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("Lato".into(), Arc::new(FontData::from_static(REGULAR)));
    fonts.font_data.insert("Lato Bold".into(), Arc::new(FontData::from_static(BOLD)));
    fonts.font_data.insert("Noto Sans Symbols 2".into(), Arc::new(FontData::from_static(SYMBOLS)));
    let egui_fonts = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let fallbacks: Vec<String> = std::iter::once("Noto Sans Symbols 2".to_owned()).chain(egui_fonts).collect();
    let with = |first: &str| std::iter::once(first.to_owned()).chain(fallbacks.iter().cloned()).collect::<Vec<_>>();
    fonts.families.insert(FontFamily::Proportional, with("Lato"));
    fonts.families.insert(FontFamily::Name(BOLD_FAMILY.into()), with("Lato Bold"));
    fonts
}

/// Lato Bold at `size`, for titles.
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD_FAMILY.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_faces_lay_out_names_in_the_scripts_lato_covers() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for font in [FontId::proportional(13.0), bold(13.0)] {
                for text in ["Sigur Rós — Ágætis byrjun", "Мумий Тролль", "Βαγγέλης"] {
                    let galley = ui.painter().layout_no_wrap(text.into(), font.clone(), egui::Color32::BLACK);
                    assert!(galley.size().x > 10.0, "{text:?} in {font:?}");
                }
            }
        });
        output.textures_delta.clear();
    }

    /// Every character outside ASCII that the interface's source code puts in a string must
    /// exist in the font stack, or it shows as an empty box (as the multiplication-cross icon
    /// and the menu tick once did).
    #[test]
    fn every_symbol_the_interface_writes_has_a_glyph() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        let mut missing = Vec::new();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for file in rust_files(&src) {
            let text = std::fs::read_to_string(&file).unwrap();
            for c in string_literal_chars(&text).filter(|c| !c.is_ascii()) {
                for font in [FontId::proportional(13.0), bold(13.0)] {
                    if !ctx.fonts_mut(|f| f.has_glyph(&font, c)) {
                        missing.push(format!("{c:?} (U+{:04X}) in {} ({:?})", c as u32, file.display(), font.family));
                    }
                }
            }
        }
        // Menu shortcuts come from the engine, written the macOS way or the Windows/Linux way.
        for command in tunebox_engine::commands::COMMANDS {
            for mac in [true, false] {
                let label = command.shortcut.map(|s| s.label(mac)).unwrap_or_default();
                for c in label.chars().filter(|c| !c.is_ascii()) {
                    if !ctx.fonts_mut(|f| f.has_glyph(&FontId::proportional(13.0), c)) {
                        missing.push(format!("{c:?} (U+{:04X}) in the shortcut {label:?}", c as u32));
                    }
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "characters no font can draw:\n{}", missing.join("\n"));
    }

    fn rust_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(rust_files(&path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
        files
    }

    /// The characters inside `"…"` string literals (good enough for this code base: no raw
    /// strings hold interface text).
    fn string_literal_chars(text: &str) -> impl Iterator<Item = char> + '_ {
        let mut inside = false;
        let mut escaped = false;
        text.chars().filter(move |&c| {
            let was_inside = inside;
            match (inside, escaped, c) {
                (true, true, _) => escaped = false,
                (true, false, '\\') => escaped = true,
                (_, false, '"') => inside = !inside,
                _ => {}
            }
            was_inside && inside
        })
    }
}
