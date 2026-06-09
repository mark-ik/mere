/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral accessibility descriptors keyed by [`SurfaceId`].
//!
//! Provides a portable contract every Graphshell host can honor for
//! AccessKit instrumentation. The vendored iced fork doesn't yet
//! expose AccessKit hooks through its widget tree; this module's job
//! is to lock the shape in regardless — the descriptor lookup is
//! addressable today (tests assert completeness), and a future host
//! integration (or a future iced version that gains AccessKit
//! support) renders descriptors into AccessKit nodes via this single
//! lookup.
//!
//! ## Design choices
//!
//! - **AccessKit roles directly.** No bespoke role enum — we adopt
//!   `accesskit::Role` so future hosts that wire AccessKit don't have
//!   to translate. AccessKit is the de-facto cross-platform a11y
//!   layer (used by egui, winit, GTK, AT-SPI, UI Automation, NSAccessibility).
//! - **Keyed by `SurfaceId`, not stored on widgets.** Widgets stay
//!   `From<T>`-conversion-shaped (no need to add `.label(...)`
//!   builders to every gs widget). The descriptor lookup is the
//!   single source of truth — change the role once, every host
//!   picks it up.
//! - **Static const-ish data**. `descriptor_for` is a pure function
//!   over `SurfaceId`. Hosts can call it from anywhere without
//!   threading a registry.
//!
//! ## Slice 24 status
//!
//! This module ships the lookup + tests. Iced does *not* render the
//! descriptors today (iced's vendored fork has no AccessKit hooks).
//! When that gap closes, the integration is one place: the iced
//! widget conversions emit AccessKit nodes built from
//! `descriptor_for(surface_id)`. Future hosts (egui, Stage-G/H) do
//! the same against their respective AccessKit bridges.

use accesskit::Role;
use serde::{Deserialize, Serialize};

/// Identifier for a chrome surface that emits UX events. Adding a new
/// surface (e.g., a future "Inspector" pane) is one new variant here
/// and one new emission site at the surface's open/dismiss seams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceId {
    Omnibar,
    CommandPalette,
    NodeFinder,
    ContextMenu,
    ConfirmDialog,
    /// Modal that prompts the user for a URL when an action like
    /// `NodeNew` fires from a non-omnibar surface (palette, context
    /// menu, programmatic).
    NodeCreate,
    /// Modal that prompts the user for a new label when the
    /// `FrameRename` action fires.
    FrameRename,
    StatusBar,
    TreeSpine,
    NavigatorHost,
    /// A tile pane (tile-tabs + body). The variant is the granular
    /// surface; per-pane discrimination (PaneId) is host-side state
    /// and not carried in the `UxEvent` payload today — extending
    /// `UxEvent::SurfaceOpened` with a target field is queued as a
    /// future enrichment when downstream observers need it.
    TilePane,
    /// A canvas pane. Same per-pane discrimination caveat as
    /// [`Self::TilePane`].
    CanvasPane,
    /// The canvas base layer (empty Frame fallback).
    BaseLayer,
    /// The graph roster pane: node manifest/list for the current graph.
    RosterPane,
    /// The inspector pane: selected object provenance, trust, parse diagnostics,
    /// document structure, local cache state, and lineage.
    InspectorPane,
    /// The steward pane: live async operations and actionable actor/job state.
    StewardPane,
    /// The gloss pane: outline, swatch, and graphlet navigation surface.
    GlossPane,
    /// The apparatus pane: system diagnostics, settings, probes, and a11y status.
    ApparatusPane,
    /// The comms pane: mail/cabal conversations.
    CommsPane,
    /// The tiled workbench pane.
    WorkbenchPane,
}

/// A surface's accessibility metadata. Carries the AccessKit role
/// plus the visible label, an optional longer description (used as
/// the AccessKit `description` field — read by screen readers when
/// the user dwells on the surface), and an optional keyboard-shortcut
/// hint surfaced both visually and as the AccessKit `keyboard_shortcuts`
/// field so AT users can discover the binding.
///
/// Any one host may consume more or fewer of these fields depending
/// on its AccessKit integration depth. The portable contract is:
/// every surface declares all four; hosts use what they can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibilityDescriptor {
    /// Maps to `accesskit::Node::set_role`. Serialized as the
    /// numeric discriminant via accesskit's `serde` feature.
    pub role: Role,
    /// Visible label (`accesskit::Node::set_label`). Required —
    /// every surface ships with a non-empty label.
    pub label: &'static str,
    /// Longer description for screen readers
    /// (`accesskit::Node::set_description`). Optional.
    pub description: Option<&'static str>,
    /// Keyboard shortcut hint (e.g. "Ctrl+Shift+P") —
    /// `accesskit::Node::set_keyboard_shortcuts`. Optional.
    pub keyboard_shortcut: Option<&'static str>,
}

impl AccessibilityDescriptor {
    /// Apply this descriptor to an `accesskit::Node` builder. Used
    /// by host integrations once their AccessKit bridge is wired.
    /// Today no host calls this; tests verify the shape compiles.
    pub fn apply(&self, node: &mut accesskit::Node) {
        node.set_role(self.role);
        node.set_label(self.label);
        if let Some(desc) = self.description {
            node.set_description(desc);
        }
        if let Some(ks) = self.keyboard_shortcut {
            node.set_keyboard_shortcut(ks);
        }
    }
}

/// The canonical descriptor for a surface. Pure function over
/// [`SurfaceId`]; the result is the single source of truth every
/// host renders identically.
pub fn descriptor_for(surface: SurfaceId) -> AccessibilityDescriptor {
    match surface {
        SurfaceId::Omnibar => AccessibilityDescriptor {
            role: Role::TextInput,
            label: "Address bar",
            description: Some(
                "URL entry and breadcrumb display. Type a URL and \
                 press Enter to open it as a node.",
            ),
            keyboard_shortcut: Some("Ctrl+L"),
        },
        SurfaceId::CommandPalette => AccessibilityDescriptor {
            role: Role::Dialog,
            label: "Command palette",
            description: Some(
                "Search and run any registered Graphshell action. \
                 Type to filter; arrow keys to navigate; Enter to run.",
            ),
            keyboard_shortcut: Some("Ctrl+Shift+P"),
        },
        SurfaceId::NodeFinder => AccessibilityDescriptor {
            role: Role::Dialog,
            label: "Node finder",
            description: Some(
                "Search graph nodes by title, tag, URL, or content. \
                 Type to filter; arrow keys to navigate; Enter to open.",
            ),
            keyboard_shortcut: Some("Ctrl+P"),
        },
        SurfaceId::ContextMenu => AccessibilityDescriptor {
            role: Role::Menu,
            label: "Context menu",
            description: Some("Actions available for the right-clicked target."),
            keyboard_shortcut: None,
        },
        SurfaceId::ConfirmDialog => AccessibilityDescriptor {
            role: Role::AlertDialog,
            label: "Confirm destructive action",
            description: Some(
                "Press Confirm to proceed with the destructive action, \
                 or Cancel / Escape to abandon it.",
            ),
            keyboard_shortcut: None,
        },
        SurfaceId::NodeCreate => AccessibilityDescriptor {
            role: Role::Dialog,
            label: "Create node",
            description: Some(
                "Enter a URL or address. Press Enter to create the \
                 node, or Cancel / Escape to abandon.",
            ),
            keyboard_shortcut: None,
        },
        SurfaceId::FrameRename => AccessibilityDescriptor {
            role: Role::Dialog,
            label: "Rename frame",
            description: Some(
                "Enter a new label for the active Frame. Press Enter \
                 to apply, or Cancel / Escape to abandon.",
            ),
            keyboard_shortcut: None,
        },
        SurfaceId::StatusBar => AccessibilityDescriptor {
            role: Role::ContentInfo,
            label: "Status bar",
            description: Some(
                "Ambient runtime indicators: ready state, dispatched \
                 actions, pending intent queue depth, focused node.",
            ),
            keyboard_shortcut: None,
        },
        SurfaceId::TreeSpine => AccessibilityDescriptor {
            role: Role::Tree,
            label: "Tree spine",
            description: Some("Structural list of nodes in the current workbench."),
            keyboard_shortcut: None,
        },
        SurfaceId::NavigatorHost => AccessibilityDescriptor {
            role: Role::Navigation,
            label: "Navigator host",
            description: Some(
                "Sidebar or toolbar host containing the Tree Spine, \
                 Swatches, and Activity Log buckets.",
            ),
            keyboard_shortcut: None,
        },
        SurfaceId::TilePane => AccessibilityDescriptor {
            role: Role::TabPanel,
            label: "Tile pane",
            description: Some(
                "Pane showing the active tiles of a graphlet. \
                 Right-click for tile actions.",
            ),
            keyboard_shortcut: None,
        },
        SurfaceId::CanvasPane => AccessibilityDescriptor {
            role: Role::Canvas,
            label: "Graph canvas pane",
            description: Some(
                "Force-directed canvas of graph nodes. Left-drag to \
                 pan, scroll to zoom, right-click on a node for \
                 actions.",
            ),
            keyboard_shortcut: None,
        },
        SurfaceId::BaseLayer => AccessibilityDescriptor {
            role: Role::Canvas,
            label: "Graph canvas",
            description: Some(
                "Default home view of the current graph. Right-click \
                 for graph-level actions.",
            ),
            keyboard_shortcut: None,
        },
        SurfaceId::RosterPane => AccessibilityDescriptor {
            role: Role::List,
            label: "Roster pane",
            description: Some("List of graph nodes with their current content state."),
            keyboard_shortcut: Some("Ctrl+R"),
        },
        SurfaceId::InspectorPane => AccessibilityDescriptor {
            role: Role::Complementary,
            label: "Inspector pane",
            description: Some(
                "Selected object details: provenance, trust, parse diagnostics, document structure, cache state, and lineage.",
            ),
            keyboard_shortcut: None,
        },
        SurfaceId::StewardPane => AccessibilityDescriptor {
            role: Role::Complementary,
            label: "Steward pane",
            description: Some(
                "Live async operations and actionable actor state: fetches, sync, retries, background work, and jobs.",
            ),
            keyboard_shortcut: None,
        },
        SurfaceId::GlossPane => AccessibilityDescriptor {
            role: Role::Navigation,
            label: "Gloss pane",
            description: Some("Navigator surface for outlines, swatches, and graphlets."),
            keyboard_shortcut: Some("Ctrl+G"),
        },
        SurfaceId::ApparatusPane => AccessibilityDescriptor {
            role: Role::Complementary,
            label: "Apparatus pane",
            description: Some("System diagnostics, probes, accessibility status, and settings."),
            keyboard_shortcut: Some("Ctrl+,"),
        },
        SurfaceId::CommsPane => AccessibilityDescriptor {
            role: Role::Complementary,
            label: "Comms pane",
            description: Some("Mail and cabal conversation surface."),
            keyboard_shortcut: None,
        },
        SurfaceId::WorkbenchPane => AccessibilityDescriptor {
            role: Role::TabPanel,
            label: "Workbench pane",
            description: Some("Tiled live node workbench."),
            keyboard_shortcut: Some("Ctrl+T"),
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant of `SurfaceId` must have a non-empty label.
    /// Adding a new surface variant without a descriptor breaks this
    /// test — that's the point. The lookup stays exhaustive.
    #[test]
    fn every_surface_has_a_non_empty_label() {
        for surface in [
            SurfaceId::Omnibar,
            SurfaceId::CommandPalette,
            SurfaceId::NodeFinder,
            SurfaceId::ContextMenu,
            SurfaceId::ConfirmDialog,
            SurfaceId::NodeCreate,
            SurfaceId::FrameRename,
            SurfaceId::StatusBar,
            SurfaceId::TreeSpine,
            SurfaceId::NavigatorHost,
            SurfaceId::TilePane,
            SurfaceId::CanvasPane,
            SurfaceId::BaseLayer,
            SurfaceId::RosterPane,
            SurfaceId::InspectorPane,
            SurfaceId::StewardPane,
            SurfaceId::GlossPane,
            SurfaceId::ApparatusPane,
            SurfaceId::CommsPane,
            SurfaceId::WorkbenchPane,
        ] {
            let d = descriptor_for(surface);
            assert!(
                !d.label.is_empty(),
                "{surface:?} ships with an empty a11y label",
            );
        }
    }

    /// Descriptors with shortcut hints must include the modifier
    /// (Ctrl / Cmd / Alt / Shift) so the AT can announce it
    /// canonically rather than reading "P" alone.
    #[test]
    fn keyboard_shortcuts_include_modifier_keys() {
        for surface in [
            SurfaceId::Omnibar,
            SurfaceId::CommandPalette,
            SurfaceId::NodeFinder,
        ] {
            let d = descriptor_for(surface);
            let ks = d
                .keyboard_shortcut
                .expect("this surface should ship with a shortcut hint");
            assert!(
                ks.contains("Ctrl") || ks.contains("Cmd") || ks.contains("Alt"),
                "{surface:?} shortcut hint missing modifier: {ks}",
            );
        }
    }

    #[test]
    fn apply_writes_role_and_label_to_accesskit_node() {
        let d = descriptor_for(SurfaceId::ConfirmDialog);
        let mut node = accesskit::Node::new(d.role);
        d.apply(&mut node);
        // `accesskit::Node` doesn't expose role/label getters
        // directly here; we rely on the apply() being a no-panic
        // invocation as a smoke test. The real verification is at
        // host-integration time when an AccessKit tree is built.
        let _ = node;
    }
}
