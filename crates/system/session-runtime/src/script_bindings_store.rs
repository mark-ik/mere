/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! DocumentScript origin bindings sidecar — `<session_dir>/script-bindings.json`.
//!
//! "Installed scripts": a list of `origin -> component` bindings (§11.4 follow-on
//! #2, auto-attach). On a fresh navigation whose origin matches a binding, the host
//! auto-attaches that wasm `document-core` component to the tile (through the same
//! `Constellation::attach_script` path the omnibar `>attach-script` verb uses). Kept
//! in its own sidecar rather than the session-wide [`settings_store`] because it is
//! a distinct, list-shaped concern (installed extensions), not a flat preference.
//!
//! Same I/O shape as the other sidecars: typed records + `save` / `load` / `exists`,
//! atomic write (tmp + rename), `Ok(None)` when absent so the host falls back to no
//! bindings. Native-only (filesystem), like the session/frame stores.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Filename for the script-bindings sidecar (sibling to `graph.json`).
pub const SCRIPT_BINDINGS_FILENAME: &str = "script-bindings.json";

/// One origin -> component binding. `origin` is matched against a navigated URL's
/// host: an exact host (`example.com`) or a `*.`-prefixed suffix glob
/// (`*.example.com`). `component_path` is the wasm `document-core` component to
/// attach. The capability grant is resolved by the host at attach time from the
/// session-scope script permissions (follow-on #1), not stored here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptBinding {
    pub origin: String,
    pub component_path: String,
}

impl ScriptBinding {
    pub fn new(origin: impl Into<String>, component_path: impl Into<String>) -> Self {
        Self { origin: origin.into(), component_path: component_path.into() }
    }
}

/// Build the path `<session_dir>/script-bindings.json`.
pub fn script_bindings_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SCRIPT_BINDINGS_FILENAME)
}

/// Serialise `bindings` to pretty JSON and write atomically (tmp + rename),
/// creating the session directory if needed.
pub fn save_script_bindings(session_dir: &Path, bindings: &[ScriptBinding]) -> io::Result<()> {
    let target = script_bindings_path(session_dir);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(bindings)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Read `<session_dir>/script-bindings.json`. `Ok(None)` when the file doesn't
/// exist (no bindings installed — the host auto-attaches nothing).
pub fn load_script_bindings(session_dir: &Path) -> io::Result<Option<Vec<ScriptBinding>>> {
    let path = script_bindings_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let bindings: Vec<ScriptBinding> =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(bindings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("mere-bindings-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_dir("round-trip");
        let bindings = vec![
            ScriptBinding::new("example.com", "mods/hi.wasm"),
            ScriptBinding::new("*.wikipedia.org", "mods/wiki.wasm"),
        ];
        save_script_bindings(&dir, &bindings).unwrap();
        let restored = load_script_bindings(&dir).unwrap().expect("file present");
        assert_eq!(restored, bindings);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_absent() {
        let dir = temp_dir("absent");
        assert!(load_script_bindings(&dir).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }
}
