/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! App-wide actions, default keymap, and user-keymap JSON loader.
//!
//! Layered as: built-in defaults (Rust-defined, always present) +
//! optional user overrides from `keymap.json` (loaded once at startup).
//! Conflicting user bindings win because gpui's `bind_keys` is
//! additive — later bindings take precedence for the same keystroke +
//! context.
//!
//! Omnibar editor actions (Left, Right, Backspace, Submit, …) live
//! in [`mere_host_graphshell::omnibar_input`] and are scoped to the
//! `Omnibar` key-context, so they don't shadow these app-global
//! shortcuts even when the omnibar has focus.

use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context as _, Result};
use gpui::{App, KeyBinding, KeyBindingContextPredicate, actions};
use serde::Deserialize;

actions!(
    mere,
    [
        /// Quit the application.
        Quit,
        /// Move keyboard focus to the omnibar and select its contents.
        FocusOmnibar,
        /// Open the action palette — searchable command surface for
        /// every registered action, with current keybindings shown.
        OpenPalette,
        /// Navigate back in the active surface's history. v0: no-op
        /// + trace; wires to inker once history lands.
        GoBack,
        /// Navigate forward in the active surface's history.
        GoForward,
        /// Reload the active surface.
        Reload,
        /// Cycle the shellbar's edge: Top → Right → Bottom → Left.
        /// Persists across sessions.
        CycleShellbarPosition,
        /// Cycle the workbench tile-strip's edge in the same order.
        CycleWorkbenchStripPosition,
        /// Open a new host window. The new window shares the graph
        /// (so edits in either window propagate) and starts with a
        /// fresh per-window state: tile list, history, frame
        /// layout, omnibar, palette.
        OpenNewWindow,
        /// Toggle a workbench panel in the current window — summon
        /// one if none exists, close the first one if any exist.
        ToggleWorkbench,
        /// Toggle a gloss panel in the current window.
        ToggleGloss,
        /// Toggle an apparatus panel in the current window.
        ToggleApparatus,
        /// Summon an orrery panel bound to a freshly-minted graph.
        /// Adds the new orrery to the right edge of the current
        /// layout; the new graph joins the registry.
        SummonOrreryForNewGraph,
        /// Toggle the graph-switcher dropdown — a list of every
        /// graph in the registry. Selecting one summons its orrery
        /// in the current window.
        ToggleGraphSwitcher,
        /// Tear out the active tile into a **new window with a
        /// minimized orrery in a new graph**. Donor unchanged; new
        /// graph contains a copy of the tile's node.
        TearOutTileToNewGraphMinimized,
        /// Tear out the active tile into a **new window with a
        /// visible orrery in a new graph**. Donor unchanged.
        TearOutTileToNewGraphVisible,
        /// Tear out the active tile as a **sticky-note window**:
        /// no orrery, workbench keyed to the donor graph. The
        /// tile moves (donor closes it). The sticky-note shares
        /// the donor's graph so edits propagate.
        TearOutTileAsStickyNote,
    ]
);

/// On-disk path for the user's keymap override. Same data-locality
/// rules as the saved frame layout — `%LOCALAPPDATA%\mere\mere-host\`
/// on Windows, `~/.config/mere/mere-host/` on Linux, etc.
pub fn user_keymap_path() -> Option<PathBuf> {
    dirs::config_local_dir().map(|d| d.join("mere").join("mere-host").join("keymap.json"))
}

/// Built-in keymap. Always applied; user keymap is layered on top.
///
/// Bindings with `context = None` fire regardless of which widget
/// has focus; context-scoped bindings (e.g. `Omnibar`) live in their
/// own modules and are bound separately.
pub fn default_bindings() -> Vec<KeyBinding> {
    vec![
        // App lifecycle. Cmd-Q on Mac, Ctrl-Q on Linux/Windows.
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("ctrl-q", Quit, None),
        // Focus omnibar — browser convention: Ctrl-L, Cmd-L, F6.
        KeyBinding::new("cmd-l", FocusOmnibar, None),
        KeyBinding::new("ctrl-l", FocusOmnibar, None),
        KeyBinding::new("f6", FocusOmnibar, None),
        // Action palette — editor convention: Cmd/Ctrl-Shift-P.
        KeyBinding::new("cmd-shift-p", OpenPalette, None),
        KeyBinding::new("ctrl-shift-p", OpenPalette, None),
        // New host window — Cmd/Ctrl-N, browser convention. Opens
        // a second window sharing the same graph.
        KeyBinding::new("cmd-n", OpenNewWindow, None),
        KeyBinding::new("ctrl-n", OpenNewWindow, None),
        // Navigation. Alt-Left/Right is the browser convention; F5
        // is the universal reload key.
        KeyBinding::new("alt-left", GoBack, None),
        KeyBinding::new("alt-right", GoForward, None),
        KeyBinding::new("cmd-r", Reload, None),
        KeyBinding::new("ctrl-r", Reload, None),
        KeyBinding::new("f5", Reload, None),
        // Tear-out gestures (Phase 2 v0: action-only; drag wiring
        // lands in Part 2). Three modes, three distinct chords —
        // shift varies the new-graph orrery prominence, alt drops
        // the orrery entirely (sticky-note).
        KeyBinding::new("cmd-shift-t", TearOutTileToNewGraphMinimized, None),
        KeyBinding::new("ctrl-shift-t", TearOutTileToNewGraphMinimized, None),
        KeyBinding::new("cmd-shift-alt-t", TearOutTileToNewGraphVisible, None),
        KeyBinding::new("ctrl-shift-alt-t", TearOutTileToNewGraphVisible, None),
        KeyBinding::new("cmd-alt-t", TearOutTileAsStickyNote, None),
        KeyBinding::new("ctrl-alt-t", TearOutTileAsStickyNote, None),
    ]
}

#[derive(Deserialize)]
struct KeymapFile {
    /// Optional preamble — currently ignored, reserved for future
    /// schema-version handshake.
    #[serde(default)]
    #[allow(dead_code)]
    version: Option<u32>,
    bindings: Vec<KeymapEntry>,
}

#[derive(Deserialize)]
struct KeymapEntry {
    /// Single keystroke (e.g. `"ctrl-shift-p"`) or a space-separated
    /// chord sequence (e.g. `"ctrl-k ctrl-s"`).
    keystroke: String,
    /// Fully-qualified action name, e.g. `"mere::FocusOmnibar"`.
    action: String,
    /// Optional context predicate matching gpui's grammar
    /// (e.g. `"Omnibar"`, `"!Omnibar"`, `"Workbench && !modal"`).
    #[serde(default)]
    context: Option<String>,
    /// Optional JSON args for actions that take parameters. Unit
    /// struct actions (the kind produced by `actions!`) accept `{}`.
    #[serde(default)]
    args: Option<serde_json::Value>,
}

/// Load the user's keymap from disk and convert to `KeyBinding`s.
///
/// Returns `Ok(vec![])` if the file does not exist — that's the
/// expected state for a fresh install and is not an error.
/// Per-entry parse failures are logged and skipped so a single bad
/// binding can't break the rest of the user's keymap.
pub fn load_user_keymap(cx: &App) -> Result<Vec<KeyBinding>> {
    let Some(path) = user_keymap_path() else {
        return Ok(vec![]);
    };
    if !path.exists() {
        return Ok(vec![]);
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("read user keymap {}", path.display()))?;
    let file: KeymapFile = serde_json::from_str(&text)
        .with_context(|| format!("parse user keymap {}", path.display()))?;

    let mapper = cx.keyboard_mapper().clone();
    let mut bindings = Vec::with_capacity(file.bindings.len());
    for (i, entry) in file.bindings.into_iter().enumerate() {
        match build_entry(cx, &entry, &mapper) {
            Ok(binding) => bindings.push(binding),
            Err(e) => {
                tracing::warn!(
                    index = i,
                    keystroke = %entry.keystroke,
                    action = %entry.action,
                    error = %e,
                    "skipping invalid keymap entry"
                );
            }
        }
    }
    tracing::debug!(
        path = %path.display(),
        count = bindings.len(),
        "loaded user keymap"
    );
    Ok(bindings)
}

fn build_entry(
    cx: &App,
    entry: &KeymapEntry,
    mapper: &Rc<dyn gpui::PlatformKeyboardMapper>,
) -> Result<KeyBinding> {
    let context_predicate = entry
        .context
        .as_deref()
        .map(|s| -> Result<Rc<KeyBindingContextPredicate>> {
            Ok(Rc::new(
                KeyBindingContextPredicate::parse(s)
                    .with_context(|| format!("parse context predicate `{s}`"))?,
            ))
        })
        .transpose()?;
    let action = cx
        .build_action(&entry.action, entry.args.clone())
        .with_context(|| format!("build action `{}`", entry.action))?;
    let binding = KeyBinding::load(
        &entry.keystroke,
        action,
        context_predicate,
        false,
        None,
        mapper.as_ref(),
    )
    .with_context(|| format!("load keybinding `{}`", entry.keystroke))?;
    Ok(binding)
}
