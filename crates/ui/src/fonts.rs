//! The interface typeface: Lato 2.015 (OFL-1.1, `assets/fonts/`), Regular for text and Bold for
//! titles. Lato covers Latin with its accents, Greek, Cyrillic and Vietnamese; egui's built-in
//! fonts stand behind it for emoji and symbols. Chinese, Japanese and Korean are not covered by
//! either yet (a system CJK font loaded at run time is the planned fallback).

use egui::{FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

const REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Lato-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../../assets/fonts/Lato-Bold.ttf");

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
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
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
                for text in ["Sigur Rós — Ágætis byrjun", "Мумий Тролль", "Βαγγέλης", "♫ ✓"]
                {
                    let galley = ui.painter().layout_no_wrap(text.into(), font.clone(), egui::Color32::BLACK);
                    assert!(galley.size().x > 10.0, "{text:?} in {font:?}");
                }
            }
        });
        output.textures_delta.clear();
    }
}
