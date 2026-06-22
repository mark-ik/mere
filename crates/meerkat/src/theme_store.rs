/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! User / mod theme persistence.
//!
//! A user theme and a community "mod" theme are the same thing: a serialized
//! [`ThemeDef`] (seeds + name + mode) in `<mere_root>/themes/`. They are loaded
//! into the [`ThemeRegistry`](register_theme::theme::ThemeRegistry) at startup
//! and written back on edit. Because a Mere theme is a handful of seed colours,
//! a theme file is tiny + reviewable — the mod-distribution path.
//!
//! Format is JSON today (consistent with `settings.json`, no extra dep); the
//! `ThemeDef` serde is format-agnostic, so a TOML swap is just the
//! (de)serializer.

use std::path::{Path, PathBuf};

use register_theme::theme::ThemeDef;

/// `<mere_root>/themes/` — where user + mod theme files live.
pub fn themes_dir(mere_root: &Path) -> PathBuf {
    mere_root.join("themes")
}

/// Load every theme file in the themes dir. Unreadable / malformed files are
/// skipped (logged), never fatal — a bad mod theme can't strand the user.
pub fn load_user_themes(mere_root: &Path) -> Vec<ThemeDef> {
    let dir = themes_dir(mere_root);
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out; // no themes dir yet
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<ThemeDef>(&s).ok())
        {
            Some(def) => out.push(def),
            None => tracing::warn!(path = %path.display(), "skipping unreadable theme file"),
        }
    }
    out
}

/// Write a user theme to `<themes>/<id>.json` (pretty). Creates the dir.
pub fn save_user_theme(mere_root: &Path, def: &ThemeDef) -> std::io::Result<()> {
    let dir = themes_dir(mere_root);
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(def)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(dir.join(format!("{}.json", sanitize(&def.id))), json)
}

/// Delete a user theme's file. Absent file is success (idempotent).
pub fn delete_user_theme(mere_root: &Path, id: &str) -> std::io::Result<()> {
    let path = themes_dir(mere_root).join(format!("{}.json", sanitize(id)));
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// A filesystem-safe filename stem from a theme id (`user:abc` -> `user_abc`).
fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use register_theme::theme::ThemeSource;

    #[test]
    fn sanitize_makes_safe_stems() {
        assert_eq!(sanitize("user:abc-1"), "user_abc-1");
        assert_eq!(sanitize("theme:default"), "theme_default");
    }

    #[test]
    fn save_then_load_round_trips() {
        // A temp dir under the OS temp; deterministic name per process is fine
        // for a single-threaded test run (the suite isn't run concurrently).
        let root = std::env::temp_dir().join("mere-theme-store-test");
        let _ = std::fs::remove_dir_all(&root);
        let def = register_theme::seed::builtin_defs().swap_remove(0);
        let user = ThemeDef {
            id: "user:roundtrip".into(),
            name: "Round Trip".into(),
            source: ThemeSource::User,
            seeds: def.seeds,
            high_contrast: false,
            harmony: Default::default(),
        };
        save_user_theme(&root, &user).expect("save");
        let loaded = load_user_themes(&root);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "user:roundtrip");
        assert_eq!(loaded[0].name, "Round Trip");
        delete_user_theme(&root, "user:roundtrip").expect("delete");
        assert!(load_user_themes(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
