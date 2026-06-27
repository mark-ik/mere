/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Construction, graph/physics lifecycle, and per-frame derived-state reconcile.

use super::*;

impl Orrery {
    /// A new orrery over an **empty** session graph. The host grows it with
    /// [`visit`](Orrery::visit) as the user navigates (the graph-rooted browse
    /// loop). For an isolated demo / the standalone bin, use
    /// [`with_sample_graph`](Orrery::with_sample_graph).
    pub fn new() -> Self {
        Self::from_graph(Graph::new())
    }

    /// A new orrery over a small built-in sample graph (a ring + spokes), seeded
    /// into a tight central spiral so the first settle is visible. The standalone
    /// `orrery-host` bin and the orrery tests use this; meerkat uses
    /// [`new`](Orrery::new) and drives the graph through [`visit`](Orrery::visit).
    pub fn with_sample_graph() -> Self {
        Self::from_graph(sample_graph())
    }

    /// Build an orrery over a **restored** session `graph`: keep each node at its
    /// saved (committed) position and do not auto-settle, so a reloaded session
    /// looks as it was left rather than re-scrambling into a fresh spiral.
    /// (Persistence host seam, S3.)
    pub fn with_graph(graph: Graph) -> Self {
        let mut orrery = Self::from_graph(graph);
        let positions: Vec<(NodeKey, Point2D<f32>)> = orrery
            .graph
            .nodes()
            .map(|(key, node)| {
                let p = node.projected_position();
                (key, Point2D::new(p.x, p.y))
            })
            .collect();
        for &(key, pos) in &positions {
            orrery.view.set_position(key, pos);
        }
        orrery.physics.seed(positions);
        orrery.physics.halt();
        orrery
    }

    /// Re-point this orrery at a different session `graph` **in place**, keeping the
    /// (possibly offloaded) physics actor and the node-children pool alive. The
    /// graph is replaced wholesale, every derived view is reconciled to it
    /// (departed nodes drop, the spring topology and node pool rebuild), per-graph
    /// interaction state (selection, hidden edges, drags, pushed node states/shapes)
    /// is cleared, and each node is restored to its committed position and halted
    /// so the switched-to session looks as it was left rather than re-scrambling.
    /// This is the Model-A graph swap the multi-graph switch drives. (Multi-graph MG2.)
    pub fn set_graph(&mut self, graph: Graph) {
        self.graph = graph;
        self.selected.clear();
        self.selected_edges.clear();
        self.hidden_edges.clear();
        self.active_field = None;
        self.hidden_fields.clear();
        self.node_states.clear();
        self.node_shapes.clear();
        self.node_faces.clear();
        self.node_sizes.clear();
        self.node_sprites.clear();
        self.node_sprite_hulls.clear();
        self.node_materials.clear();
        self.node_importance.clear();
        self.importance_dirty = true;
        self.community_cache = None;
        self.drag = None;
        self.field_drag = None;
        self.marquee = None;
        self.middle_drag = None;
        self.reconcile_derived();
        let positions: Vec<(NodeKey, Point2D<f32>)> = self
            .graph
            .nodes()
            .map(|(key, node)| {
                let p = node.projected_position();
                (key, Point2D::new(p.x, p.y))
            })
            .collect();
        for &(key, pos) in &positions {
            self.view.set_position(key, pos);
        }
        self.physics.seed(positions);
        self.physics.halt();
        self.generation += 1;
    }

    /// Build an orrery over `graph`: its [`build_simulation`], the node-children
    /// pool, and a default camera. Shared by [`new`](Orrery::new) (empty),
    /// [`with_sample_graph`](Orrery::with_sample_graph), and
    /// [`with_graph`](Orrery::with_graph).
    pub(crate) fn from_graph(graph: Graph) -> Self {
        let sim = build_simulation(&graph);
        let view = sim.view();
        let physics = Physics::inline(sim, SETTLE_TICKS);
        let (node_dom, gnode_of, stage_node) = build_pool_dom(&graph);
        Self {
            graph,
            physics,
            physics_paused: false,
            view,
            node_dom,
            node_layout: None,
            gnode_of,
            stage_node,
            pool_w: 0,
            pool_h: 0,
            camera: Camera::default(),
            style: dark_scene_style(),
            backdrop: surface_bg(),
            generation: 0,
            cursor: (0.0, 0.0),
            pan_velocity: (0.0, 0.0),
            middle_drag: None,
            orbit_drag: None,
            drag: None,
            field_drag: None,
            selected: HashSet::new(),
            selected_edges: HashSet::new(),
            hidden_edges: HashSet::new(),
            active_field: None,
            hidden_fields: HashSet::new(),
            node_states: HashMap::new(),
            node_shapes: HashMap::new(),
            node_faces: HashMap::new(),
            node_sizes: HashMap::new(),
            node_sprites: HashMap::new(),
            node_sprite_hulls: HashMap::new(),
            scene_sprite_textures: HashMap::new(),
            ambient: None,
            ambient_tincture: ColorF::new(0.0, 0.0, 0.0, 0.0),
            node_materials: HashMap::new(),
            size_by_degree: false,
            size_by_importance: false,
            importance_metric: signals::ImportanceMetric::Degree,
            node_importance: HashMap::new(),
            importance_dirty: true,
            community_cache: None,
            community_cache_revision: 0,
            last_strategy_inputs: None,
            show_community_rings: false,
            offthread_wake: None,
            community_actor: None,
            show_bridge_rings: false,
            bridge_cache: None,
            bridge_cache_revision: 0,
            bridge_metric: signals::BridgeMetric::default(),
            cluster_by_affinity: false,
            affinity_cache: None,
            affinity_cache_revision: 0,
            installed_affinity_revision: None,
            gloss_strategy: None,
            gloss_positions: None,
            gloss_cache_inputs: None,
            gloss_overlays: Vec::new(),
            gloss_scope_selection: false,
            gloss_size_by_importance: false,
            weighted_edges_cache: None,
            weighted_edges_rebuilds: 0,
            height_by_degree: false,
            marquee: None,
            ctrl: false,
            shift: false,
            alt: false,
            view_w: 1024,
            view_h: 600,
            active_strategy: None,
            strategy_positions: None,
            scope: None,
            render_as_cards: false,
        }
    }

    /// Set the viewport the orrery culls + centers against. The host calls this on
    /// a surface resize; the next [`frame`](Orrery::frame) rebuilds the node-pool
    /// layout at the new size.
    ///
    /// Keeps whatever world point sits at the viewport center fixed across the
    /// resize by shifting the offset by half the size delta (the camera maps
    /// `screen = world * zoom + offset`, so the center moves by half the change in
    /// each axis, independent of zoom). Without this, the startup 1024->2560 grow
    /// would leave a freshly centered camera anchored to the old, smaller center
    /// and slide the graph toward a corner.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (new_w, new_h) = (width.max(1), height.max(1));
        self.camera.offset.0 += (new_w as f32 - self.view_w as f32) / 2.0;
        self.camera.offset.1 += (new_h as f32 - self.view_h as f32) / 2.0;
        self.view_w = new_w;
        self.view_h = new_h;
    }

    /// Put the world origin at the viewport center **at zoom 1** — the graph is
    /// laid out around `(0, 0)`, so this frames it. Resets the zoom too (a drifted
    /// zoom would otherwise leave the graph a speck or off-screen even after the
    /// offset is centered). (A fit-to-`content_bounds` camera replaces this when a
    /// real graph is hosted.)
    pub fn recenter(&mut self) {
        self.camera.offset = (self.view_w as f32 / 2.0, self.view_h as f32 / 2.0);
        self.camera.zoom = 1.0;
    }

    /// Whether the graph is empty, or at least one node lies within the current
    /// viewport. `false` means every node is off-screen — a degenerate camera (a
    /// restored pan/zoom that no longer frames the graph), which the host recovers
    /// with [`recenter`](Self::recenter).
    pub fn graph_visible(&self) -> bool {
        self.graph.nodes().next().is_none()
            || self.view.cull_aabb(self.world_viewport()).into_iter().next().is_some()
    }

    /// Whether the graph currently holds any nodes. The host gates its one-shot
    /// camera heal on this so the heal waits for an async session load to populate
    /// the graph, rather than firing (and spending its one shot) against the empty
    /// graph that exists for the first frames after launch — at which point
    /// [`graph_visible`](Self::graph_visible) is trivially true and would suppress
    /// the recenter the restored-camera case needs.
    pub fn has_nodes(&self) -> bool {
        self.graph.nodes().next().is_some()
    }

    /// Re-sync the physics bodies, edge topology, and node-children pool to the
    /// current graph after a structural change. Does not seed positions, alter the
    /// selection, or restart the settle; callers do that as they need. The pool
    /// is structural, so it is rebuilt, not grown incrementally.
    pub(crate) fn reconcile_derived(&mut self) {
        // The graph topology changed (this is the topology-change hook), so any degree-derived
        // signal is stale: mark the importance cache for recompute on the next push. (Graph signals.)
        // The expensive caches (community) gate on `Graph::revision` instead — bumped at the kernel
        // mutation source, so a spurious reconcile (e.g. a selection change) cannot invalidate them.
        self.importance_dirty = true;
        let nodes: Vec<(NodeKey, Point2D<f32>)> = self
            .graph
            .nodes()
            .map(|(key, node)| {
                let p = node.projected_position();
                (key, Point2D::new(p.x, p.y))
            })
            .collect();
        self.physics.sync_nodes(nodes);
        self.physics.sync_edges(dedup_edges(&self.graph));
        // Re-resolve field couplings against the new node set, so a field gathers
        // nodes added after it was placed (its targets snapshot at build time).
        // (Field regions — rebuild-on-mutation / new-node capture.)
        self.rebuild_coupling_forces();
        let (node_dom, gnode_of, stage_node) = build_pool_dom(&self.graph);
        self.node_dom = node_dom;
        self.gnode_of = gnode_of;
        self.stage_node = stage_node;
        self.node_layout = None;
        self.resync_view_to_graph();
        // Degree-based sizes shift when the topology changes, so re-push radii (also
        // seeds them for newly-spawned bodies). No re-settle here: the structural
        // change's own settle covers it.
        self.push_node_geometry();
    }

    /// Push every node's collider/pick radius (`node_size / 2`) to the read-model view
    /// and the physics bodies, so a node sized in the view — size-by-degree or a per-node
    /// footprint — picks and collides at its true face (Decision 5). A no-op visually when
    /// every node is the uniform default. Does not settle; callers that change a size knob
    /// follow with [`resettle_for_size`](Self::resettle_for_size) so neighbors re-separate.
    /// (P0/P5 collider.)
    pub(crate) fn push_node_geometry(&mut self) {
        // Refresh the cached importance before sizing, so size-by-importance reads a current
        // snapshot (the normalization needs all nodes; do it once here, not per `node_size`).
        if self.size_by_importance {
            self.recompute_importance();
        }
        let radii: Vec<(NodeKey, f32)> = self
            .graph
            .nodes()
            .map(|(key, _)| (key, self.node_size(key) / 2.0))
            .collect();
        self.view.set_radii(radii.iter().copied());
        // The physics collider matches the *shape*, not just the size: a square node collides
        // square, a circle round (Decision 1 — the face is the collider). The view keeps the
        // bounding radius above for the pick + edge-trim. (Node-rep — collider matches shape.)
        let colliders: Vec<(NodeKey, gyre::NodeCollider)> =
            self.graph.nodes().map(|(key, _)| (key, self.node_collider(key))).collect();
        self.physics.set_node_colliders(colliders);
    }

    /// The collider shape for a node: the node's **body**. A node with a custom hull (the
    /// Body axis: traced from a sprite or hand-authored in the shape editor) collides at that
    /// hull, scaled from face-normalized coords to the current [`node_size`](Self::node_size);
    /// otherwise its content silhouette ([`node_shape`](Self::node_shape)) at its footprint.
    /// The hull applies **regardless of the face** ([`node_face`](Self::node_face)), so a node
    /// can wear a favicon over a custom-shaped body. So physics tracks the body, not just a
    /// bounding box. (Node body & face — the Body axis.)
    pub(crate) fn node_collider(&self, key: NodeKey) -> gyre::NodeCollider {
        let size = self.node_size(key);
        let half = size / 2.0;
        // A custom hull (sprite-traced or hand-authored) collides at that hull, scaled to the
        // face. Independent of the face texture — the body is its own axis.
        if let Some(hull) = self.node_sprite_hulls.get(&key).filter(|h| h.len() >= 3) {
            let points = hull.iter().map(|&(nx, ny)| (nx * size, ny * size)).collect();
            return gyre::NodeCollider::Hull { points, fallback: half };
        }
        match self.node_shape(key) {
            NodeShape::Circle => gyre::NodeCollider::Ball { radius: half },
            NodeShape::Square => gyre::NodeCollider::Square { half },
            NodeShape::Rounded => gyre::NodeCollider::RoundedSquare { half, border: half * 0.3 },
        }
    }

    /// Re-push radii and kick a gentle re-separation burst — the response to a size knob
    /// (a per-node footprint, the size-by-degree toggle) moving on an otherwise idle graph,
    /// so grown nodes actually push their neighbors apart. (P0/P5 collider.)
    pub(crate) fn resettle_for_size(&mut self) {
        self.push_node_geometry();
        self.physics.settle(SIZE_RESETTLE_TICKS);
    }

    /// Make the read model's node set match the graph after a structural change,
    /// **backend-free**: keep the live position of every node still present, fall
    /// back to its committed position for a node the view has not placed yet, drop
    /// departed nodes, and refresh the edge topology. This gives the next input
    /// read (hit-test, edge-pick) a correct node set synchronously, without
    /// waiting for the physics backend's next snapshot; the per-frame
    /// [`Physics::advance_frame`] then overwrites positions with authoritative
    /// ones. A subsequent seed overrides newly-added nodes' positions.
    pub(crate) fn resync_view_to_graph(&mut self) {
        let positions: Vec<(NodeKey, Point2D<f32>)> = self
            .graph
            .nodes()
            .map(|(key, node)| {
                let pos = self.view.position_of(key).unwrap_or_else(|| {
                    let p = node.projected_position();
                    Point2D::new(p.x, p.y)
                });
                (key, pos)
            })
            .collect();
        // A node-position-only refresh (no scene bodies / fluid here); the actor's per-tick
        // snapshots carry the live scene + fluid. (Physics scenes P1 / P4c.)
        self.view.apply_snapshot(&LayoutSnapshot {
            positions,
            scene: Vec::new(),
            fluid: Vec::new(),
            fluid_radius: 0.0,
            generation: self.generation,
        });
        self.view.set_edges(dedup_edges(&self.graph));
    }

    /// Whether the layout is still settling or a node is being dragged — the host
    /// chains another frame while true.
    pub fn is_settling(&self) -> bool {
        self.physics.is_settling()
    }

    /// Move physics onto an off-thread actor (the native always-offload path).
    /// The host calls this once, just after construction, with a [`Wake`] that
    /// pokes its event loop when a layout snapshot is ready. Left uncalled, the
    /// orrery keeps ticking in-thread (tests; the no-threads wasm profile).
    ///
    /// [`Wake`]: armillary::Wake
    pub fn offload_physics(&mut self, wake: armillary::Wake) {
        // Capture the host's wake as the orrery's off-thread wake: the community lane reuses it to
        // poke the same loop when its worker result lands, so it needs no separate host wiring.
        // `Some` here is also the "native + offloaded" signal that gates community off-thread.
        self.offthread_wake = Some(wake.clone());
        self.physics.offload(wake);
    }

    /// Park this orrery's physics: stop any in-progress settle so a backgrounded
    /// graph does not keep ticking and waking the host loop. The off-thread actor
    /// then idles on its command channel (no busy-spin, no CPU) and stays warm in
    /// the pool. The host calls this when a pooled graph loses focus. No explicit
    /// unpark is needed: the layout is left at its current positions (a restored
    /// session must not re-scramble), and any later interaction resumes the settle.
    /// (Window composition P1, OQ2 park.)
    pub fn park_physics(&mut self) {
        self.physics.halt();
    }

    /// Apply a structural mutation to the session graph and reconcile every derived
    /// view, so externally-ingested nodes/edges (e.g. a linked-data merge) join the
    /// spatial field. `mutate` returns whether it changed the graph; on a change,
    /// the newly-added nodes are fanned out around the current selection (they are
    /// minted at the origin, which the force sim must not stack), and the settle
    /// restarts. Returns whether the graph changed.
    ///
    /// orrery-host stays free of the linked-data bridge: the host passes the merge
    /// in as a closure over [`Graph`].
    pub fn ingest_graph<F: FnOnce(&mut Graph) -> bool>(&mut self, mutate: F) -> bool {
        let before: HashSet<NodeKey> = self.graph.nodes().map(|(k, _)| k).collect();
        if !mutate(&mut self.graph) {
            return false;
        }
        self.reconcile_derived();
        let anchor = self
            .selected
            .iter()
            .copied()
            .next()
            .and_then(|k| self.view.position_of(k))
            .unwrap_or(Point2D::new(0.0, 0.0));
        let seeds: Vec<_> = self
            .graph
            .nodes()
            .map(|(k, _)| k)
            .filter(|k| !before.contains(k))
            .enumerate()
            .map(|(i, k)| {
                let col = (i % 6) as f32;
                let row = (i / 6) as f32;
                (k, Point2D::new(anchor.x + 12.0 + col * 16.0, anchor.y + 12.0 + row * 16.0))
            })
            .collect();
        for &(key, pos) in &seeds {
            self.view.set_position(key, pos);
        }
        self.physics.seed(seeds);
        self.settle_physics(SETTLE_TICKS);
        true
    }

    /// Stamp a fetched favicon (RGBA8 + dimensions) onto the node currently at
    /// `url`, if one exists. A metadata-only change: unlike [`ingest_graph`], it
    /// neither reconciles the derived views nor disturbs the spatial layout (no node
    /// or edge is added), so a favicon arriving mid-browse does not jostle the field.
    /// The tile reads the stamped favicon on the next frame. Returns whether a node
    /// was found and its favicon changed. (Favicon-on-tile.)
    pub fn set_node_favicon(&mut self, url: &str, rgba: Vec<u8>, width: u32, height: u32) -> bool {
        let Some(key) = self.graph.get_node_by_url(url).map(|(k, _)| k) else {
            return false;
        };
        self.graph.set_node_favicon(key, rgba, width, height)
    }
}
