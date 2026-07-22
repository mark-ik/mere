// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Layout strategy, community detection, bridges, and affinity clustering.

use super::*;

impl Canvas {
    /// The pane's active layout-strategy id, or `None` for force-directed (seiche).
    /// The host persists this as view-intent and checkmarks it in the layout picker.
    pub fn layout_strategy(&self) -> Option<&str> {
        self.active_strategy.as_deref()
    }

    /// Switch the canvas's layout strategy. `Some(id)` selects a cartography adapter
    /// (the host then pushes its positions via [`apply_strategy_positions`]) and halts
    /// seiche so the analytic layout holds still; `None` reverts to force-directed,
    /// dropping the buffered positions and re-settling the physics. (Layout picker.)
    pub fn set_layout_strategy(&mut self, id: Option<String>) {
        let reverting = id.is_none() && self.active_strategy.is_some();
        self.active_strategy = id;
        // Any strategy (de)activation forces a fresh layout: the buffered positions are recomputed
        // (or dropped on revert), so the inputs cache must not skip the first recompute of the new
        // strategy. (Arrangements — the layout cache.)
        self.last_strategy_inputs = None;
        if self.active_strategy.is_some() {
            self.physics.halt();
        } else if reverting {
            self.strategy_positions = None;
            self.settle_physics(SETTLE_TICKS);
        }
    }

    /// Whether a strategy's layout depends on the **focus** (the single selection). Only radial
    /// centers on it; for every other strategy a selection change must not invalidate the cached
    /// layout. (Arrangements — the layout cache.)
    pub(crate) fn strategy_uses_focus(id: &str) -> bool {
        id == "radial.default"
    }

    /// Whether the active analytic layout must be recomputed: `true` when its inputs — the strategy,
    /// the kernel's structural [`Graph::revision`](kernel::graph::Graph::revision), the viewport, and
    /// the focus (only for focus-driven strategies) — differ from the last computed layout. The host
    /// gates its per-frame `project_canvas_strategy` call on this, so an unchanged analytic layout is
    /// computed once per real change, not every frame. (Arrangements — the layout cache.)
    pub fn needs_strategy_recompute(
        &self,
        id: &str,
        w: u32,
        h: u32,
        focus: Option<NodeKey>,
    ) -> bool {
        // The by-site kanban groups by URL host — node *content* the structural revision does not
        // track (a url edit is content, not structure). Its layout is cheap (host extraction +
        // grouping), so recompute it every frame rather than risk a stale column. The structural
        // strategies (grid, penrose, radial, timeline, community-kanban, ...) cache on the revision.
        if id == "kanban.default" {
            return true;
        }
        let focus = if Self::strategy_uses_focus(id) {
            focus
        } else {
            None
        };
        match &self.last_strategy_inputs {
            Some((sid, rev, sw, sh, sfocus)) => {
                sid.as_str() != id
                    || *rev != self.graph.revision()
                    || *sw != w
                    || *sh != h
                    || *sfocus != focus
            }
            None => true,
        }
    }

    /// Record the inputs the active layout was just computed for, so
    /// [`needs_strategy_recompute`](Self::needs_strategy_recompute) returns `false` until one
    /// changes. The host calls this right after it projects + applies. (Arrangements — the cache.)
    pub fn note_strategy_computed(&mut self, id: &str, w: u32, h: u32, focus: Option<NodeKey>) {
        let focus = if Self::strategy_uses_focus(id) {
            focus
        } else {
            None
        };
        self.last_strategy_inputs = Some((id.to_string(), self.graph.revision(), w, h, focus));
    }

    /// Per-node visual extents `(w, h)` in px for the extent-aware strategy
    /// path: each node's resolved face footprint ([`node_size`](Self::node_size),
    /// so per-node overrides and the size-by-degree/importance channels ride
    /// along). The host measures, the strategy places. (Projection proofs — P2.)
    pub fn strategy_extents(&self) -> HashMap<NodeKey, (f32, f32)> {
        self.graph
            .nodes()
            .map(|(key, _)| {
                let side = self.node_size(key);
                (key, (side, side))
            })
            .collect()
    }

    /// Buffer the active strategy's node positions (host-computed through platen's
    /// cartography dispatch). They are written into the read model each frame after
    /// the physics snapshot, so they take effect regardless of the off-thread sim's
    /// timing. A no-op unless a strategy is active. (Layout picker.)
    pub fn apply_strategy_positions(&mut self, positions: &[(NodeKey, PortablePoint)]) {
        if self.active_strategy.is_some() {
            self.strategy_positions = Some(positions.to_vec());
        }
    }

    /// Record the portable score that produced the active analytic layout.
    /// Passing `None` drops a stale score when the host selects another
    /// strategy or returns to force physics.
    pub fn set_projection_score(&mut self, score: Option<sceno::Score>) {
        self.projection_score = score;
    }

    /// The last score supplied by the product adapter, for persistence and
    /// proof receipts.
    pub fn projection_score(&self) -> Option<&sceno::Score> {
        self.projection_score.as_ref()
    }

    /// Restore a persisted Mere Spiral score into the live strategy buffer.
    ///
    /// The shared score remains product-free. This boundary resolves only the
    /// `mere.graph` opaque refs back to the graph's current node keys, then
    /// reinstates the local Spiral strategy. Unknown score shapes are retained
    /// as sidecar state but deliberately do not invent a local renderer.
    pub fn restore_projection_score(&mut self, score: sceno::Score) -> bool {
        self.projection_score = Some(score.clone());
        if !matches!(score.arrangement, sceno::Arrangement::Spiral(_)) {
            return false;
        }
        let scene = scenomise::solve(&score);
        let positions: Vec<_> = scene
            .items
            .iter()
            .filter_map(|item| {
                let source = scene.sources.get(item.source.0 as usize)?;
                if source.adapter != ::cartography::MERE_GRAPH_ADAPTER {
                    return None;
                }
                let id = uuid::Uuid::parse_str(&source.id).ok()?;
                let key = self.graph.get_node_key_by_id(id)?;
                Some((
                    key,
                    PortablePoint::new(item.transform.translate.x, item.transform.translate.y),
                ))
            })
            .collect();
        if positions.is_empty() {
            return false;
        }
        self.active_strategy = Some("phyllotaxis.default".to_string());
        self.last_strategy_inputs = None;
        self.physics.halt();
        self.strategy_positions = Some(positions);
        true
    }

    /// Refresh the cached community partition if the active strategy needs it (cluster-kanban) and
    /// the topology generation has advanced since it was last computed. The generation-gated cache:
    /// Louvain runs once per structural change, not once per frame (the host calls this before
    /// projecting, then reads [`community`](Self::community)). Computed inline here; the off-thread
    /// armillary lane is a drop-in behind this method, exactly as physics offloads. (Graph signals — P3.)
    pub fn refresh_community_cache(&mut self, strategy_id: &str) {
        // Community is needed by the cluster-kanban layout and by the community-ring overlay.
        if strategy_id == "kanban.community" || self.show_community_rings {
            self.ensure_community_fresh();
        }
    }

    /// Recompute the community partition if it is stale for the current graph revision — off-thread
    /// when the host has offloaded (native), inline otherwise (wasm / tests). The caller decides
    /// *whether* community is needed (cluster-kanban or the rings toggle); this only refreshes.
    /// (Graph signals — P3.)
    pub(crate) fn ensure_community_fresh(&mut self) {
        let revision = self.graph.revision();
        // Already fresh for this revision (cache or an in-flight request) — nothing to do.
        if self.community_cache.is_some() && self.community_cache_revision == revision {
            return;
        }
        let Some(wake) = self.offthread_wake.clone() else {
            // Inline path (wasm / tests / physics not offloaded): compute synchronously.
            self.community_cache = Some(signals::community_louvain(&self.graph));
            self.community_cache_revision = revision;
            return;
        };
        // Off-thread path: spin up the worker lazily on first need, then dispatch this revision.
        // The result lands via `drain_community` on a later frame; until then the last-good
        // partition (or the inline fallback in the kanban projection) holds. (Graph signals.)
        if self.community_actor.is_none() {
            self.community_actor = Some(community_lane::CommunityActor::spawn(wake));
        }
        let snapshot = signals::CommunitySnapshot::from_graph(&self.graph);
        if let Some(actor) = self.community_actor.as_mut() {
            actor.request(snapshot, revision);
        }
    }

    /// Drain the off-thread community worker and accept its freshest result **only if it still
    /// matches the live graph revision** (stale-rejection — a partition computed against a
    /// since-mutated graph is dropped, and the next [`refresh_community_cache`] re-dispatches). A
    /// no-op when computing inline (no worker). Called once per frame. (Graph signals — P3.)
    pub(crate) fn drain_community(&mut self) {
        let update = match self.community_actor.as_mut() {
            Some(actor) => actor.drain(),
            None => return,
        };
        let Some(update) = update else { return };
        if update.revision == self.graph.revision() {
            self.community_cache = Some(update.clusters);
            self.community_cache_revision = update.revision;
        }
    }

    /// The cached community partition, or `None` if none has been computed (no cluster strategy has
    /// run this session, or the canvas was cleared). The host threads it into the cluster-kanban
    /// projection so Louvain is not re-run per frame. (Graph signals — P3.)
    pub fn community(&self) -> Option<&signals::ClusterSet> {
        self.community_cache.as_ref()
    }

    /// Toggle the community-ring overlay: a halo per node in its community's colour, in any layout.
    /// (Graph signals — community to a ring.)
    pub fn set_show_community_rings(&mut self, on: bool) {
        self.show_community_rings = on;
    }

    /// Whether the community-ring overlay is on. (Graph signals — community to a ring.)
    pub fn show_community_rings(&self) -> bool {
        self.show_community_rings
    }

    /// Toggle the bridge-ring overlay: a bold ring on the structural broker nodes, in any layout.
    /// (Graph signals — bridges.)
    pub fn set_show_bridge_rings(&mut self, on: bool) {
        self.show_bridge_rings = on;
    }

    /// Whether the bridge-ring overlay is on. (Graph signals — bridges.)
    pub fn show_bridge_rings(&self) -> bool {
        self.show_bridge_rings
    }

    /// The bridge metric (betweenness brokers vs articulation / cut vertices). (Graph signals.)
    pub fn bridge_metric(&self) -> signals::BridgeMetric {
        self.bridge_metric
    }

    /// Choose the bridge metric. Invalidates the bridge cache so the next
    /// [`ensure_bridges_fresh`](Self::ensure_bridges_fresh) recomputes under the new metric (the
    /// graph revision may not have moved). (Graph signals — bridges / articulation points.)
    pub fn set_bridge_metric(&mut self, metric: signals::BridgeMetric) {
        if self.bridge_metric != metric {
            self.bridge_metric = metric;
            self.bridge_cache = None;
        }
    }

    /// Recompute the bridge set if stale for the current graph revision, under the chosen metric
    /// (betweenness brokers, thresholded; or articulation points). Both are cheap (O(V·E) /
    /// O(V+E)), so this stays inline (no off-thread lane); the revision gate avoids the per-frame
    /// redo, and [`set_bridge_metric`](Self::set_bridge_metric) clears the cache on a metric change.
    /// (Graph signals — bridges.)
    pub(crate) fn ensure_bridges_fresh(&mut self) {
        let revision = self.graph.revision();
        if self.bridge_cache.is_some() && self.bridge_cache_revision == revision {
            return;
        }
        self.bridge_cache = Some(signals::bridges(&self.graph, self.bridge_metric, 0.5));
        self.bridge_cache_revision = revision;
    }

    /// The cached bridge set, if computed. (Graph signals — bridges.)
    pub fn bridges(&self) -> Option<&signals::BridgeNodes> {
        self.bridge_cache.as_ref()
    }

    /// Refresh the revision-gated weighted-edge memo (cache generalization C): recompute the
    /// collapsed multiplicity-weighted edge list only when the graph structure changed since it was
    /// last built, so the per-frame gloss redraw reads it instead of re-deduping `relations()` every
    /// frame. Called from [`frame`](Self::frame). (Graph signals — query memos.)
    pub(crate) fn refresh_weighted_edges(&mut self) {
        let revision = self.graph.revision();
        if self
            .weighted_edges_cache
            .as_ref()
            .is_none_or(|(r, _)| *r != revision)
        {
            self.weighted_edges_cache = Some((revision, dedup_edges_weighted(&self.graph)));
            self.weighted_edges_rebuilds += 1;
        }
    }

    /// How many times the weighted-edge memo recomputed (test introspection: a static frame must not
    /// bump it). (Graph signals — query memos, C.)
    #[cfg(test)]
    pub(crate) fn weighted_edges_rebuilds(&self) -> u64 {
        self.weighted_edges_rebuilds
    }

    /// Toggle the **affinity force**: a weighted, attract-only seiche spring over structural-Jaccard
    /// similarity, drawing structurally-similar nodes into clusters on top of the force-directed
    /// layout ("cluster by affinity"). The force is (un)installed on the next [`frame`](Self::frame)
    /// via [`sync_affinity_force`](Self::sync_affinity_force), with a settle so the change takes.
    /// (Graph signals — P4.)
    pub fn set_cluster_by_affinity(&mut self, on: bool) {
        self.cluster_by_affinity = on;
    }

    /// Whether the affinity-clustering force is on. (Graph signals — P4.)
    pub fn cluster_by_affinity(&self) -> bool {
        self.cluster_by_affinity
    }

    /// Inject a host-computed **content-affinity** signal — semantic similarity from node
    /// embeddings, as `(a, b, weight)` triples (`weight` in `0..=1`) — to drive the affinity force,
    /// superseding the internal structural-Jaccard signal while set. `None` reverts to structural.
    ///
    /// The host owns the embedding provider (so burn stays out of the canvas), recomputes this when
    /// node *content* changes, and re-injects; the canvas installs it on the next [`frame`](Self::frame)
    /// under the [`cluster_by_affinity`](Self::set_cluster_by_affinity) toggle, with a settle so the
    /// new clustering takes. `Some(empty)` is authoritative-but-inert (the host ran embeddings and
    /// found no pairs above its threshold) — it clears the force rather than falling back to
    /// structural; pass `None` to opt back into structural. (burn brief Lane 5 — P4.)
    pub fn set_content_affinity(&mut self, pairs: Option<Vec<(NodeKey, NodeKey, f32)>>) {
        self.content_affinity = pairs;
        self.content_affinity_dirty = true;
    }

    /// Whether a host content-affinity signal is currently the affinity source (vs the internal
    /// structural one). `Some(empty)` still counts as content-sourced. (burn brief Lane 5 — P4.)
    pub fn has_content_affinity(&self) -> bool {
        self.content_affinity.is_some()
    }

    /// Set how the structural and content affinity signals combine ([`AffinityBlend`]). Default is
    /// [`Blend`](AffinityBlend::Blend) (noisy-OR of both). Forces a rebuild of the affinity force on
    /// the next [`frame`](Self::frame) so the mode change takes. (burn brief Lane 5 — P6.)
    pub fn set_affinity_blend(&mut self, mode: AffinityBlend) {
        if self.affinity_blend == mode {
            return;
        }
        self.affinity_blend = mode;
        // The contributing source set changed; re-arm both gates so the next sync reinstalls.
        self.installed_affinity_revision = None;
        self.content_affinity_dirty = true;
    }

    /// How the structural and content affinity signals currently combine. (burn brief Lane 5 — P6.)
    pub fn affinity_blend(&self) -> AffinityBlend {
        self.affinity_blend
    }

    /// The number of affinity pairs in the live affinity force (`0` when off). Test introspection
    /// for the P4 wiring (inline backend). (Graph signals — P4.)
    #[cfg(test)]
    pub(crate) fn affinity_pair_count(&self) -> usize {
        self.physics.affinity_pair_count()
    }

    /// Recompute the affinity signal if stale for the current graph revision. Jaccard is cheap (like
    /// betweenness at current scale), so this stays inline; the revision gate avoids the per-frame
    /// redo. (Graph signals — P4.)
    pub(crate) fn ensure_affinity_fresh(&mut self) {
        let revision = self.graph.revision();
        if self.affinity_cache.is_some() && self.affinity_cache_revision == revision {
            return;
        }
        self.affinity_cache = Some(signals::structural_affinity(
            &self.graph,
            AFFINITY_MIN_SIMILARITY,
        ));
        self.affinity_cache_revision = revision;
    }

    /// Install / refresh / clear the affinity force to match the toggle + the current signal, once
    /// per real change (not per frame). Called from [`frame`](Self::frame). When on, recompute the
    /// signal if stale and (re)install the force only when the graph revision moved since the live
    /// force was built; when off, clear it once. Each install/clear is followed by a settle so the
    /// new equilibrium takes. (Graph signals — P4.)
    pub(crate) fn sync_affinity_force(&mut self) {
        if self.cluster_by_affinity {
            let revision = self.graph.revision();
            let has_content = self.content_affinity.is_some();
            // Which sources feed the force under the current blend mode. `ContentOnly` with no
            // injected signal falls back to structural, so the toggle always does something.
            let (use_structural, use_content) = match self.affinity_blend {
                AffinityBlend::StructuralOnly => (true, false),
                AffinityBlend::ContentOnly => (!has_content, has_content),
                AffinityBlend::Blend => (true, has_content),
            };
            // Structural is revision-gated (recompute the Jaccard memo if stale); content is
            // host-fresh (dirty-gated). Reinstall when a *contributing* source changed, or when
            // nothing is installed yet.
            if use_structural {
                self.ensure_affinity_fresh();
            }
            let structural_changed =
                use_structural && self.installed_affinity_revision != Some(revision);
            let content_changed = use_content && self.content_affinity_dirty;
            if self.affinity_force_installed && !structural_changed && !content_changed {
                // Nothing that feeds the live force moved; leave it (no per-frame rebuild).
            } else {
                let structural = use_structural
                    .then(|| self.affinity_cache.as_ref())
                    .flatten();
                let content = use_content
                    .then(|| self.content_affinity.as_deref())
                    .flatten();
                let pairs = blend_affinity_pairs(structural, content);
                let force = (!pairs.is_empty()).then(|| AffinitySpring::new(pairs));
                self.affinity_force_installed = force.is_some();
                self.physics.set_affinity_force(force);
                self.physics.settle(SETTLE_TICKS / 2);
                // Structural is what the revision gate tracks; content-only installs leave it `None`
                // so a later switch back to structural/blend reinstalls from the current revision.
                self.installed_affinity_revision = use_structural.then_some(revision);
                self.content_affinity_dirty = false;
            }
        } else if self.affinity_force_installed {
            self.physics.set_affinity_force(None);
            self.physics.settle(SETTLE_TICKS / 2);
            self.affinity_force_installed = false;
            self.installed_affinity_revision = None;
            // Re-arm the content signal so re-enabling the toggle reinstalls it (the dirty flag was
            // consumed at install; without this a toggle off→on would leave content uninstalled).
            self.content_affinity_dirty = true;
        }
    }

    /// Overlay the buffered strategy positions onto `view` — called by
    /// [`frame`](Canvas::frame) right after the physics snapshot, so the underlay,
    /// DOM nodes, cull, and edges (all reading `view`) stay consistent in one write.
    /// A no-op under force-directed. (Layout picker.)
    pub(crate) fn apply_strategy_to_view(&mut self) {
        let Some(positions) = self.strategy_positions.take() else {
            return;
        };
        for &(key, p) in &positions {
            self.view.set_position(key, Point2D::new(p.x, p.y));
        }
        self.strategy_positions = Some(positions);
    }
}
