/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable intent vocabulary primitives — the pure-data subset of
//! the intent enum/struct family that's free of `crate::app::*`,
//! `crate::shell::desktop::workbench::*`, and `crate::services::*`
//! coupling.
//!
//! Slice 57 ("the wedge"). The full intent vocabulary in
//! `app/intents.rs` (1882 LOC, 1308 external usages) carries
//! workbench / services entanglement that requires multiple
//! pre-cleanups before promotion. This module ships the cleanly
//! portable primitives now so they can be referenced from registrar
//! crates and graph crates without going through the binary root.
//!
//! Promotion roadmap (subsequent slices, when their pre-cleanups
//! land):
//!
//! - Slice 57b: `AppCommand` (after workbench types portable)
//! - Slice 57c: `GraphMutation` + `GraphIntent` (after services
//!   types portable)
//! - Slice 57d: `RuntimeEvent` + `ViewAction` + `NavigatorProjectionIntent`
//!
//! Until those land, the in-tree `app/intents.rs` re-exports the
//! types here so existing call sites resolve unchanged.

use std::path::PathBuf;

use crate::graph::NodeKey;

/// Browser-shell command — back / forward / reload / stop / zoom / close.
/// Targets a single browser surface (focused viewer, chrome projection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserCommand {
    Back,
    Forward,
    Reload,
    StopLoad,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Close,
}

impl BrowserCommand {
    /// Stable lowercase tag used in diagnostic channel payloads.
    pub fn diagnostic_label(self) -> &'static str {
        match self {
            BrowserCommand::Back => "back",
            BrowserCommand::Forward => "forward",
            BrowserCommand::Reload => "reload",
            BrowserCommand::StopLoad => "stop_load",
            BrowserCommand::ZoomIn => "zoom_in",
            BrowserCommand::ZoomOut => "zoom_out",
            BrowserCommand::ZoomReset => "zoom_reset",
            BrowserCommand::Close => "close",
        }
    }
}

/// Where a [`BrowserCommand`] should land — the focused-input
/// surface or the chrome projection (with an optional fallback node
/// when no chrome is active).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserCommandTarget {
    FocusedInput,
    ChromeProjection { fallback_node: Option<NodeKey> },
}

/// Spec for a user-supplied stylesheet that the runtime should apply
/// across viewers. The path is the source-of-truth identity; the
/// `source` is the loaded contents at the time of the intent (so the
/// intent is self-contained even if the file changes underneath).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeUserStylesheetSpec {
    pub path: PathBuf,
    pub source: String,
}

/// Edge-mutation command vocabulary. The pair-shaped variants carry
/// explicit endpoints so the receiver doesn't need to inspect a
/// selection set; the bare variants act on the current selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeCommand {
    ConnectSelectedPair,
    ConnectPair { from: NodeKey, to: NodeKey },
    ConnectBothDirections,
    ConnectBothDirectionsPair { a: NodeKey, b: NodeKey },
    RemoveUserEdge,
    RemoveUserEdgePair { a: NodeKey, b: NodeKey },
    PinSelected,
    UnpinSelected,
}
