/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Minimap / gloss swatch geometry and gloss configuration.

use super::*;

impl Orrery {
    /// Graph geometry for a minimap swatch (gloss): each node's `(uuid, world
    /// position, selected)` and each visible edge as a world-space `(from, to)`
    /// segment. World coordinates — the consumer fits them into its own rect. The
    /// gloss pane draws its own swatch from this rather than rendering a second
    /// orrery (the Navigator is one surface, never a second instance).
    #[allow(clippy::type_complexity)]
    pub fn minimap_geometry(
        &self,
    ) -> (
        Vec<(uuid::Uuid, (f32, f32), bool, f32)>,
        Vec<((f32, f32), (f32, f32), f32)>,
    ) {
        // The mirror minimap draws uniform-size nodes (size factor 1.0) and uniform edges (weight
        // 1.0); the gloss lens ([`gloss_geometry`](Self::gloss_geometry)) is the path that varies
        // size by importance and edge thickness by multiplicity.
        let nodes = self
            .view
            .positions()
            .filter_map(|(key, p)| {
                self.graph
                    .get_node(key)
                    .map(|node| (node.id, (p.x, p.y), self.selected.contains(&key), 1.0))
            })
            .collect();
        let edges = self
            .view
            .edge_segments()
            .filter_map(|(a, b, pa, pb)| {
                let pair = if a <= b { (a, b) } else { (b, a) };
                (!self.hidden_edges.contains(&pair)).then_some(((pa.x, pa.y), (pb.x, pb.y), 1.0))
            })
            .collect();
        (nodes, edges)
    }

    /// The gloss swatch geometry built from **arbitrary** node positions (its own lens), rather than
    /// the live main-view positions [`minimap_geometry`](Self::minimap_geometry) mirrors. Returns the
    /// nodes (id, position, selected), the edges (de-duplicated, non-hidden relations between
    /// positioned pairs), and the **rings**: the signal overlays
    /// ([`gloss_overlays`](Self::gloss_overlays)) resolved to `(center, radius-factor, rgba)` at the
    /// lens positions (community halos coloured per cluster, bridge emphasis in bold near-white). The
    /// radius factor is a multiple of the swatch node size the renderer applies. Lets the gloss show
    /// a different arrangement *and* the cluster/broker structure than the main view. (Graph signals —
    /// P6 / P6b, the independent gloss projection + the overlay pipe.)
    #[allow(clippy::type_complexity)]
    pub fn gloss_geometry(
        &self,
        positions: &HashMap<NodeKey, PortablePoint>,
    ) -> (
        Vec<(uuid::Uuid, (f32, f32), bool, f32)>,
        Vec<((f32, f32), (f32, f32), f32)>,
        Vec<((f32, f32), f32, [f32; 4])>,
    ) {
        // Scope (P6c): when on with a non-empty selection, show only the selected nodes (+ induced
        // edges + halos over selected members), cropped — the swatch auto-refits, zooming to the
        // selection. A pure render-time filter, so it tracks the selection live. Empty selection or
        // the toggle off shows the whole lens.
        let scoped: Option<&HashSet<NodeKey>> =
            (self.gloss_scope_selection && !self.selected.is_empty()).then_some(&self.selected);
        let in_scope = |key: &NodeKey| scoped.is_none_or(|s| s.contains(key));
        // Encoding (P6c): when size-by-importance is on, scale each node by the importance signal
        // (0..=1) into a 0.7..=1.9 factor, independent of the main view's sizing. The renderer
        // multiplies the swatch node size by this. Off => uniform 1.0.
        let size_factor = |key: &NodeKey| -> f32 {
            if self.gloss_size_by_importance {
                0.7 + self.node_importance.get(key).copied().unwrap_or(0.0) * 1.2
            } else {
                1.0
            }
        };
        let nodes = positions
            .iter()
            .filter(|(key, _)| in_scope(key))
            .filter_map(|(&key, p)| {
                self.graph.get_node(key).map(|node| {
                    (
                        node.id,
                        (p.x, p.y),
                        self.selected.contains(&key),
                        size_factor(&key),
                    )
                })
            })
            .collect();
        // Edges carry their multiplicity weight (the number of relations between the pair), so the
        // gloss draws thicker edges for denser pairs — the same free channel as the main view. The
        // weighted edge list comes from the revision-gated memo (cache C) when fresh, else a direct
        // recompute (a call before the first frame). (Graph signals — gloss edge-thickness + memos.)
        let fresh_edges = self
            .weighted_edges_cache
            .as_ref()
            .filter(|(rev, _)| *rev == self.graph.revision());
        let fallback_edges;
        let weighted: &[(NodeKey, NodeKey, u32)] = match fresh_edges {
            Some((_, edges)) => edges,
            None => {
                fallback_edges = dedup_edges_weighted(&self.graph);
                &fallback_edges
            }
        };
        let edges = weighted
            .iter()
            .filter_map(|&(a, b, weight)| {
                if !in_scope(&a) || !in_scope(&b) {
                    return None;
                }
                let pair = if a <= b { (a, b) } else { (b, a) };
                if self.hidden_edges.contains(&pair) {
                    return None;
                }
                let pa = positions.get(&a)?;
                let pb = positions.get(&b)?;
                Some(((pa.x, pa.y), (pb.x, pb.y), weight as f32))
            })
            .collect();
        // Resolve the stored overlays to rings at the lens positions. Cluster halos enumerate in
        // cluster order (so the colour index matches the main view); bridge emphasis is the bold
        // near-white. A node the lens did not place, or one out of scope, is skipped. (Graph
        // signals — P6b / P6c.)
        let mut rings: Vec<((f32, f32), f32, [f32; 4])> = Vec::new();
        let mut cluster_index = 0usize;
        for overlay in &self.gloss_overlays {
            match overlay {
                signals::Overlay::ClusterHalo { members, .. } => {
                    let c = cluster_color(cluster_index);
                    cluster_index += 1;
                    let color = [c.r, c.g, c.b, GLOSS_CLUSTER_RING_ALPHA];
                    for member in members {
                        if in_scope(member) {
                            if let Some(p) = positions.get(member) {
                                rings.push(((p.x, p.y), GLOSS_CLUSTER_RING_FACTOR, color));
                            }
                        }
                    }
                }
                signals::Overlay::BridgeEmphasis { node, .. } => {
                    if in_scope(node) {
                        if let Some(p) = positions.get(node) {
                            rings.push((
                                (p.x, p.y),
                                GLOSS_BRIDGE_RING_FACTOR,
                                GLOSS_BRIDGE_RING_RGBA,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        (nodes, edges, rings)
    }

    /// The gloss lens's layout strategy, or `None` to mirror the main view. (Graph signals — P6.)
    pub fn gloss_strategy(&self) -> Option<&str> {
        self.gloss_strategy.as_deref()
    }

    /// Set the gloss lens (`Some(strategy_id)` for an independent arrangement, `None` to mirror the
    /// main view) and invalidate its cache so the next frame recomputes. (Graph signals — P6.)
    pub fn set_gloss_strategy(&mut self, id: Option<String>) {
        self.gloss_strategy = id;
        self.gloss_cache_inputs = None;
        self.gloss_positions = None;
        self.gloss_overlays.clear();
    }

    /// Whether the host must recompute the gloss lens: `true` when a gloss strategy is set and its
    /// inputs (strategy, graph revision, viewport, **and the ring-toggle states** — they choose which
    /// overlays the lens carries) differ from the cached ones. `false` when the gloss mirrors the
    /// main view (no own arrangement). (Graph signals — P6 / P6b.)
    pub fn gloss_needs_recompute(&self, w: u32, h: u32) -> bool {
        let Some(id) = &self.gloss_strategy else {
            return false;
        };
        // The by-site kanban groups by URL host — node *content* the structural revision does not
        // track (a url edit is content, not structure) — so its lens is recomputed every frame
        // (cheap), the same guard the main-view cache uses. (Graph signals — the layout cache.)
        if id == "kanban.default" {
            return true;
        }
        // Focus only matters for a focus-driven lens (radial); for every other strategy a selection
        // change must not invalidate the cached lens. Mirrors the main-view cache.
        let focus = Self::strategy_uses_focus(id)
            .then(|| self.focused_key())
            .flatten();
        match &self.gloss_cache_inputs {
            Some((sid, rev, sw, sh, rings, bridges, scope, sfocus)) => {
                sid != id
                    || *rev != self.graph.revision()
                    || *sw != w
                    || *sh != h
                    || *rings != self.show_community_rings
                    || *bridges != self.show_bridge_rings
                    || *scope != self.gloss_scope_keys()
                    || *sfocus != focus
            }
            None => true,
        }
    }

    /// The gloss lens's scope: the sorted selection keys when [`gloss_scope_selection`]
    /// (Self::gloss_scope_selection) is on and the selection is non-empty, else `None` (whole graph).
    /// The host projects the **induced subgraph** of these keys for the lens, and it is part of the
    /// gloss cache key, so changing the selection re-lays-out the subgraph. (Graph signals — P6c.)
    pub fn gloss_scope_keys(&self) -> Option<Vec<NodeKey>> {
        if self.gloss_scope_selection && !self.selected.is_empty() {
            let mut keys: Vec<NodeKey> = self.selected.iter().copied().collect();
            keys.sort();
            Some(keys)
        } else {
            None
        }
    }

    /// Store the host-computed gloss lens positions + overlays + record the inputs they were computed
    /// for, so [`gloss_needs_recompute`](Self::gloss_needs_recompute) returns `false` until one
    /// changes. The overlays ride the same `project_orrery_lens` projection as the positions (the
    /// overlay pipe); [`gloss_geometry`](Self::gloss_geometry) resolves them to rings. (Graph signals
    /// — P6 / P6b.)
    pub fn set_gloss_positions(
        &mut self,
        positions: Vec<(NodeKey, PortablePoint)>,
        overlays: Vec<signals::Overlay>,
        w: u32,
        h: u32,
    ) {
        self.gloss_positions = Some(positions.into_iter().collect());
        self.gloss_overlays = overlays;
        if let Some(id) = self.gloss_strategy.clone() {
            let focus = Self::strategy_uses_focus(&id)
                .then(|| self.focused_key())
                .flatten();
            self.gloss_cache_inputs = Some((
                id,
                self.graph.revision(),
                w,
                h,
                self.show_community_rings,
                self.show_bridge_rings,
                self.gloss_scope_keys(),
                focus,
            ));
        }
    }

    /// The gloss swatch geometry (nodes, edges, rings) from the cached lens positions + overlays
    /// (all empty until first computed). The host draws this when a gloss strategy is set; otherwise
    /// it uses [`minimap_geometry`](Self::minimap_geometry). (Graph signals — P6 / P6b.)
    #[allow(clippy::type_complexity)]
    pub fn gloss_geometry_cached(
        &self,
    ) -> (
        Vec<(uuid::Uuid, (f32, f32), bool, f32)>,
        Vec<((f32, f32), (f32, f32), f32)>,
        Vec<((f32, f32), f32, [f32; 4])>,
    ) {
        match &self.gloss_positions {
            Some(positions) => self.gloss_geometry(positions),
            None => (Vec::new(), Vec::new(), Vec::new()),
        }
    }

    /// Toggle the gloss **scope**: when on, the gloss lens crops to the current selection (+ induced
    /// edges), zooming the swatch to it; empty selection shows the whole graph. (Graph signals — P6c.)
    pub fn set_gloss_scope_selection(&mut self, on: bool) {
        self.gloss_scope_selection = on;
    }

    /// Whether the gloss lens is scoped to the selection. (Graph signals — P6c.)
    pub fn gloss_scope_selection(&self) -> bool {
        self.gloss_scope_selection
    }

    /// Toggle the gloss **encoding**: when on, the gloss lens sizes nodes by the importance signal,
    /// independent of the main view's sizing. (Graph signals — P6c.)
    pub fn set_gloss_size_by_importance(&mut self, on: bool) {
        self.gloss_size_by_importance = on;
    }

    /// Whether the gloss lens sizes nodes by importance. (Graph signals — P6c.)
    pub fn gloss_size_by_importance(&self) -> bool {
        self.gloss_size_by_importance
    }
}
