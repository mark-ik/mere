// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Physics/ambient scene loaders, size tiers, and focus / member / history queries.

use kernel::graph::apply::{self as graph_apply, GraphDelta, apply_graph_delta};

use super::*;

impl Canvas {
    /// Add a drifting scene-decoration body to the canvas's world — a "living backdrop"
    /// element, intangible to the graph by default (it never perturbs the layout). A ball
    /// of `radius` at world `position`, with an initial `velocity` (px/s). Best called
    /// before [`offload_physics`](Self::offload_physics) so it rides onto the actor with
    /// the rest of the world. (Physics scenes P1.)
    pub fn add_scene_body(&mut self, position: (f32, f32), radius: f32, velocity: (f32, f32)) {
        self.physics.add_scene_body(
            seiche::NodeCollider::Ball { radius },
            Point2D::new(position.0, position.1),
            velocity,
        );
    }

    /// Set every node's tangibility to the scene bodies: `true` lets the graph collide with
    /// the scene (push it / be pushed), `false` (the default) passes through. The scene-wide
    /// tangibility lever; applies to the current nodes. (Physics scenes P2.)
    pub fn set_nodes_tangible(&mut self, tangible: bool) {
        self.physics.set_nodes_tangible(tangible);
    }

    /// Register a scene-prop sprite texture under an opaque `handle`: raw RGBA8 (straight alpha) plus
    /// its pixel dimensions. A scene prop whose [`SceneBodySpec::sprite`](seiche::SceneBodySpec) carries
    /// the same handle then paints as a textured billboard over its footprint (otherwise it falls back
    /// to the abstract orb / polygon). The registry persists across scene loads, so a host registers
    /// its props' textures once at startup. A re-register under the same handle replaces it. (Physics
    /// scenes — scene-prop sprites.)
    /// Supply decoded pixels for one content-addressed image so the paint path
    /// can draw it. The host loads the blob (`image_store::load_image`),
    /// decodes it, and registers it under the reference's digest; nodes
    /// holding that reference then paint. Registering is idempotent.
    pub fn register_resolved_image(
        &mut self,
        digest: [u8; 32],
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) {
        self.resolved_images.insert(digest, (rgba, width, height));
    }

    /// Whether decoded pixels are available for a digest.
    pub fn has_resolved_image(&self, digest: &[u8; 32]) -> bool {
        self.resolved_images.contains_key(digest)
    }

    pub fn register_scene_sprite(
        &mut self,
        handle: impl Into<String>,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) {
        self.scene_sprite_textures
            .insert(handle.into(), (rgba, width, height));
    }

    /// Load Conway's Game of Life as the ambient backdrop: a wrapped grid of cells, seeded with a
    /// random soup, that the frame loop steps a few generations a second and paints behind the graph.
    /// Replaces any prior ambient sim; the canvas keeps redrawing while one is active so it animates.
    /// Clear with [`clear_ambient`](Self::clear_ambient). (Physics scenes P5.)
    pub fn load_game_of_life(&mut self) {
        let sim = GameOfLife::seeded(100, 64, 0x5EED_1234);
        self.ambient_tincture = sim.default_tincture();
        self.ambient = Some(Box::new(sim));
    }

    /// Load the n-body drift as the ambient backdrop: a cloud of bodies orbiting a central well and
    /// tugging on one another, a slow galaxy-like swirl behind the graph. (Physics scenes P5.)
    pub fn load_nbody(&mut self) {
        let sim = NBody::seeded(200, 0x0B17_2024);
        self.ambient_tincture = sim.default_tincture();
        self.ambient = Some(Box::new(sim));
    }

    /// Load particle-life as the ambient backdrop: particles of a few species self-organising under
    /// asymmetric attraction into drifting clusters and chains, each species a hue rotated from the
    /// tincture. (Physics scenes P5.)
    pub fn load_particle_life(&mut self) {
        let sim = ParticleLife::seeded(320, 0x9A17_2024);
        self.ambient_tincture = sim.default_tincture();
        self.ambient = Some(Box::new(sim));
    }

    /// Load falling-sand as the ambient backdrop: grains pour from the top, pile into drifting dunes,
    /// and drain at the bottom - a gravity-shaped CA flowing behind the graph. (Physics scenes P5.)
    pub fn load_sand(&mut self) {
        let sim = SandFall::new(120, 76, 0x5A11_2024);
        self.ambient_tincture = sim.default_tincture();
        self.ambient = Some(Box::new(sim));
    }

    /// Remove the ambient backdrop sim. (Physics scenes P5.)
    pub fn clear_ambient(&mut self) {
        self.ambient = None;
    }

    /// Override the active ambient backdrop's [`Tincture`] (its base paint colour). Takes effect on
    /// the next frame; persists until the next backdrop is loaded (which resets it to that sim's
    /// default). A no-op visually when no backdrop is loaded. (Physics scenes P5 - tincture.)
    pub fn set_ambient_tincture(&mut self, tincture: Tincture) {
        self.ambient_tincture = tincture;
    }

    /// Load a declarative [`SceneSpec`] into the world (clearing any prior scene), then kick
    /// a settle so it falls / arranges into place. A perpetual scene (a drift) then keeps
    /// ticking on its own. The graph is intangible to the scene by default
    /// ([`set_nodes_tangible`] makes it interactive). Pair with the re-exported catalog
    /// constructors ([`pyramid_scene`], [`domino_scene`], ...). (Physics scenes P4a.)
    pub fn load_scene(&mut self, spec: SceneSpec) {
        self.physics.load_scene(spec);
        self.settle_physics(SETTLE_TICKS);
    }

    /// Load the demo "drop bowl" interactive scene (a bumpy fixed floor + dynamic balls
    /// falling under gravity). (Physics scenes P3.)
    pub fn load_demo_scene(&mut self) {
        self.load_scene(drop_bowl_scene());
    }

    /// Remove every scene body (the living backdrop / loaded scene). (Physics scenes P3.)
    pub fn clear_scene(&mut self) {
        self.physics.clear_scene();
    }

    /// Load a demo liquid pool: a block of PBF particles dropped into a basin behind the graph; the
    /// physics actor keeps ticking while it flows. Clear with [`clear_fluid`]. (Physics scenes P4c.)
    pub fn load_demo_fluid(&mut self) {
        let basin = seiche::Basin {
            min_x: -250.0,
            max_x: 250.0,
            floor_y: 250.0,
        };
        self.physics.load_fluid(
            seiche::FluidParams::default(),
            basin,
            Point2D::new(-150.0, -170.0),
            16,
            10,
            18.0,
        );
        self.settle_physics(SETTLE_TICKS);
    }

    /// Remove the liquid pool. (Physics scenes P4c.)
    pub fn clear_fluid(&mut self) {
        self.physics.clear_fluid();
    }

    /// Load the demo whirlpool: a ring of loose balls plus a centred vortex force-field that swirls
    /// them into an orbiting seiche. The field keeps the actor ticking; `clear_scene` (key `0`) drops
    /// both. (Physics scenes P4 — force-field tier.)
    pub fn load_whirlpool(&mut self) {
        self.physics.load_scene(whirlpool_scene());
        self.physics
            .set_scene_field(Some(seiche::SceneField::Vortex {
                center: (0.0, 0.0),
                strength: 90.0,
                inward: 30.0,
            }));
        self.settle_physics(SETTLE_TICKS);
    }

    /// Load the demo fountain: a catch basin plus an upward emitter whose droplets spray up, arc,
    /// and rain back down (ageing out so the jet is perpetual). The emitter keeps the actor ticking;
    /// `clear_scene` (key `0`) drops both. (Physics scenes — emitters.)
    pub fn load_fountain(&mut self) {
        self.physics.load_scene(fountain_scene());
        self.physics.add_emitter(seiche::SceneEmitter {
            collider: seiche::NodeCollider::Ball { radius: 7.0 },
            position: (0.0, 260.0),
            position_jitter: (10.0, 0.0),
            velocity: (0.0, -430.0),
            velocity_jitter: (70.0, 40.0),
            rate_per_sec: 45.0,
            lifetime_secs: 2.2,
            max_alive: 130,
        });
        self.settle_physics(SETTLE_TICKS);
    }

    /// The size-tier index (0..[`SIZE_TIERS`]`.len()`) nearest a node's current resolved
    /// size — where the resize control's filled notches stop. A size-by-degree or default
    /// size snaps to its nearest tier for display. (Node-rep — size tiers.)
    pub fn node_size_tier(&self, key: NodeKey) -> usize {
        let size = self.node_size(key);
        SIZE_TIERS
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (**a - size).abs().total_cmp(&(**b - size).abs()))
            .map(|(i, _)| i)
            .unwrap_or(1)
    }

    /// Step a node's size by `delta` tiers (−1 / +1) from its current nearest tier, set the
    /// per-node override to that preset, and return the new tier index. The first step snaps
    /// a size-by-degree / default size onto the tier ladder. Keyed by node UUID; returns the
    /// unchanged nearest tier for an unknown id. (Node-rep — size tiers.)
    pub fn step_node_size_tier(&mut self, id: uuid::Uuid, delta: i32) -> usize {
        let Some((key, _)) = self.graph.get_node_by_id(id) else {
            return 1;
        };
        let current = self.node_size_tier(key) as i32;
        let next = (current + delta).clamp(0, SIZE_TIERS.len() as i32 - 1) as usize;
        self.set_node_size(id, SIZE_TIERS[next]);
        next
    }

    /// The URL of the single focused (selected) node, if exactly one node is
    /// selected. The host reads this to project the focused node's media — e.g.
    /// meerkat's floating content card. `None` when zero or many are selected.
    pub fn focused_url(&self) -> Option<&str> {
        if self.selected.len() != 1 {
            return None;
        }
        let key = *self.selected.iter().next()?;
        self.graph.get_node(key).map(|n| n.url())
    }

    /// The focused node's center in **content-band** screen px (its world
    /// position through the camera, `screen = world * zoom + offset`), if exactly
    /// one node is selected. The host anchors the floating card next to this; add
    /// the toolbar height for window coords. `None` when zero or many are
    /// selected, or the node has no position yet. (Card system: anchored card.)
    pub fn focused_node_screen(&self) -> Option<(f32, f32)> {
        if self.selected.len() != 1 {
            return None;
        }
        let key = *self.selected.iter().next()?;
        let world = self.view.position_of(key)?;
        Some(
            self.camera
                .to_screen(kernel::geometry::PortablePoint::new(world.x, world.y)),
        )
    }

    /// The member id (node UUID) of the single focused node, if exactly one is
    /// selected. The host targets per-node navigation (omnibar, back/forward) at
    /// it in Cartography; in Tree the host uses the focused tile's member.
    pub fn focused_member(&self) -> Option<uuid::Uuid> {
        if self.selected.len() != 1 {
            return None;
        }
        let key = *self.selected.iter().next()?;
        self.graph.get_node(key).map(|n| n.id)
    }

    /// Navigate `member` in place to `url`: the node is a browsing surface whose
    /// content changes; its position does not (no new node, no edge, no
    /// re-settle). The within-node history grows. Returns false if `member` is
    /// unknown. Per-node navigation (the node-lineage model).
    pub fn navigate_member(&mut self, member: uuid::Uuid, url: &str) -> bool {
        let Some((key, _)) = self.graph.get_node_by_id(member) else {
            return false;
        };
        let _ = apply_graph_delta(
            &mut self.graph,
            GraphDelta::NavigateNode {
                key,
                url: url.to_string(),
            },
        );
        true
    }

    /// Step `member` back one visit in its own browse history, returning the
    /// revealed URL (the host re-fetches / re-renders it). `None` at the root.
    pub fn member_history_back(&mut self, member: uuid::Uuid) -> Option<String> {
        let (key, _) = self.graph.get_node_by_id(member)?;
        graph_apply::node_history_back(&mut self.graph, key)
    }

    /// Step `member` forward one visit in its own history. `None` at the tip.
    pub fn member_history_forward(&mut self, member: uuid::Uuid) -> Option<String> {
        let (key, _) = self.graph.get_node_by_id(member)?;
        graph_apply::node_history_forward(&mut self.graph, key)
    }

    /// Whether `member`'s history can step back (toolbar enablement).
    pub fn member_can_back(&self, member: uuid::Uuid) -> bool {
        match self.graph.get_node_by_id(member) {
            Some((key, _)) => self.graph.node_can_back(key),
            None => false,
        }
    }

    /// Whether `member` has ever been visited — it has a current entry in the
    /// shared navigation memory. The host shows a "last visit" snapshot for a
    /// visited node and the "unvisited" placeholder otherwise. (Card system #4.)
    pub fn member_visited(&self, member: uuid::Uuid) -> bool {
        match self.graph.get_node_by_id(member) {
            Some((key, _)) => self.graph.node_current_url(key).is_some(),
            None => false,
        }
    }

    /// Whether `member`'s history can step forward (toolbar enablement).
    pub fn member_can_forward(&self, member: uuid::Uuid) -> bool {
        match self.graph.get_node_by_id(member) {
            Some((key, _)) => self.graph.node_can_forward(key),
            None => false,
        }
    }

    /// The graph members (node UUIDs) of the currently-selected nodes. The host
    /// reads this for a selection-driven open: a single selection opens that
    /// node's graphlet, a multi-selection opens the selected nodes.
    pub fn selected_members(&self) -> Vec<uuid::Uuid> {
        self.selected
            .iter()
            .filter_map(|&k| self.graph.get_node(k).map(|n| n.id))
            .collect()
    }

    /// Replace the node selection with the nodes named by `members` (their UUIDs),
    /// dropping any uuid no longer in the graph. The inverse of
    /// [`selected_members`](Self::selected_members). The per-window selection
    /// install/readback uses this so two windows on one graph hold **independent**
    /// selection (and thus focus, derived via [`focused_key`](Self::focused_key)) over
    /// the shared node positions, the way per-window viewports do for the camera.
    /// Member-keyed (not `NodeKey`) so a window's selection survives an evict+reload.
    /// No-ops (skipping the reconcile) when the selection is unchanged — it runs on
    /// every ctx pass. (Per-window focus isolation.)
    pub fn set_selected_members(&mut self, members: &[uuid::Uuid]) {
        let new: HashSet<NodeKey> = members
            .iter()
            .filter_map(|id| self.graph.get_node_by_id(*id).map(|(k, _)| k))
            .collect();
        if new == self.selected {
            return;
        }
        self.selected = new;
        self.reconcile_derived();
    }

    /// The members in `member`'s connected component — `member` plus every node
    /// reachable from it through relations (undirected), breadth-first from the
    /// queried node. Empty if `member` is not in the graph. This is the node's
    /// "graphlet"; the host intersects it with the warm-tab set to decide what to
    /// tile.
    pub fn connected_members(&self, member: uuid::Uuid) -> Vec<uuid::Uuid> {
        let Some((start, _)) = self.graph.get_node_by_id(member) else {
            return Vec::new();
        };
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        let mut queue = VecDeque::new();
        seen.insert(start);
        queue.push_back(start);
        while let Some(key) = queue.pop_front() {
            if let Some(node) = self.graph.get_node(key) {
                order.push(node.id);
            }
            for neighbor in self.graph.neighbors_undirected_sorted(key) {
                if seen.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
        order
    }
}
