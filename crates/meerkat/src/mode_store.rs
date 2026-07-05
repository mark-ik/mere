/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Custom-mode persistence (theme-modes T5, declarative lane).
//!
//! A custom MODE is a declarative palette calculator
//! ([`CustomModeDef`](register_theme::mode_calc::CustomModeDef)) in
//! `<mere_root>/modes/<id>.json` — authored by hand today, tiny and
//! reviewable, the same mod-distribution shape as theme files. Loaded once at
//! boot into the presentation state; a malformed or incomplete file is
//! skipped + logged, never fatal. There is no save path yet (no in-app
//! editor) — hand-edit and restart.

use std::path::{Path, PathBuf};

use register_theme::mode_calc::{CustomModeDef, chrome_from_custom_mode};

/// `<mere_root>/modes/` — where custom-mode files live.
pub fn modes_dir(mere_root: &Path) -> PathBuf {
    mere_root.join("modes")
}

/// Load every mode file in the modes dir. A file whose calculator table is
/// incomplete (missing / unknown roles) is rejected here — completeness is
/// seed-independent, so a dry run against any seed set proves it — and the
/// shell keeps running on the modes that do parse.
pub fn load_custom_modes(mere_root: &Path) -> Vec<CustomModeDef> {
    let dir = modes_dir(mere_root);
    let mut out: Vec<CustomModeDef> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out; // no modes dir yet
    };
    let probe_seeds = register_theme::seed::builtin_defs().swap_remove(0).seeds;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let def = match std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<CustomModeDef>(&s).ok())
        {
            Some(def) => def,
            None => {
                tracing::warn!(path = %path.display(), "skipping unreadable mode file");
                continue;
            }
        };
        if let Err(err) = chrome_from_custom_mode(&def, &probe_seeds) {
            tracing::warn!(path = %path.display(), %err, "skipping incomplete mode file");
            continue;
        }
        if out.iter().any(|m| m.id == def.id) {
            tracing::warn!(path = %path.display(), id = %def.id, "skipping duplicate mode id");
            continue;
        }
        out.push(def);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_complete_modes_and_skips_broken_ones() {
        let root = std::env::temp_dir().join("mere-mode-store-test");
        let _ = std::fs::remove_dir_all(&root);
        let dir = modes_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();

        // A complete mode: every chrome role from the neutral seed.
        let roles: Vec<String> = register_theme::mode_calc::CHROME_ROLES
            .iter()
            .map(|r| format!(r#""{r}": {{ "seed": "neutral", "l": 0.2 }}"#))
            .collect();
        std::fs::write(
            dir.join("dusk.json"),
            format!(
                r#"{{ "id": "dusk", "name": "Dusk", "dark": true, "chrome": {{ {} }} }}"#,
                roles.join(", ")
            ),
        )
        .unwrap();
        // An incomplete one (one role) and a malformed one.
        std::fs::write(
            dir.join("partial.json"),
            r#"{ "id": "partial", "name": "Partial", "chrome": { "toolbar_bg": { "seed": "neutral" } } }"#,
        )
        .unwrap();
        std::fs::write(dir.join("broken.json"), "{ not json").unwrap();

        let modes = load_custom_modes(&root);
        assert_eq!(modes.len(), 1, "only the complete mode loads");
        assert_eq!(modes[0].id, "dusk");
        assert!(modes[0].dark);
        let _ = std::fs::remove_dir_all(&root);
    }
}
