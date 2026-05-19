// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Cartography orchestration for the xilem host — composes
//! [`crate::graph_registry::GraphRegistry`] +
//! [`cartography::LayoutStrategy`] + the orrery renderer's snapshot
//! map without making `HostApp` aware of any of it.
//!
//! `host-substrate` stays narrow (substrate ↔ runtime bridge);
//! cartography integration lives in this crate as the place where
//! "the modular pieces become a running host."

use cartography::{IntelligenceSignals, Projection, ProjectionRequest};
use frame::{FrameLayout, PaneContent, PaneId};
use host_substrate::{HostApp, walk_leaves};
use register_renderer::NodeIdentity;

use crate::graph_registry::GraphRegistry;
use crate::orrery_renderer::OrrerySnapshots;
use crate::strategy_registry::StrategyRegistry;
use crate::view_preset::{ProjectionPath, default_preset_for};

/// Per-call report from [`project_orreries`] — counts of what got
/// projected vs. skipped, for diagnostics and tests.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionReport {
    /// Orrery leaves that produced a fresh projection this call —
    /// painted snapshots plus exploded panes.
    pub projected: usize,
    /// Painted (Path A) leaves whose projection was written to the
    /// orrery snapshot map this call.
    pub painted: usize,
    /// Exploded (Path B) leaves collected into [`ProjectionOutcome::exploded`]
    /// for the caller to insert into the substrate scene.
    pub exploded: usize,
    /// Orrery leaves whose `graph_id` had no entry in the registry.
    pub missing_graph: usize,
    /// Orrery leaves with no substrate identity (the frame layout
    /// wasn't synced to the substrate scene, or the pane isn't
    /// currently mapped).
    pub missing_identity: usize,
    /// Orrery leaves whose preset routes to a strategy id the registry
    /// doesn't know about. Skipped without writing a snapshot.
    pub missing_strategy: usize,
    /// Non-orrery leaves walked through without action.
    pub skipped_non_orrery: usize,
}

/// One Path-B (exploded) pane the caller must insert into the
/// substrate scene via
/// [`crate::graph_node_explode::explode_projection_into_scene`].
/// Returned by value (not written to the scene inside
/// [`project_orreries`]) so the projection pass can borrow `HostApp`
/// immutably while the caller takes the `&mut scene` borrow afterward.
#[derive(Debug, Clone)]
pub struct ExplodedPane {
    pub pane_id: PaneId,
    /// Window-space top-left of the pane (pane-local projection
    /// coordinates add to this).
    pub pane_origin: kurbo::Point,
    pub projection: Projection,
}

/// Result of a [`project_orreries`] pass: the diagnostic report plus
/// the Path-B panes the caller still needs to explode into the scene.
#[derive(Debug, Default, Clone)]
pub struct ProjectionOutcome {
    pub report: ProjectionReport,
    pub exploded: Vec<ExplodedPane>,
}

/// Project every `PaneContent::Orrery` leaf in `layout` through the
/// strategy its preset resolves to in `strategies`, writing each
/// pane's projection into `orrery_snapshots` keyed by its substrate
/// `NodeIdentity`.
///
/// Per-pane viewport sizing comes from `walk_leaves`'s split-computed
/// bounds; the cartography `ViewIntent` shape is picked by the leaf's
/// [`ViewPreset`](crate::view_preset::ViewPreset) (form factor +
/// target-size policy).
///
/// `pane_identity_for` resolves a `PaneId` to its substrate identity —
/// typically `|id| host_app.identity_for_pane(id)`. Decoupled as a
/// closure so callers aren't forced to thread a specific HostApp
/// reference. Stale entries (orreries no longer in the layout) are
/// not swept here — call [`clear_orrery_snapshots`] before a fresh
/// pass when the frametree's orrery membership may have changed.
pub fn project_orreries<F>(
    layout: &FrameLayout,
    viewport_size: kurbo::Size,
    pane_identity_for: F,
    graph_registry: &GraphRegistry,
    strategies: &StrategyRegistry,
    orrery_snapshots: &OrrerySnapshots,
) -> ProjectionOutcome
where
    F: Fn(PaneId) -> Option<NodeIdentity>,
{
    let mut report = ProjectionReport::default();
    let mut exploded = Vec::new();
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
        let preset = default_preset_for(&leaf.content);
        let Some(strategy) = strategies.resolve(preset.default_strategy_id()) else {
            report.missing_strategy += 1;
            continue;
        };
        let intent = preset.intent_for(leaf.size);
        let request = ProjectionRequest {
            graph,
            signals: &signals,
            intent,
        };
        let projection = strategy.project(&request);
        match preset.projection_path() {
            ProjectionPath::Painted => {
                // Path A: orrery renderer paints the whole projection
                // inside the pane, keyed by the pane's substrate id.
                writer.insert(identity, projection);
                report.painted += 1;
            }
            ProjectionPath::Exploded => {
                // Path B: the caller explodes this into per-node
                // substrate entities. Clear any stale painted snapshot
                // so the pane's GraphView renderer falls to bg-only.
                writer.remove(&identity);
                exploded.push(ExplodedPane {
                    pane_id: leaf.pane_id,
                    pane_origin: leaf.placement.transform.translation().to_point(),
                    projection,
                });
                report.exploded += 1;
            }
        }
        report.projected += 1;
    }
    ProjectionOutcome { report, exploded }
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
/// when the caller has a `&HostApp` handy.
pub fn pane_identity_closure(
    host_app: &HostApp,
) -> impl Fn(PaneId) -> Option<NodeIdentity> + '_ {
    move |id| host_app.identity_for_pane(id)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use cartography::{LayoutStrategy, Projection, ProjectionMetadata, ProjectionRequest};
    use kurbo::Size;
    use frame::{FrameId, GraphId, PaneId, PaneNode, SplitAxis};
    use kernel::graph::Graph;
    use register_renderer::NodeIdentity;

    use super::*;
    use crate::graph_registry::GraphRegistry;
    use crate::orrery_renderer::OrreryRenderer;
    use crate::view_preset::ViewPreset;

    /// Stub strategy that returns an empty projection under the given
    /// `projection_id`. Lets each test register a stub at whatever id
    /// the preset under test routes to.
    struct StubStrategy(&'static str);
    impl LayoutStrategy for StubStrategy {
        fn projection_id(&self) -> &'static str {
            self.0
        }
        fn project(&self, _request: &ProjectionRequest<'_>) -> Projection {
            Projection {
                metadata: ProjectionMetadata {
                    strategy_id: Some(self.0.to_string()),
                    settled: true,
                },
                ..Projection::empty()
            }
        }
    }

    /// Build a [`StrategyRegistry`] containing a stub under the id the
    /// Orrery preset routes to. Default test fixture for the bulk of
    /// the projection-pass tests below.
    fn stub_registry_for_orrery() -> StrategyRegistry {
        let mut registry = StrategyRegistry::empty();
        registry.register(Box::new(StubStrategy(ViewPreset::Orrery.default_strategy_id())));
        registry
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
        let strategies = stub_registry_for_orrery();
        let snapshots = OrreryRenderer::default().snapshots();
        let outcome = project_orreries(
            &layout,
            Size::new(800.0, 600.0),
            |_| None,
            &graphs,
            &strategies,
            &snapshots,
        );
        assert_eq!(outcome.report.skipped_non_orrery, 1);
        assert_eq!(outcome.report.projected, 0);
    }

    #[test]
    fn reports_missing_graph_when_registry_lacks_id() {
        let (layout, _graph_id, orrery_pane) = workbench_plus_orrery_layout();
        let graphs = GraphRegistry::new();
        let strategies = stub_registry_for_orrery();
        let identity = NodeIdentity::next();
        let mut pane_to_identity = HashMap::new();
        pane_to_identity.insert(orrery_pane, identity);
        let snapshots = OrreryRenderer::default().snapshots();
        let outcome = project_orreries(
            &layout,
            Size::new(800.0, 600.0),
            |p| pane_to_identity.get(&p).copied(),
            &graphs,
            &strategies,
            &snapshots,
        );
        assert_eq!(outcome.report.missing_graph, 1);
        assert_eq!(outcome.report.projected, 0);
    }

    #[test]
    fn reports_missing_strategy_when_registry_lacks_preset_id() {
        let (layout, graph_id, orrery_pane) = workbench_plus_orrery_layout();
        let mut graphs = GraphRegistry::new();
        graphs.insert(graph_id, Graph::new());
        let identity = NodeIdentity::next();
        let mut pane_to_identity = HashMap::new();
        pane_to_identity.insert(orrery_pane, identity);
        let snapshots = OrreryRenderer::default().snapshots();

        let outcome = project_orreries(
            &layout,
            Size::new(800.0, 600.0),
            |p| pane_to_identity.get(&p).copied(),
            &graphs,
            &StrategyRegistry::empty(),
            &snapshots,
        );

        assert_eq!(outcome.report.missing_strategy, 1);
        assert_eq!(outcome.report.projected, 0);
    }

    #[test]
    fn orrery_pane_explodes_rather_than_painting() {
        // The Orrery preset routes to ProjectionPath::Exploded, so a
        // resolvable orrery pane yields an ExplodedPane (Path B) and
        // writes NO painted snapshot.
        let (layout, graph_id, orrery_pane) = workbench_plus_orrery_layout();
        let mut graphs = GraphRegistry::new();
        graphs.insert(graph_id, Graph::new());
        let strategies = stub_registry_for_orrery();
        let identity = NodeIdentity::next();
        let mut pane_to_identity = HashMap::new();
        pane_to_identity.insert(orrery_pane, identity);
        let snapshots = OrreryRenderer::default().snapshots();

        let outcome = project_orreries(
            &layout,
            Size::new(800.0, 600.0),
            |p| pane_to_identity.get(&p).copied(),
            &graphs,
            &strategies,
            &snapshots,
        );

        assert_eq!(outcome.report.projected, 1);
        assert_eq!(outcome.report.exploded, 1);
        assert_eq!(outcome.report.painted, 0);
        assert_eq!(outcome.exploded.len(), 1);
        assert_eq!(outcome.exploded[0].pane_id, orrery_pane);
        // No painted snapshot for an exploded pane.
        assert!(snapshots.read().unwrap().get(&identity).is_none());
        // The exploded projection carries the stub strategy's id.
        assert_eq!(
            outcome.exploded[0]
                .projection
                .metadata
                .strategy_id
                .as_deref(),
            Some(ViewPreset::Orrery.default_strategy_id())
        );
    }

    #[test]
    fn exploded_pane_clears_stale_painted_snapshot() {
        // If a pane previously painted (snapshot present) and now
        // explodes, the stale snapshot is removed so the GraphView
        // renderer falls back to bg-only.
        let (layout, graph_id, orrery_pane) = workbench_plus_orrery_layout();
        let mut graphs = GraphRegistry::new();
        graphs.insert(graph_id, Graph::new());
        let strategies = stub_registry_for_orrery();
        let identity = NodeIdentity::next();
        let mut pane_to_identity = HashMap::new();
        pane_to_identity.insert(orrery_pane, identity);
        let snapshots = OrreryRenderer::default().snapshots();
        // Seed a stale painted snapshot for this pane.
        snapshots
            .write()
            .unwrap()
            .insert(identity, Projection::empty());

        project_orreries(
            &layout,
            Size::new(800.0, 600.0),
            |p| pane_to_identity.get(&p).copied(),
            &graphs,
            &strategies,
            &snapshots,
        );

        assert!(snapshots.read().unwrap().get(&identity).is_none());
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
