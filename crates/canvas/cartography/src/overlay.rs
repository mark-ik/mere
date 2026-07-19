// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Overlay vocabulary — semantic emphases canvases apply over geometry.
//!
//! Overlays are *additive* — a strategy can emit any subset (or none).
//! Canvases that don't recognize a variant ignore it; cartography
//! owners can grow the vocabulary additively without breaking
//! downstream consumers.

use kernel::graph::{EdgeKey, NodeKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Overlay {
    /// A halo around a cluster of nodes — community visualization.
    ClusterHalo {
        cluster_id: String,
        members: Vec<NodeKey>,
        label: Option<String>,
        confidence: f32,
    },
    /// Per-node activity / freshness heat (0..=1).
    ActivityHeat { node: NodeKey, intensity: f32 },
    /// Visual emphasis on a bridge node (extra ring, label, color
    /// shift) — for community-spanning hubs.
    BridgeEmphasis { node: NodeKey, weight: f32 },
    /// Importance hint: scale this node's screen-area by `factor`.
    /// Canvases that respect this multiply their default radius.
    ImportanceScale { node: NodeKey, factor: f32 },
    /// Per-edge weight hint (0..=1) — heavier edges render thicker.
    EdgeWeight { edge: EdgeKey, weight: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_variants_are_debuggable() {
        // Smoke test: every variant should at least Debug-print
        // without panicking. Catches accidental impl regressions.
        let overlays = vec![
            Overlay::ClusterHalo {
                cluster_id: "c1".into(),
                members: Vec::new(),
                label: Some("Work".into()),
                confidence: 0.85,
            },
            Overlay::ActivityHeat {
                node: NodeKey::new(0),
                intensity: 0.5,
            },
            Overlay::BridgeEmphasis {
                node: NodeKey::new(0),
                weight: 0.9,
            },
            Overlay::ImportanceScale {
                node: NodeKey::new(0),
                factor: 1.5,
            },
            Overlay::EdgeWeight {
                edge: EdgeKey::new(0),
                weight: 0.7,
            },
        ];
        for overlay in &overlays {
            let _ = format!("{overlay:?}");
        }
        assert_eq!(overlays.len(), 5);
    }
}
