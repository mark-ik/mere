/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Minimap descriptor — how to render a thumbnail of a projection.
//!
//! Set on a [`crate::Projection`] when the strategy can produce a
//! meaningful thumbnail. Canvases consume this to draw the minimap
//! overlay alongside the main view.
//!
//! The minimap is produced by the **same** strategy, projected with
//! `FormFactor::Minimap` and a small `target_size` — there's no
//! separate "minimap strategy" concept. This descriptor just tells
//! the canvas where the minimap goes and which overlays to keep at
//! thumbnail scale.

use mere_kernel::geometry::PortableRect;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MinimapDescriptor {
    /// Logical-unit bounds of the minimap render rectangle within the
    /// host canvas.
    pub bounds: PortableRect,
    /// The viewport-window rectangle to overlay on the minimap
    /// (Sublime-style), in minimap-local coordinates. `None` for
    /// minimaps that don't represent a viewport (whole-graph
    /// thumbnails on tile previews, etc.).
    pub viewport_window: Option<PortableRect>,
    /// Which overlay kinds should remain visible at minimap scale.
    /// Cluster halos and importance scaling typically survive; edge
    /// weights and activity heat typically don't.
    pub overlays_retained: Vec<MinimapOverlayKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MinimapOverlayKind {
    ClusterHalo,
    ActivityHeat,
    BridgeEmphasis,
    ImportanceScale,
    EdgeWeight,
}
