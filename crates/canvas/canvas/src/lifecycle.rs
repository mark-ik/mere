// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Construction, graph/physics lifecycle, and per-frame derived-state reconcile.

use super::*;

impl Canvas {
    /// A new canvas over an **empty** session graph. The host grows it with
    /// [`visit`](Canvas::visit) as the user navigates (the graph-rooted browse
    /// loop). For an isolated demo / the standalone bin, use
    /// [`with_sample_graph`](Canvas::with_sample_graph).
    pub fn new() -> Self {
        Self::from_graph(Graph::new())
    }

    /// A new canvas over a small built-in sample graph (a ring + spokes), seeded
    /// into a tight central spiral so the first settle is visible. The standalone
    /// `canvas-host` bin and the canvas tests use this; meerkat uses
    /// [`new`](Canvas::new) and drives the graph through [`visit`](Canvas::visit).
    pub fn with_sample_graph() -> Self {
        Self::from_graph(sample_graph())
    }

    /// Build an canvas over a **restored** session `graph`: park every node at the
    /// origin and halt (do not auto-settle), so the host can then apply the saved
    /// layout from its cartography sidecar via [`view`](Canvas::view) / physics
    /// seeding without the sim re-scrambling first. Positions are no longer graph
    /// truth (S2), so they no longer ride the graph snapshot. (Persistence host seam, S3.)
    pub fn with_graph(graph: Graph) -> Self {
        let mut canvas = Self::from_graph(graph);
        let positions: Vec<(NodeKey, Point2D<f32>)> = canvas
            .graph
            .nodes()
            .map(|(key, _node)| (key, Point2D::zero()))
            .collect();
        for &(key, pos) in &positions {
            canvas.view.set_position(key, pos);
        }
        canvas.physics.seed(positions);
        canvas.physics.halt();
        canvas
    }

    /// Re-point this canvas at a different session `graph` **in place**, keeping the
    /// (possibly offloaded) physics actor and the node-children pool alive. The
    /// graph is replaced wholesale, every derived view is reconciled to it
    /// (departed nodes drop, the spring topology and node pool rebuild), per-graph
    /// interaction state (selection, hidden edges, drags, pushed node states/shapes)
    /// is cleared, and every node is parked at the origin and halted so the host can
    /// re-apply the switched-to session's saved layout from its cartography sidecar
    /// without the sim re-scrambling first (positions are no longer graph truth, S2).
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
        self.derived_face_cache.clear();
        self.node_sizes.clear();
        self.node_sprites.clear();
        self.node_sprite_hulls.clear();
        self.node_materials.clear();
        self.pending_image_requests.clear();
        self.requested_images.clear();
        self.node_importance.clear();
        self.importance_dirty = true;
        self.node_recency.clear();
        self.projection_score = None;
        self.projection_representations.clear();
        self.restored_score_hold = None;
        self.community_cache = None;
        self.drag = None;
        self.pinned_nodes.clear();
        self.field_drag = None;
        self.marquee = None;
        self.middle_drag = None;
        self.fold = None;
        self.fold_press = None;
        self.fold_undo.clear();
        self.fold_redo.clear();
        self.reconcile_derived();
        let positions: Vec<(NodeKey, Point2D<f32>)> = self
            .graph
            .nodes()
            .map(|(key, _node)| (key, Point2D::zero()))
            .collect();
        for &(key, pos) in &positions {
            self.view.set_position(key, pos);
        }
        self.physics.seed(positions);
        self.physics.halt();
        self.generation += 1;
    }

    /// Build an canvas over `graph`: its [`build_simulation`], the node-children
    /// pool, and a default camera. Shared by [`new`](Canvas::new) (empty),
    /// [`with_sample_graph`](Canvas::with_sample_graph), and
    /// [`with_graph`](Canvas::with_graph).
    pub(crate) fn from_graph(graph: Graph) -> Self {
        let sim = build_simulation(&graph);
        let view = sim.view();
        let physics = Physics::inline(sim, SETTLE_TICKS);
        let (node_dom, gnode_of, stage_node) = build_pool_dom(&graph);
        let node_document = genet_livery::LiveryDocument::new(
            node_dom,
            genet_livery::StyleSet::cambium(&crate::build::NODE_SHEET),
            genet_livery::Device::screen(1.0, 1.0),
        );
        Self {
            graph,
            physics,
            physics_paused: false,
            view,
            node_document,
            gnode_of,
            stage_node,
            camera: Camera::default(),
            style: dark_scene_style(),
            backdrop: surface_bg(),
            generation: 0,
            cursor: (0.0, 0.0),
            pan_velocity: (0.0, 0.0),
            middle_drag: None,
            orbit_drag: None,
            drag: None,
            pinned_nodes: HashSet::new(),
            field_drag: None,
            selected: HashSet::new(),
            selected_edges: HashSet::new(),
            hidden_edges: HashSet::new(),
            active_field: None,
            hidden_fields: HashSet::new(),
            node_states: HashMap::new(),
            node_shapes: HashMap::new(),
            node_faces: HashMap::new(),
            derived_face_palette: DerivedFacePalette::default(),
            derived_face_cache: HashMap::new(),
            node_sizes: HashMap::new(),
            node_sprites: HashMap::new(),
            node_sprite_hulls: HashMap::new(),
            scene_sprite_textures: HashMap::new(),
            resolved_images: resolved_image_cache::ResolvedImageCache::default(),
            pending_image_requests: BTreeMap::new(),
            requested_images: HashSet::new(),
            ambient: None,
            ambient_tincture: ColorF::new(0.0, 0.0, 0.0, 0.0),
            node_materials: HashMap::new(),
            size_by_degree: false,
            size_by_importance: false,
            size_by_recency: false,
            node_recency: HashMap::new(),
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
            content_affinity: None,
            content_affinity_dirty: false,
            affinity_force_installed: false,
            affinity_blend: AffinityBlend::default(),
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
            projection_score: None,
            projection_representations: HashMap::new(),
            arrangement_pull: seiche::DEFAULT_ANCHOR_STIFFNESS,
            physics_law: crate::PhysicsLaw::Springs,
            physics_overlays: Vec::new(),
            physics_kind_source: crate::PhysicsKindSource::Site,
            physics_mass_source: crate::PhysicsMassSource::Degree,
            physics_depth_source: crate::PhysicsDepthSource::Roots,
            restored_score_hold: None,
            scope: None,
            fold: None,
            fold_press: None,
            fold_undo: Vec::new(),
            fold_redo: Vec::new(),
            render_gnodes_as_dom: false,
        }
    }

    /// Set the current app-launch session number (Alembic B5). The host calls this
    /// once per pooled canvas, right after construction, with its persisted-and-
    /// incremented persona session counter; every in-place navigation afterwards
    /// stamps the visited node's `last_session_visited` for
    /// `EvictionPolicy::KeepSessions`.
    pub fn set_current_session(&mut self, session: u64) {
        self.graph.set_current_session(session);
    }

    /// Set the viewport the canvas culls + centers against. The host calls this on
    /// a surface resize; the next [`frame`](Canvas::frame) rebuilds the node-pool
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

    /// Frame the graph itself: the fit-to-`content_bounds` camera that
    /// [`recenter`](Self::recenter) documents as its replacement for a hosted
    /// graph. Restored sessions need this rather than `recenter` — persisted
    /// positions may have settled anywhere in world space, so framing the
    /// origin can frame empty ground with every node off-screen. Zoom fits
    /// the padded bounds but never exceeds `1.0` (a lone node reads at its
    /// natural size, not blown up to fill the window). An empty graph (or one
    /// with no finite positions) falls back to `recenter`.
    pub fn fit_to_content(&mut self) {
        let mut min = (f32::INFINITY, f32::INFINITY);
        let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        for (key, _node) in self.graph.nodes() {
            let Some(p) = self.view.position_of(key) else {
                continue;
            };
            if !p.x.is_finite() || !p.y.is_finite() {
                continue;
            }
            any = true;
            min = (min.0.min(p.x), min.1.min(p.y));
            max = (max.0.max(p.x), max.1.max(p.y));
        }
        if !any {
            self.recenter();
            return;
        }
        // Pad the bounds so rim nodes draw fully inside the viewport (a node's
        // disc + caption extend past its position point).
        const MARGIN: f32 = 160.0;
        let (w, h) = (self.view_w as f32, self.view_h as f32);
        let span_w = (max.0 - min.0) + MARGIN * 2.0;
        let span_h = (max.1 - min.1) + MARGIN * 2.0;
        let zoom = (w / span_w)
            .min(h / span_h)
            .min(1.0)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        let center = ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0);
        self.camera.zoom = zoom;
        self.camera.offset = (w / 2.0 - center.0 * zoom, h / 2.0 - center.1 * zoom);
    }

    /// Whether the graph is empty, or at least one node lies within the current
    /// viewport. `false` means every node is off-screen — a degenerate camera (a
    /// restored pan/zoom that no longer frames the graph), which the host recovers
    /// with [`recenter`](Self::recenter).
    pub fn graph_visible(&self) -> bool {
        self.graph.nodes().next().is_none()
            || self
                .view
                .cull_aabb(self.world_viewport())
                .into_iter()
                .next()
                .is_some()
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
        // New bodies spawn at the origin (positions are no longer graph truth, S2);
        // `sync_nodes` leaves existing bodies at their simulated position, so a live
        // node is not teleported by a topology change.
        let nodes: Vec<(NodeKey, Point2D<f32>)> = self
            .graph
            .nodes()
            .map(|(key, _node)| (key, Point2D::zero()))
            .collect();
        self.physics.sync_nodes(nodes);
        self.physics
            .sync_edges(visible_relation_edges(&self.graph, &self.hidden_edges));
        self.pinned_nodes
            .retain(|key| self.graph.get_node(*key).is_some());
        // Re-resolve field couplings against the new node set, so a field gathers
        // nodes added after it was placed (its targets snapshot at build time).
        // (Field regions — rebuild-on-mutation / new-node capture.)
        self.rebuild_coupling_forces();
        // A law or overlay that snapshots graph structure (Stress's hop distances,
        // Orbit's masses, Kinds' kinds, the group / depth overlays) is rebuilt
        // against the new topology the same way; the others keep their state.
        // (Physics catalog — P1.)
        if self.physics_forces_are_graph_bound() {
            self.rebuild_law_forces();
        }
        let (node_dom, gnode_of, stage_node) = build_pool_dom(&self.graph);
        self.node_document = genet_livery::LiveryDocument::new(
            node_dom,
            genet_livery::StyleSet::cambium(&crate::build::NODE_SHEET),
            genet_livery::Device::screen(1.0, 1.0),
        );
        self.gnode_of = gnode_of;
        self.stage_node = stage_node;
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
        // Recency likewise needs all nodes to normalize; recompute unconditionally when on (cheap,
        // and a visit's fresh timestamp must read current). (Projection proofs — P3.)
        if self.size_by_recency {
            self.recompute_recency();
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
        let colliders: Vec<(NodeKey, seiche::NodeCollider)> = self
            .graph
            .nodes()
            .map(|(key, _)| (key, self.node_collider(key)))
            .collect();
        self.physics.set_node_colliders(colliders);
    }

    /// The collider shape for a node: the node's **body**. A node with a custom hull (the
    /// Body axis: traced from a sprite or hand-authored in the shape editor) collides at that
    /// hull, scaled from face-normalized coords to the current [`node_size`](Self::node_size);
    /// otherwise its content silhouette ([`node_shape`](Self::node_shape)) at its footprint.
    /// The hull applies **regardless of the face** ([`node_face`](Self::node_face)), so a node
    /// can wear a favicon over a custom-shaped body. So physics tracks the body, not just a
    /// bounding box. (Node body & face — the Body axis.)
    pub(crate) fn node_collider(&self, key: NodeKey) -> seiche::NodeCollider {
        let size = self.node_size(key);
        let half = size / 2.0;
        // A custom hull (sprite-traced or hand-authored) collides at that hull, scaled to the
        // face. Independent of the face texture — the body is its own axis.
        if let Some(hull) = self.node_sprite_hulls.get(&key).filter(|h| h.len() >= 3) {
            let points = hull
                .iter()
                .map(|&(nx, ny)| (nx * size, ny * size))
                .collect();
            return seiche::NodeCollider::Hull {
                points,
                fallback: half,
            };
        }
        match self.node_shape(key) {
            NodeShape::Circle => seiche::NodeCollider::Ball { radius: half },
            NodeShape::Square => seiche::NodeCollider::Square { half },
            NodeShape::Rounded => seiche::NodeCollider::RoundedSquare {
                half,
                border: half * 0.3,
            },
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
            .map(|(key, _node)| {
                // A node the view has not placed yet (just added) parks at the origin;
                // positions are no longer graph truth (S2), and a following seed / the
                // actor snapshot supplies its real position.
                let pos = self.view.position_of(key).unwrap_or_else(Point2D::zero);
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
    /// canvas keeps ticking in-thread (tests; the no-threads wasm profile).
    ///
    /// [`Wake`]: armillary::Wake
    pub fn offload_physics(&mut self, wake: armillary::Wake) {
        // Capture the host's wake as the canvas's off-thread wake: the community lane reuses it to
        // poke the same loop when its worker result lands, so it needs no separate host wiring.
        // `Some` here is also the "native + offloaded" signal that gates community off-thread.
        self.offthread_wake = Some(wake.clone());
        self.physics.offload(wake);
    }

    /// Park this canvas's physics: stop any in-progress settle so a backgrounded
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
    /// canvas-host stays free of the linked-data bridge: the host passes the merge
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
                (
                    k,
                    Point2D::new(anchor.x + 12.0 + col * 16.0, anchor.y + 12.0 + row * 16.0),
                )
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
    pub fn set_node_favicon(&mut self, url: &str, image: kernel::types::ImageRef) -> bool {
        let Some(key) = self.graph.get_node_by_url(url).map(|(k, _)| k) else {
            return false;
        };
        self.favicon_at_key(key, image)
    }

    /// [`set_node_favicon`](Self::set_node_favicon) keyed by the node's stable
    /// member id instead of its URL. URL keying answers first-match when two
    /// nodes share an address; a host correlating fetches by node id stamps
    /// the exact requester (correlation-over-URLs).
    pub fn set_node_favicon_for(
        &mut self,
        member: uuid::Uuid,
        image: kernel::types::ImageRef,
    ) -> bool {
        let Some(key) = self.graph.get_node_key_by_id(member) else {
            return false;
        };
        self.favicon_at_key(key, image)
    }

    fn favicon_at_key(&mut self, key: NodeKey, image: kernel::types::ImageRef) -> bool {
        matches!(
            kernel::graph::apply::apply_graph_delta(
                &mut self.graph,
                kernel::graph::apply::GraphDelta::SetNodeImage {
                    key,
                    role: kernel::types::ImageRole::Favicon,
                    image,
                },
            ),
            kernel::graph::apply::GraphDeltaResult::NodeMetadataUpdated(true)
        )
    }

    /// Stamp a fetched page `<title>` onto the node currently at `url`, if one
    /// exists and the title actually changes. Unlike the favicon stamp below,
    /// the caption is static DOM text (built once in `build_pool_dom`), so a
    /// changed title rebuilds the node-children pool via `reconcile_derived` —
    /// positions and selection are kept, no re-settle. (Fetch enrichment: the
    /// display label prefers a real title over the host fallback.)
    pub fn set_node_title(&mut self, url: &str, title: String) -> bool {
        let Some(key) = self.graph.get_node_by_url(url).map(|(k, _)| k) else {
            return false;
        };
        self.title_at_key(key, title)
    }

    /// [`set_node_title`](Self::set_node_title) keyed by member id (see
    /// [`set_node_favicon_for`](Self::set_node_favicon_for) for why).
    pub fn set_node_title_for(&mut self, member: uuid::Uuid, title: String) -> bool {
        let Some(key) = self.graph.get_node_key_by_id(member) else {
            return false;
        };
        self.title_at_key(key, title)
    }

    /// Write a node's body text, keyed by member id.
    ///
    /// Routes through the delta spine, so a body written by a behavior
    /// journals attributed to it and reads back in the node's history under
    /// its name rather than the user's.
    /// `None` clears the body, matching the kernel delta's own shape.
    pub fn set_node_body_for(&mut self, member: uuid::Uuid, body: Option<String>) -> bool {
        let Some(key) = self.graph.get_node_key_by_id(member) else {
            return false;
        };
        matches!(
            kernel::graph::apply::apply_graph_delta(
                &mut self.graph,
                kernel::graph::apply::GraphDelta::SetNodeBody { key, body },
            ),
            kernel::graph::apply::GraphDeltaResult::NodeMetadataUpdated(true)
        )
    }

    /// Set or clear the borne graph (`Node.nested`) on a member, keyed by
    /// member id — structural containment per the one-node ruling. Routes
    /// through the delta spine so the change journals attributed
    /// (`ReplaySetNodeNestedById`).
    pub fn set_node_nested_for(
        &mut self,
        member: uuid::Uuid,
        nested: Option<kernel::graph::LogId>,
    ) -> bool {
        let Some(key) = self.graph.get_node_key_by_id(member) else {
            return false;
        };
        matches!(
            kernel::graph::apply::apply_graph_delta(
                &mut self.graph,
                kernel::graph::apply::GraphDelta::SetNodeNested { key, nested },
            ),
            kernel::graph::apply::GraphDeltaResult::NodeMetadataUpdated(true)
        )
    }

    fn title_at_key(&mut self, key: NodeKey, title: String) -> bool {
        let updated = matches!(
            kernel::graph::apply::apply_graph_delta(
                &mut self.graph,
                kernel::graph::apply::GraphDelta::SetNodeTitle { key, title },
            ),
            kernel::graph::apply::GraphDeltaResult::NodeMetadataUpdated(true)
        );
        if updated {
            self.reconcile_derived();
        }
        updated
    }

    /// Stamp a more precise MIME hint (e.g. a fetch's Content-Type) onto the
    /// node currently at `url`. Metadata-only, like the favicon stamp: no
    /// reconcile, no layout disturbance. (Fetch enrichment.)
    pub fn set_node_mime_hint(&mut self, url: &str, mime_hint: Option<String>) -> bool {
        let Some(key) = self.graph.get_node_by_url(url).map(|(k, _)| k) else {
            return false;
        };
        self.mime_hint_at_key(key, mime_hint)
    }

    /// [`set_node_mime_hint`](Self::set_node_mime_hint) keyed by member id
    /// (see [`set_node_favicon_for`](Self::set_node_favicon_for) for why).
    pub fn set_node_mime_hint_for(
        &mut self,
        member: uuid::Uuid,
        mime_hint: Option<String>,
    ) -> bool {
        let Some(key) = self.graph.get_node_key_by_id(member) else {
            return false;
        };
        self.mime_hint_at_key(key, mime_hint)
    }

    fn mime_hint_at_key(&mut self, key: NodeKey, mime_hint: Option<String>) -> bool {
        matches!(
            kernel::graph::apply::apply_graph_delta(
                &mut self.graph,
                kernel::graph::apply::GraphDelta::SetNodeMimeHint { key, mime_hint },
            ),
            kernel::graph::apply::GraphDeltaResult::NodeMetadataUpdated(true)
        )
    }

    /// Attach or clear a content-addressed representation on `member`.
    /// Bytes are deposited by the host before this call; Canvas carries and
    /// journals only their muniment address.
    pub fn set_node_content_for(
        &mut self,
        member: uuid::Uuid,
        content: Option<kernel::graph::ContentHash>,
    ) -> bool {
        let Some(key) = self.graph.get_node_key_by_id(member) else {
            return false;
        };
        matches!(
            kernel::graph::apply::apply_graph_delta(
                &mut self.graph,
                kernel::graph::apply::GraphDelta::SetNodeContent { key, content },
            ),
            kernel::graph::apply::GraphDeltaResult::NodeMetadataUpdated(true)
        )
    }

    /// Stamp a preview thumbnail PNG onto the node `member`, if it exists. This mirrors
    /// [`set_node_favicon`](Self::set_node_favicon): metadata-only, no reconcile, no layout
    /// disturbance. Used when the host already rendered a snapshot preview and wants to persist
    /// that exact image onto the node rather than keeping it only in a window-local cache.
    pub fn set_node_thumbnail(
        &mut self,
        member: uuid::Uuid,
        image: kernel::types::ImageRef,
    ) -> bool {
        let Some((key, _)) = self.graph.get_node_by_id(member) else {
            return false;
        };
        matches!(
            kernel::graph::apply::apply_graph_delta(
                &mut self.graph,
                kernel::graph::apply::GraphDelta::SetNodeImage {
                    key,
                    role: kernel::types::ImageRole::Preview,
                    image,
                },
            ),
            kernel::graph::apply::GraphDeltaResult::NodeMetadataUpdated(true)
        )
    }
}
