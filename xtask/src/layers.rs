//! The layering rules, enforced by `cargo xtask layers`.
//!
//! A crate may depend only on crates in lower layers (or on the same-layer crates listed in
//! `SIDEWAYS`), and nothing below the frontends may depend on a UI toolkit. This keeps the
//! engine usable headlessly and every lower crate testable on its own.

use serde_json::Value;

/// Every workspace crate (without the `tunebox-` prefix) and its layer. `None`: exempt.
const TABLE: &[(&str, Option<u8>)] = &[
    ("model", Some(0)),
    ("library", Some(1)),
    ("tags", Some(1)),
    ("audio", Some(1)),
    ("mtp", Some(1)),
    ("import", Some(2)),
    ("playback", Some(2)),
    ("rip", Some(2)),
    ("sync", Some(2)),
    ("engine", Some(6)),
    ("platform", Some(7)),
    ("visualizer", Some(7)),
    ("ui", Some(7)),
    ("tunebox", None),
    ("cli", None),
    ("xtask", None),
];

/// Allowed same-layer edges.
const SIDEWAYS: &[(&str, &str)] = &[("rip", "import"), ("ui", "platform"), ("ui", "visualizer")];

/// External crates that make up a UI toolkit (a trailing `*` matches a prefix).
const UI_CRATES: &[&str] = &["egui*", "eframe", "winit", "wgpu*", "rfd"];
const UI_MIN_LAYER: u8 = 7;

fn short(name: &str) -> &str {
    name.strip_prefix("tunebox-").unwrap_or(name)
}

fn layer_of(name: &str) -> Result<Option<u8>, String> {
    TABLE
        .iter()
        .find(|(n, _)| *n == short(name))
        .map(|(_, l)| *l)
        .ok_or_else(|| format!("crate `{name}` has no layer: add it to TABLE in xtask/src/layers.rs"))
}

fn is_ui_crate(name: &str) -> bool {
    UI_CRATES.iter().any(|p| p.strip_suffix('*').map_or(name == *p, |prefix| name.starts_with(prefix)))
}

/// Problems with one dependency edge, if any.
fn check_edge(from: &str, to: &str, workspace: bool) -> Result<Option<String>, String> {
    let Some(from_layer) = layer_of(from)? else { return Ok(None) };
    if !workspace {
        return Ok((is_ui_crate(to) && from_layer < UI_MIN_LAYER)
            .then(|| format!("`{from}` (L{from_layer}) depends on UI crate `{to}`")));
    }
    let Some(to_layer) = layer_of(to)? else { return Ok(Some(format!("`{from}` depends on exempt crate `{to}`"))) };
    let sideways = SIDEWAYS.contains(&(short(from), short(to)));
    let ok = to_layer < from_layer || (to_layer == from_layer && sideways);
    Ok((!ok).then(|| format!("`{from}` (L{from_layer}) may not depend on `{to}` (L{to_layer})")))
}

pub fn check() -> Result<(), String> {
    let output = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|e| e.to_string())?;
    let metadata: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let packages = metadata["packages"].as_array().cloned().unwrap_or_default();
    let names: Vec<String> = packages.iter().filter_map(|p| p["name"].as_str().map(str::to_owned)).collect();
    let mut problems = Vec::new();
    for package in &packages {
        let Some(name) = package["name"].as_str() else { continue };
        for dep in package["dependencies"].as_array().into_iter().flatten() {
            if dep["kind"].as_str() == Some("dev") {
                continue;
            }
            let Some(to) = dep["name"].as_str() else { continue };
            if let Some(problem) = check_edge(name, to, names.iter().any(|n| n == to))? {
                problems.push(problem);
            }
        }
    }
    if problems.is_empty() {
        eprintln!("layers: {} crates OK", names.len());
        Ok(())
    } else {
        Err(format!("layering violations:\n  {}", problems.join("\n  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges() {
        assert_eq!(check_edge("tunebox-engine", "tunebox-library", true), Ok(None));
        assert!(check_edge("tunebox-library", "tunebox-engine", true).unwrap().is_some());
        assert_eq!(check_edge("tunebox-rip", "tunebox-import", true), Ok(None));
        assert!(check_edge("tunebox-import", "tunebox-rip", true).unwrap().is_some());
        assert!(check_edge("tunebox-engine", "egui", false).unwrap().is_some());
        assert_eq!(check_edge("tunebox-ui", "eframe", false), Ok(None));
        assert_eq!(check_edge("tunebox-library", "serde", false), Ok(None));
        assert!(check_edge("tunebox-unknown", "x", false).is_err());
    }
}
