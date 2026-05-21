// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Per-pane panel routing — which reactive panel a `Panel`-kind pane
//! shows, by its domain role.
//!
//! All `PaneContent` variants except Orrery/Tile map to
//! `NodeContentKind::Panel`, so a single `MasonryEmbeddedRenderer`
//! handles them all. To stop every panel showing the same content,
//! the host hands that renderer a factory that routes on the node's
//! identity → [`PaneRole`]. The host refreshes the identity→role map
//! each scene sync ([`update_pane_roles`]); the factory
//! ([`build_panel`]) reads it when the substrate first asks for a
//! producer.
//!
//! v0: Apparatus shows the live diagnostics panel; Workbench / Gloss /
//! System show distinct static placeholders (real content — tile strip,
//! content strip, inspector — lands in later phases). The point here is
//! that panes are routed by role, not that each panel is finished.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use frame::{FrameLayout, PaneContent};
use host_substrate::{HostApp, walk_leaves};
use kurbo::Size;
use masonry_core::core::DefaultProperties;
use mere_masonry::xilem_masonry::view::{flex_col, label};
use mere_masonry::xilem_masonry::{AnyWidgetView, WidgetView};
use mere_masonry::{PanelLogic, PanelTile, TileSize, XilemPanel};
use register_renderer::{NodeIdentity, SceneNodeRef};

use crate::diagnostics_panel::{DiagnosticsHandle, build_diagnostics_panel};

/// The domain role a `Panel`-kind pane plays — the routing key for
/// which panel content it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneRole {
    Workbench,
    Gloss,
    Apparatus,
    System,
    /// Any other `Panel` pane (Custom, or an unmapped role).
    Generic,
}

impl PaneRole {
    /// Role for a [`PaneContent`]. Orrery / Tile aren't `Panel`-kind —
    /// they route to the orrery / document renderers, not here — so
    /// they fall to `Generic` (never actually reached by the panel
    /// factory).
    pub fn for_content(content: &PaneContent) -> Self {
        match content {
            PaneContent::Workbench => Self::Workbench,
            PaneContent::Gloss => Self::Gloss,
            PaneContent::Apparatus => Self::Apparatus,
            PaneContent::System => Self::System,
            _ => Self::Generic,
        }
    }
}

/// Host-shared `NodeIdentity → PaneRole` map. Written by the host each
/// scene sync; read by the panel factory at producer-construction
/// time. Shared (Arc) because the factory outlives any single sync.
pub type PaneRoles = Arc<RwLock<HashMap<NodeIdentity, PaneRole>>>;

/// Refresh the identity→role map from the current frame layout. Call
/// after `sync_scene_from_frame_layout` (which assigns the pane
/// identities this reads back). `viewport` only scales leaf bounds,
/// which this ignores — pane id + content are viewport-independent.
pub fn update_pane_roles(
    host_app: &HostApp,
    layout: &FrameLayout,
    viewport: Size,
    roles: &PaneRoles,
) {
    let Ok(mut map) = roles.write() else {
        return;
    };
    map.clear();
    for leaf in walk_leaves(layout, viewport) {
        if let Some(identity) = host_app.identity_for_pane(leaf.pane_id) {
            map.insert(identity, PaneRole::for_content(&leaf.content));
        }
    }
}

/// Build the panel for `node`, routed by its pane role. Falls back to
/// the diagnostics panel when the role is unknown (e.g. the role map
/// hasn't caught up — shouldn't happen after a sync).
pub fn build_panel(
    node: &SceneNodeRef,
    roles: &PaneRoles,
    diagnostics: &DiagnosticsHandle,
    runtime: &Arc<tokio::runtime::Runtime>,
    default_properties: &Arc<DefaultProperties>,
    size: TileSize,
    scale_factor: f64,
) -> Box<dyn PanelTile> {
    let role = roles
        .read()
        .ok()
        .and_then(|m| m.get(&node.identity).copied())
        .unwrap_or(PaneRole::Generic);
    match role {
        PaneRole::Apparatus => build_diagnostics_panel(
            diagnostics.clone(),
            runtime.clone(),
            default_properties.clone(),
            size,
            scale_factor,
        ),
        PaneRole::Workbench => static_panel(
            "Workbench",
            "tile strip — engine content lands in a later phase",
            runtime,
            default_properties,
            size,
            scale_factor,
        ),
        PaneRole::Gloss => static_panel(
            "Gloss",
            "content strip",
            runtime,
            default_properties,
            size,
            scale_factor,
        ),
        PaneRole::System => static_panel(
            "System",
            "system inspector",
            runtime,
            default_properties,
            size,
            scale_factor,
        ),
        PaneRole::Generic => static_panel(
            "Panel",
            "—",
            runtime,
            default_properties,
            size,
            scale_factor,
        ),
    }
}

/// A non-reactive labelled panel: a title plus one description line.
/// `State = ()` — the view never changes, so `tick`'s rebuild is a
/// cheap no-op-shaped pass. Used for the placeholder domain panes
/// until their real content (tile strip, inspector, …) lands.
fn static_panel(
    title: &'static str,
    description: &'static str,
    runtime: &Arc<tokio::runtime::Runtime>,
    default_properties: &Arc<DefaultProperties>,
    size: TileSize,
    scale_factor: f64,
) -> Box<dyn PanelTile> {
    let logic: PanelLogic<()> = Box::new(move |_state: &mut ()| {
        flex_col((
            label(title).text_size(18.0),
            label(description),
        ))
        .boxed()
    });
    Box::new(XilemPanel::new(
        (),
        logic,
        runtime.clone(),
        default_properties.clone(),
        size,
        scale_factor,
    ))
}
