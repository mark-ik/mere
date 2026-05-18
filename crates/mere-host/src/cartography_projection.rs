// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Cartography orchestration for the xilem host — composes
//! [`crate::graph_registry::GraphRegistry`] +
//! [`cartography::LayoutStrategy`] + the orrery renderer's snapshot
//! map without making `MereHostApp` aware of any of it.
//!
//! `mere-host-substrate` stays narrow (substrate ↔ runtime bridge);
//! cartography integration lives in this crate as the place where
//! "the modular pieces become a running host."

use cartography::{IntelligenceSignals, LayoutStrategy, ProjectionRequest, ViewIntent};
use mere_frame::{FrameLayout, PaneContent, PaneId};
use mere_host_substrate::{MereHostApp, walk_leaves};
use mere_renderer_registry::NodeIdentity;

use crate::graph_registry::GraphRegistry;
use crate::orrery_renderer::OrrerySnapshots;

/// Per-call report from [`project_orreries`] — counts of what got
/// projected vs. skipped, for diagnostics and tests.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionReport {
    /// Orrery leaves that produced a fresh projection this call.
    pub projected: usize,
    /// Orrery leaves whose `graph_id` had no entry in the registry.
    pub missing_graph: usize,
    /// Orrery leaves with no substrate identity (the frame layout
    /// wasn't synced to the substrate scene, or the pane isn't
    /// currently mapped).
    pub missing_identity: usize,
    /// Non-orrery leaves walked through without action.
    pub skipped_non_orrery: usize,
}

/// Project every `PaneContent::Orrery` leaf in `layout` through
/// `strategy`, writing each pane's projection into `orrery_snapshots`
/// keyed by its substrate `NodeIdentity`.
///
/// Per-pane viewport sizing comes from `walk_leaves`'s split-computed
/// bounds; the cartography `ViewIntent.target_size` is set to
/// `Pixels { pane_width, pane_height }` so positions land in pane-
/// local coordinates the orrery renderer paints directly.
///
/// `pane_identity_for` resolves a `PaneId` to its substrate identity —
/// typically `|id| host_app.identity_for_pane(id)`. Decoupled as a
/// closure so callers aren't forced to thread a specific MereHostApp
/// reference. Stale entries (orreries no longer in the layout) are
/// not swept here — call [`clear_orrery_snapshots`] before a fresh
/// pass when the frametree's orrery membership may have changed.
pub fn project_orreries<F>(
    layout: &FrameLayout,
    viewport_size: kurbo::Size,
    pane_identity_for: F,
    graph_registry: &GraphRegistry,
    strategy: &dyn LayoutStrategy,
    orrery_snapshots: &OrrerySnapshots,
) -> ProjectionReport
where
    F: Fn(PaneId) -> Option<NodeIdentity>,
{
    let mut report = ProjectionReport::default();
    let leaves = walk_leaves(layout, viewport_size);
    let signals = IntelligenceSignals::default();
    let mut writer = orrery_snapshots
        .write()
        .expect("orrery snapshots lock poisoned");
    for leaf in &leaves {
        if !matches!(leaf.content, PaneContent::Orrery) {
            report.skipped_non_orrery += 1;
            continue;
        }
        let Some(identity) = pane_identity_for(leaf.pane_id) else {
            report.missing_identity += 1;
            continue;
        };
        let Some(graph) = graph_registry.get(leaf.graph_id) else {
            report.missing_graph += 1;
            continue;
        };
        let intent = ViewIntent::canvas_pixels(
            leaf.size.width.max(1.0) as u32,
            leaf.size.height.max(1.0) as u32,
        );
        let request = ProjectionRequest {
            graph,
            signals: &signals,
            intent,
        };
        let projection = strategy.project(&request);
        writer.insert(identity, projection);
        report.projected += 1;
    }
    report
}

/// Drop every snapshot entry in `orrery_snapshots`. Call before a
/// fresh [`project_orreries`] pass when the frametree's orrery
/// membership may have changed (panes closed / reparented).
pub fn clear_orrery_snapshots(orrery_snapshots: &OrrerySnapshots) {
    orrery_snapshots
        .write()
        .expect("orrery snapshots lock poisoned")
        .clear();
}

/// Convenience: pull `host_app.identity_for_pane` into a closure
/// suitable for [`project_orreries`]. Cuts the call-site boilerplate
/// when the caller has a `&MereHostApp` handy.
pub fn pane_identity_closure(host_app: &MereHostApp) -> impl Fn(PaneId) -> Option<NodeIdentity> + '_ {
    move |id| host_app.identity_for_pane(id)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use cartography::{LayoutStrategy, Projection, ProjectionMetadata, ProjectionRequest};
    use kurbo::Size;
    use mere_frame::{FrameId, GraphId, PaneId, PaneNode, SplitAxis};
    use mere_kernel::graph::Graph;
    use mere_renderer_registry::NodeIdentity;

    use super::*;
    use crate::graph_registry::GraphRegistry;
    use crate::orrery_renderer::OrreryRenderer;

    /// Stub strategy that returns an empty projection with a known
    /// strategy id so tests can assert which strategy ran without
    /// asserting on positions.
    struct StubStrategy;
    impl LayoutStrategy for StubStrategy {
        fn projection_id(&self) -> &'static str {
            "stub.test"
        }
        fn project(&self, _request: &ProjectionRequest<'_>) -> Projection {
            Projection {
                metadata: ProjectionMetadata {
                    strategy_id: Some("stub.test".to_string()),
                    settled: true,
                },
                ..Projection::empty()
            }
        }
    }

    fn workbench_plus_orrery_layout() -> (FrameLayout, GraphId, PaneId) {
        let graph_id = GraphId::new();
        let orrery_pane = PaneId(2);
        let layout = FrameLayout {
            id: FrameId::new("test"),
            label: "test".to_string(),
            root: PaneNode::Split {
                axis: SplitAxis::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(1),
                    content: PaneContent::Workbench,
                    graph_id,
                }),
                second: Box::new(PaneNode::Leaf {
                    pane_id: orrery_pane,
                    content: PaneContent::Orrery,
                    graph_id,
                }),
            },
        };
        (layout, graph_id, orrery_pane)
    }

    #[test]
    fn skips_non_orrery_panes() {
        let (layout, _, _) = workbench_plus_orrery_layout();
        let graphs = GraphRegistry::new();
        let snapshots = OrreryRenderer::default().snapshots();
        let report = project_orreries(
            &layout,
            Size::new(800.0, 600.0),
            |_| None,
            &graphs,
            &StubStrategy,
            &snapshots,
        );
        assert_eq!(report.skipped_non_orrery, 1);
        assert_eq!(report.projected, 0);
    }

    #[test]
    fn reports_missing_graph_when_registry_lacks_id() {
        let (layout, _graph_id, orrery_pane) = workbench_plus_orrery_layout();
        let graphs = GraphRegistry::new();
        let identity = NodeIdentity::next();
        let mut pane_to_identity = HashMap::new();
        pane_to_identity.insert(orrery_pane, identity);
        let snapshots = OrreryRenderer::default().snapshots();
        let report = project_orreries(
            &layout,
            Size::new(800.0, 600.0),
            |p| pane_to_identity.get(&p).copied(),
            &graphs,
            &StubStrategy,
            &snapshots,
        );
        assert_eq!(report.missing_graph, 1);
        assert_eq!(report.projected, 0);
    }

    #[test]
    fn projects_orrery_when_graph_and_identity_resolve() {
        let (layout, graph_id, orrery_pane) = workbench_plus_orrery_layout();
        let mut graphs = GraphRegistry::new();
        graphs.insert(graph_id, Graph::new());
        let identity = NodeIdentity::next();
        let mut pane_to_identity = HashMap::new();
        pane_to_identity.insert(orrery_pane, identity);
        let snapshots = OrreryRenderer::default().snapshots();

        let report = project_orreries(
            &layout,
            Size::new(800.0, 600.0),
            |p| pane_to_identity.get(&p).copied(),
            &graphs,
            &StubStrategy,
            &snapshots,
        );

        assert_eq!(report.projected, 1);
        let map = snapshots.read().unwrap();
        assert!(map.contains_key(&identity));
        assert_eq!(
            map.get(&identity).unwrap().metadata.strategy_id.as_deref(),
            Some("stub.test")
        );
    }

    #[test]
    fn clear_drops_every_snapshot() {
        let snapshots = OrreryRenderer::default().snapshots();
        snapshots
            .write()
            .unwrap()
            .insert(NodeIdentity::next(), Projection::empty());
        assert_eq!(snapshots.read().unwrap().len(), 1);
        clear_orrery_snapshots(&snapshots);
        assert!(snapshots.read().unwrap().is_empty());
    }
}
