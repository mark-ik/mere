/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Cartography geometry + per-node attribute reads/writes (color, shape, face, sprite, material, size, importance, height).

use super::*;

impl Canvas {
    /// The Cartography projection geometry: each member's current world position,
    /// member-keyed — the canvas's settled layout, for the host to persist as the
    /// cartography sidecar (the counterpart of the workbench's `TreeGeometry`). Reads
    /// the live seiche positions, so it captures whatever is shown (force-directed or a
    /// picked layout strategy). (Position sidecar.)
    pub fn cartography_geometry(&self) -> crate::geometry::CartographyGeometry {
        crate::geometry::CartographyGeometry::from_positions(
            self.view
                .positions()
                .filter_map(|(key, p)| self.graph.get_node(key).map(|node| (node.id, (p.x, p.y)))),
        )
        // Persist the deliberate per-node size overrides + the size-by-degree scene flag
        // alongside the positions, so a sized graph re-opens sized. Only explicit overrides
        // travel; degree-derived sizes recompute from the flag. (Node-rep — size persistence.)
        .with_sizes(
            self.node_sizes
                .iter()
                .filter_map(|(&key, &size)| self.graph.get_node(key).map(|node| (node.id, size))),
        )
        .with_size_by_degree(self.size_by_degree)
        .with_size_by_importance(self.size_by_importance)
        .with_importance_metric(self.importance_metric.as_code())
        // Persist the custom sprite faces (the imported image as a data-URI) so a node textured
        // with one re-opens textured, not reverted to its default face. (Node-rep — sprite persistence.)
        .with_sprites(
            self.node_sprites.iter().filter_map(|(&key, uri)| {
                self.graph.get_node(key).map(|node| (node.id, uri.clone()))
            }),
        )
        // Persist each sprite's collider hull alongside it, so the traced-to-image collider
        // survives a reload without re-decoding the image. (Node-rep — sprite hull persistence.)
        .with_sprite_hulls(self.node_sprite_hulls.iter().filter_map(|(&key, hull)| {
            self.graph.get_node(key).map(|node| (node.id, hull.clone()))
        }))
        // Persist the per-node physical material overrides (restitution / friction / density),
        // so a node tuned heavier / bouncier re-opens that way. (Node body & face — material.)
        .with_materials(self.node_materials.iter().filter_map(|(&key, mat)| {
            self.graph
                .get_node(key)
                .map(|node| (node.id, (mat.restitution, mat.friction, mat.density)))
        }))
        // Persist the per-node face overrides (favicon / sprite / bare), so a node's chosen
        // texture re-opens that way (the body persists separately). (Node body & face — face.)
        .with_faces(self.node_faces.iter().filter_map(|(&key, &face)| {
            self.graph
                .get_node(key)
                .map(|node| (node.id, face.as_code().to_string()))
        }))
    }

    /// One node's current world position (the live seiche position), so the host can
    /// draw a switcher thumbnail from the canvas rather than the now position-free
    /// graph. `None` for a node the view has not placed. (Position gut.)
    pub fn node_position(&self, key: NodeKey) -> Option<PortablePoint> {
        self.view
            .position_of(key)
            .map(|p| PortablePoint::new(p.x, p.y))
    }

    /// Write the live physics positions back into the graph's node records (their
    /// projected position), so a clone or snapshot of the graph reflects the current
    /// on-screen layout rather than the spawn seed. The graph's own node positions are
    /// only the initial seed; physics owns the live layout in `view`. A tear-out
    /// **fork** calls this on the donor before cloning, so the fork opens with the
    /// donor's layout instead of every node piled at the seed. (Tear-out gestures G4.)
    pub fn commit_positions_to_graph(&mut self) {
        // No-op since S2: positions are no longer graph truth, so there is nothing
        // to write back into the graph. The live layout is `view` (seiche); the
        // durable layout is the cartography sidecar. A tear-out fork carries the
        // donor's layout by copying the sidecar, not the graph. (Kept as a seam so
        // callers need not change; retire when the fork path reads the sidecar.)
    }

    /// A node's render color, matching the in-scene gnode's class palette: orange when
    /// selected, else green open / red closed / blue idle. The host colors the canvas
    /// element's chrome-DOM gnodes with it so they carry node identity without the
    /// in-scene gnode layer. (Canvas-as-element — Phase 2.)
    pub fn node_color(&self, key: NodeKey) -> &'static str {
        if self.selected.contains(&key) {
            return "#f7a440";
        }
        match self.node_states.get(&key) {
            Some(NodeState::Open) => "#5fb878",
            Some(NodeState::Closed) => "#cc5a54",
            _ => "#5a8fc8",
        }
    }

    /// A node's activation-state color WITHOUT the selection override — green open /
    /// red closed / blue idle. The chrome-DOM gnode colors its face with this and shows
    /// selection as a ring + lift instead, so the color channel stays free to carry
    /// activation state. The in-scene gnode path still uses [`node_color`](Self::node_color).
    pub fn node_state_color(&self, key: NodeKey) -> &'static str {
        match self.node_states.get(&key) {
            Some(NodeState::Open) => "#5fb878",
            Some(NodeState::Closed) => "#cc5a54",
            _ => "#5a8fc8",
        }
    }

    /// Whether `key` is in the current selection set, so a node's representation can show
    /// selection through geometry (a ring + lift) rather than recoloring its face.
    pub fn node_selected(&self, key: NodeKey) -> bool {
        self.selected.contains(&key)
    }

    /// A node's content-type silhouette (square document / rounded menu / circle feed),
    /// for the gnode to shape its face. `Square` (the default) for an unmapped node.
    pub fn node_shape(&self, key: NodeKey) -> NodeShape {
        self.node_shapes.get(&key).copied().unwrap_or_default()
    }

    /// A node's **face** (the texture on its body): a per-node override if the user set one
    /// (via [`set_node_face`](Self::set_node_face) or [`set_node_sprite`](Self::set_node_sprite)),
    /// otherwise the default [`Favicon`](Face::Favicon). Independent of the body (the collider
    /// hull). A future scene pane (Decision 6) can diversify the default by content type.
    /// (Node body & face — the Face axis.)
    pub fn node_face(&self, key: NodeKey) -> Face {
        self.node_faces.get(&key).copied().unwrap_or_default()
    }

    /// Render the on-screen gnodes as host chrome-DOM elements instead of in-scene Scene
    /// layers: the next [`frame`](Canvas::frame) drops the gnode + favicon layers, keeping
    /// edges + demoted dots as the underlay. The host sets this on the focused canvas
    /// (whose gnodes it snapshots into the shell document) and leaves it off on secondary
    /// panes. Either way a gnode is the node's rendered body, never the node's referenced
    /// document. (Canvas-as-element.)
    pub fn set_render_gnodes_as_dom(&mut self, on: bool) {
        self.render_gnodes_as_dom = on;
    }

    /// Seed node positions from a persisted cartography sidecar, overriding the graph's
    /// load-time seed so a reloaded session shows its settled layout rather than
    /// re-scrambling. Members absent from the sidecar keep their existing seed (a node
    /// added since the last save still shows); physics halts so the restored layout
    /// holds until the user nudges it. (Position sidecar.)
    pub fn seed_cartography(
        &mut self,
        positions: impl IntoIterator<Item = (uuid::Uuid, (f32, f32))>,
    ) {
        let resolved: Vec<(NodeKey, Point2D<f32>)> = positions
            .into_iter()
            .filter_map(|(id, (x, y))| {
                self.graph
                    .get_node_by_id(id)
                    .map(|(key, _)| (key, Point2D::new(x, y)))
            })
            .collect();
        if resolved.is_empty() {
            return;
        }
        for &(key, pos) in &resolved {
            self.view.set_position(key, pos);
        }
        self.physics.seed(resolved);
        self.physics.halt();
    }

    /// Restore the per-node size overrides + the size-by-degree scene flag from a persisted
    /// cartography sidecar (the sizing counterpart of [`seed_cartography`]). Pushes the
    /// resulting collider/pick radii to the view + bodies but does **not** settle, so the
    /// restored layout holds (the position seed already halted physics). Sizes are clamped
    /// like [`set_node_size`]; an unknown member id is skipped. (Node-rep — size persistence.)
    pub fn apply_cartography_sizing(
        &mut self,
        sizes: impl IntoIterator<Item = (uuid::Uuid, f32)>,
        size_by_degree: bool,
        size_by_importance: bool,
    ) {
        self.size_by_degree = size_by_degree;
        // Restore size-by-importance before `push_node_geometry` below, and force the cache stale
        // so the push actually recomputes: on a reused canvas (a session switch) `importance_dirty`
        // may already be clean, which would otherwise leave the restored mode with an empty map and
        // every node at the default size. (Graph signals — restore the importance encoding.)
        self.size_by_importance = size_by_importance;
        if size_by_importance {
            self.importance_dirty = true;
        }
        for (id, size) in sizes {
            if let Some((key, _)) = self.graph.get_node_by_id(id) {
                self.node_sizes.insert(key, size.clamp(16.0, 160.0));
            }
        }
        self.push_node_geometry();
    }

    /// Restore the per-node sprite faces from a persisted cartography sidecar (the sprite
    /// counterpart of [`apply_cartography_sizing`]). Each `(member, data-URI)` is applied via
    /// [`set_node_sprite`], so the node re-opens textured and back at [`Face::Sprite`].
    /// An unknown member id is skipped; does not settle. (Node-rep — sprite persistence.)
    pub fn apply_cartography_sprites<'a>(
        &mut self,
        sprites: impl IntoIterator<Item = (uuid::Uuid, &'a str)>,
    ) {
        for (id, uri) in sprites {
            self.set_node_sprite(id, uri.to_string());
        }
    }

    /// Restore the per-node sprite collider hulls from a persisted cartography sidecar (the
    /// companion of [`apply_cartography_sprites`] — call it after, once the sprites exist).
    /// Each `(member, hull)` is set via [`set_node_sprite_hull`], so the node re-opens
    /// colliding at its traced outline. An unknown member id is skipped; does not settle.
    /// (Node-rep — sprite hull persistence.)
    pub fn apply_cartography_sprite_hulls(
        &mut self,
        hulls: impl IntoIterator<Item = (uuid::Uuid, Vec<(f32, f32)>)>,
    ) {
        for (id, hull) in hulls {
            self.set_node_sprite_hull(id, hull);
        }
        self.push_node_geometry();
    }

    /// Restore the per-node physical materials from a persisted cartography sidecar. Each
    /// `(member, (restitution, friction, density))` is applied via [`set_node_material`], so a
    /// node re-opens with its tuned weight / bounce / grip. An unknown member id is skipped.
    /// (Node body & face — material persistence.)
    pub fn apply_cartography_materials(
        &mut self,
        materials: impl IntoIterator<Item = (uuid::Uuid, (f32, f32, f32))>,
    ) {
        for (id, (restitution, friction, density)) in materials {
            self.set_node_material(
                id,
                seiche::NodeMaterial {
                    restitution,
                    friction,
                    density,
                },
            );
        }
    }

    /// Restore the per-node face overrides from a persisted cartography sidecar (call it AFTER
    /// [`apply_cartography_sprites`], so a node the user switched off its sprite face re-opens on
    /// the chosen face rather than back on `Sprite`). Each `(member, code)` is applied via
    /// [`set_node_face`]. An unknown member id is skipped. (Node body & face — face persistence.)
    pub fn apply_cartography_faces<'a>(
        &mut self,
        faces: impl IntoIterator<Item = (uuid::Uuid, &'a str)>,
    ) {
        for (id, code) in faces {
            self.set_node_face(id, Face::from_code(code));
        }
    }

    /// Theme the canvas's surfaces: the content-surface `backdrop` and the `edge`
    /// stroke color, as straight `[r, g, b, a]` (0..1). The host pushes these from
    /// the active theme so the graph re-themes with the chrome. Node *state* colors
    /// (open / closed / idle / selected) stay semantic and are not rethemed here.
    pub fn set_palette(&mut self, backdrop: [f32; 4], edge: [f32; 4]) {
        self.backdrop = ColorF::new(backdrop[0], backdrop[1], backdrop[2], backdrop[3]);
        self.style.edge_color = ColorF::new(edge[0], edge[1], edge[2], edge[3]);
    }

    /// Set the per-node content silhouettes the canvas shapes its on-screen nodes
    /// by, keyed by node UUID; the canvas resolves each to its `NodeKey`. The host
    /// recomputes + pushes this from each node's content type as content is
    /// fetched; a node absent from `shapes` draws as [`NodeShape::Square`].
    pub fn set_node_shapes(&mut self, shapes: HashMap<uuid::Uuid, NodeShape>) {
        self.node_shapes = shapes
            .into_iter()
            .filter_map(|(id, shape)| self.graph.get_node_by_id(id).map(|(key, _)| (key, shape)))
            .collect();
        // The collider follows the silhouette, so a content-type shape change reshapes the
        // physics body too, not just the drawn face. (Node-rep — collider matches shape.)
        self.push_node_geometry();
    }

    /// Override a single node's **face** (the texture on its body), keyed by node UUID. Wins
    /// over the default [`Favicon`](Face::Favicon) until [`clear_node_face`](Self::clear_node_face).
    /// Sets only the face: the body (the collider hull) and any stored sprite image are left
    /// intact, so a face switch never reshapes the node or discards an imported sprite. The
    /// override is held on the canvas; persisting it is a follow-up (it joins the cartography
    /// sidecar). A no-op for an unknown id. (Node body & face — the Face axis.)
    pub fn set_node_face(&mut self, id: uuid::Uuid, face: Face) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_faces.insert(key, face);
        }
    }

    /// Clear a node's per-node face override, reverting it to the default
    /// [`Favicon`](Face::Favicon). Leaves the body and any sprite image intact. Keyed by node
    /// UUID; a no-op for an unknown id. (Node body & face — the Face axis.)
    pub fn clear_node_face(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_faces.remove(&key);
        }
    }

    /// Reset a node's **body** to its content-type silhouette: drop any custom hull (sprite-traced
    /// or hand-authored), so the collider falls back to the silhouette primitive. Leaves the face
    /// untouched. Pushes the new geometry to physics. Keyed by node UUID; a no-op for an unknown
    /// id. (Node body & face — the Body axis.)
    pub fn clear_node_body(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            if self.node_sprite_hulls.remove(&key).is_some() {
                self.push_node_geometry();
            }
        }
    }

    /// Give a node a custom sprite face: store the imported image (a PNG data-URI) and set its
    /// face to [`Sprite`](Face::Sprite) in one step. Keyed by node UUID; a no-op for an unknown
    /// id. Held on the canvas and persisted in the cartography sidecar. The host follows with
    /// [`set_node_sprite_hull`] to trace the body hull; this backs the drag-and-drop image
    /// import. (Node body & face — sprite face.)
    pub fn set_node_sprite(&mut self, id: uuid::Uuid, data_uri: String) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_sprites.insert(key, data_uri);
            self.node_faces.insert(key, Face::Sprite);
        }
    }

    /// Remove a node's stored sprite image; if its face was [`Sprite`](Face::Sprite), revert the
    /// face to [`Favicon`](Face::Favicon). Leaves the body (the collider hull) intact, since the
    /// hull is the node's own shape once traced. Keyed by node UUID; a no-op for an unknown id.
    /// (Node body & face — sprite face.)
    pub fn clear_node_sprite(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_sprites.remove(&key);
            if self.node_faces.get(&key) == Some(&Face::Sprite) {
                self.node_faces.insert(key, Face::Favicon);
            }
        }
    }

    /// A node's custom sprite face (a PNG data-URI), if it has one. The gnode uses this as
    /// the face image when [`node_face`](Self::node_face) is [`Sprite`](Face::Sprite).
    /// (Node body & face — sprite face.)
    pub fn node_sprite(&self, key: NodeKey) -> Option<&str> {
        self.node_sprites.get(&key).map(String::as_str)
    }

    /// Set a node's sprite collider hull: the sprite's opaque region as a convex polygon in
    /// face-normalized coords ([-0.5, 0.5], scaled to `node_size` when the collider is built).
    /// The host traces it from the image at import. A hull of fewer than 3 points clears it
    /// (the node falls back to its silhouette collider). Pushes the new geometry to physics.
    /// Keyed by node UUID; a no-op for an unknown id. (Node representation P2 — sprite hull.)
    pub fn set_node_sprite_hull(&mut self, id: uuid::Uuid, hull: Vec<(f32, f32)>) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            if hull.len() >= 3 {
                self.node_sprite_hulls.insert(key, hull);
            } else {
                self.node_sprite_hulls.remove(&key);
            }
            self.push_node_geometry();
        }
    }

    /// A node's sprite collider hull (face-normalized convex polygon), if it has one.
    /// (Node representation P2 — sprite hull.)
    pub fn node_sprite_hull(&self, key: NodeKey) -> Option<&[(f32, f32)]> {
        self.node_sprite_hulls.get(&key).map(Vec::as_slice)
    }

    /// Seed a node with a default editable body hull — a square covering most of the face — so a
    /// node with no sprite can be given a custom shape from scratch in the editor (authoring a
    /// body beyond tracing a sprite). A no-op if it already has a hull. Pushes geometry to
    /// physics. Keyed by node UUID; a no-op for an unknown id. (Node body & face — the Body axis.)
    pub fn seed_node_body(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            if self.node_sprite_hulls.contains_key(&key) {
                return;
            }
        }
        self.set_node_sprite_hull(
            id,
            vec![(-0.35, -0.35), (0.35, -0.35), (0.35, 0.35), (-0.35, 0.35)],
        );
    }

    /// A node's physical material (the Body axis): its per-node override if set, else the
    /// default [`seiche::NodeMaterial`] (the spawn restitution / friction / density).
    /// (Node body & face — material.)
    pub fn node_material(&self, key: NodeKey) -> seiche::NodeMaterial {
        self.node_materials.get(&key).copied().unwrap_or_default()
    }

    /// Set a node's physical material (restitution / friction / density) and push it to the
    /// live body, so the node feels heavier / bouncier / grippier at once. A material equal to
    /// the default is stored too (an explicit override), so the facet shows it as set. Keyed by
    /// node UUID; a no-op for an unknown id. Persisted in the cartography sidecar. (Node body
    /// & face — material.)
    pub fn set_node_material(&mut self, id: uuid::Uuid, material: seiche::NodeMaterial) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_materials.insert(key, material);
            self.physics.set_node_materials(vec![(key, material)]);
        }
    }

    /// Reset a node's physical material to the default (drop its override) and push the default
    /// back to the live body. Keyed by node UUID; a no-op for an unknown id. (Node body & face.)
    pub fn clear_node_material(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            if self.node_materials.remove(&key).is_some() {
                self.physics
                    .set_node_materials(vec![(key, seiche::NodeMaterial::default())]);
            }
        }
    }

    /// A node's face footprint (px): a per-node override if set, else size-by-degree
    /// (the face grows with the node's undirected degree, capped) when that mode is on,
    /// else the uniform default. The gnode applies the selection lift on top, and uses the
    /// same value to center the face on the seiche collider. (P0 resize.)
    pub fn node_size(&self, key: NodeKey) -> f32 {
        const DEFAULT: f32 = 36.0;
        const MAX: f32 = 88.0;
        if let Some(&s) = self.node_sizes.get(&key) {
            return s;
        }
        // Size by importance (the signal-driven channel) wins over size-by-degree: normalized
        // importance (0..=1) maps to DEFAULT..=MAX, so the most important node hits the cap and
        // the rest scale relative to it. An un-scored node reads 0 -> DEFAULT. (Graph signals.)
        if self.size_by_importance {
            let importance = self.node_importance.get(&key).copied().unwrap_or(0.0);
            return DEFAULT + importance * (MAX - DEFAULT);
        }
        if self.size_by_degree {
            let degree = self.graph.neighbors_undirected(key).count();
            return (DEFAULT + 8.0 * degree as f32).min(MAX);
        }
        DEFAULT
    }

    /// Override a single node's face footprint (px), keyed by node UUID; clamped to a sane
    /// range. Wins over size-by-degree / the default until [`clear_node_size`]; a no-op for
    /// an unknown id. (P0 resize.)
    pub fn set_node_size(&mut self, id: uuid::Uuid, size: f32) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_sizes.insert(key, size.clamp(16.0, 160.0));
            self.resettle_for_size();
        }
    }

    /// Clear a node's footprint override, reverting it to size-by-degree / the default.
    /// Keyed by node UUID; a no-op for an unknown id. (P0 resize.)
    pub fn clear_node_size(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_sizes.remove(&key);
            self.resettle_for_size();
        }
    }

    /// Toggle size-by-degree: when on, node faces grow with their undirected degree. A
    /// scene-level presentation choice (the user's opt-in, off by default). (P0 resize.)
    pub fn set_size_by_degree(&mut self, on: bool) {
        self.size_by_degree = on;
        self.resettle_for_size();
    }

    /// Whether size-by-degree is on. (P0 resize.)
    pub fn size_by_degree(&self) -> bool {
        self.size_by_degree
    }

    /// Toggle **size-by-importance**: when on, node faces grow with their graph-signals
    /// importance (degree-based now, betweenness later), normalized so the most important node
    /// hits the cap. A scene-level choice (off by default); wins over size-by-degree, loses to a
    /// manual per-node override. Refreshes the cached importance + re-separates neighbours.
    /// (Graph signals — importance encoding.)
    pub fn set_size_by_importance(&mut self, on: bool) {
        self.size_by_importance = on;
        if on {
            self.importance_dirty = true; // force a fresh compute on enable
            self.recompute_importance();
        } else {
            // Clear the cache, but mark it dirty (not clean-empty): the **gloss** size-by-importance
            // encoding may still read it, and `recompute_importance` only repopulates a dirty cache —
            // a clean-empty map would silently render every gloss node at the uniform floor factor.
            self.node_importance.clear();
            self.importance_dirty = true;
        }
        self.resettle_for_size();
    }

    /// Whether size-by-importance is on. (Graph signals — importance encoding.)
    pub fn size_by_importance(&self) -> bool {
        self.size_by_importance
    }

    /// Choose the importance metric (degree or betweenness) size-by-importance reads. Dirties the
    /// cache + re-separates so the change takes effect immediately when the mode is on; a no-op
    /// effect when off. (Graph signals — importance metric.)
    pub fn set_importance_metric(&mut self, metric: ImportanceMetric) {
        self.importance_metric = metric;
        self.importance_dirty = true;
        if self.size_by_importance {
            self.recompute_importance();
            self.resettle_for_size();
        }
    }

    /// The active importance metric. (Graph signals — importance metric.)
    pub fn importance_metric(&self) -> ImportanceMetric {
        self.importance_metric
    }

    /// Restore the importance metric from a persisted sidecar code. Call this **before**
    /// [`apply_cartography_sizing`](Self::apply_cartography_sizing), so its size recompute uses the
    /// restored metric: this only records the choice + dirties the cache, leaving the geometry push
    /// to the sizing restore. An empty / unknown code restores degree. (Graph signals — metric
    /// persistence.)
    pub fn apply_cartography_importance_metric(&mut self, code: &str) {
        self.importance_metric = ImportanceMetric::from_code(code);
        self.importance_dirty = true;
    }

    /// Recompute the cached per-node importance from `signals` (degree-based, normalized
    /// `0..=1`). Called when geometry is pushed under size-by-importance; the generation +
    /// dirty-bit cache that gates this is a later graph-signals slice. (Graph signals.)
    pub(crate) fn recompute_importance(&mut self) {
        // The cheap-signal cache: only recompute when the graph topology changed since the last
        // compute (the dirty flag), so a size-only geometry push does not redo the O(N) degree
        // pass. (Graph signals — the cheap-signal cache.)
        if !self.importance_dirty {
            return;
        }
        self.node_importance = signals::importance(&self.graph, self.importance_metric)
            .weights
            .into_iter()
            .collect();
        self.importance_dirty = false;
    }

    /// A node's render height (px above the ground plane) for the isometric float: `0`
    /// (flat) unless height-by-degree is on, where a node rises with its undirected
    /// degree (capped) so hubs stand tallest. Purely visual: it does not move the seiche
    /// body. The gnode is raised in screen-y by this (times zoom) and a stem drops to its
    /// ground anchor, where its edges meet. (Isometric camera P3 — fake height.)
    pub fn node_height(&self, key: NodeKey) -> f32 {
        if !self.height_by_degree {
            return 0.0;
        }
        let degree = self.graph.neighbors_undirected(key).count();
        (16.0 * degree as f32).min(80.0)
    }

    /// Toggle height-by-degree: when on, nodes float above the ground by their degree
    /// (hubs highest), each with a stem to its ground anchor. Off by default; pairs with
    /// the isometric tilt. (Isometric camera P3.)
    pub fn set_height_by_degree(&mut self, on: bool) {
        self.height_by_degree = on;
    }

    /// Whether height-by-degree is on. (Isometric camera P3.)
    pub fn height_by_degree(&self) -> bool {
        self.height_by_degree
    }
}
