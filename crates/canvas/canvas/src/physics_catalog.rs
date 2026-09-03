// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The physics catalog: `laws × overlays`, and the named profiles over them.
//!
//! A **law** is which dynamics the graph moves under — springs and charges,
//! stress over hop distances, gravity and orbits, particle-life kinds, a
//! flock, coupled oscillators, a magnetic field, annealing, or none. Not a
//! tuning of one force-directed rule but a different rule. An **overlay** is
//! one more pull or push composed onto any law (hub room, group pull, depth,
//! grid, a centre, a tide). A **profile** is a named (law, overlays) pair; the
//! donor Graphshell's ten presets return as the first ten, and one profile
//! per law puts every law one pick away.
//!
//! The canvas builds the seiche force set from the chosen law + overlays
//! against the current graph (Stress needs hop distances, Orbit masses, Kinds
//! a kind per node, the group and depth overlays a grouping and a depth) and
//! hands it to the physics backend wholesale — the coupling / affinity /
//! anchor slots are separate and untouched. The choice rides the saved scene
//! by id. Sibling to the arrangement catalog in [`cartography_scene`](crate::cartography_scene):
//! that is *where* nodes go, this is *how* they move.
//!
//! Plan: `design_docs/mere_docs/implementation_strategy/2026-09-02_physics_catalog_plan.md`.

use std::collections::{HashMap, HashSet, VecDeque};

use kernel::graph::{Graph, NodeKey};
use seiche::{
    Anneal, BarnesHutRepulsion, Boids, Boundary, DegreeRepulsion, DepthGravity, DomainCluster,
    EdgeSpring, Force, Gravity, GravityLocus, GridSnap, HubGravity, Kuramoto, LinLogForce,
    MagneticSpring, NodeExclusion, ParticleLife, StressSpring, graph_distances,
};

use crate::cartography_scene::url_host;
use crate::seiche_bridge::visible_relation_edges;
use crate::{Canvas, SETTLE_TICKS};

/// The seed every seeded law (Kinds' rule matrix, Anneal's walk) starts from,
/// so a scene reopens to the same rules.
const LAW_SEED: u64 = 0x5EED_CA7A_1064;

/// The physics law: which dynamics the graph moves under. Ids are technical
/// (`family.method`), labels plain, as the arrangement catalog does it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhysicsLaw {
    /// Exclusion + edge springs + a centering boundary: the force-directed default.
    Springs,
    /// Barnes–Hut charge repulsion + edge springs: every body repels every other,
    /// distant ones by their cell's centre of mass.
    Charge,
    /// Kamada–Kawai stress: a spring between every connected pair at its hop
    /// distance, so the picture is a metric map of the graph.
    Stress,
    /// LinLog energy: linear attraction along edges, logarithmic repulsion, so
    /// communities separate and hubs sit central.
    Energy,
    /// Newtonian gravity with an orbital kick: hubs are suns, leaves circle them,
    /// and it never rests.
    Orbit,
    /// Particle life: nodes carry a kind, and a kind-by-kind rule matrix says who
    /// chases and who flees.
    Kinds,
    /// Boids: separation, alignment, cohesion, and a cruising speed; the graph
    /// moves as a flock.
    Flock,
    /// Kuramoto oscillators: each node has a phase, edges couple phases, and
    /// position follows phase on a ring, so communities become phase clusters.
    Sync,
    /// Magnetic springs: edges align to a field direction, so a directed graph
    /// reads as a flow.
    Flow,
    /// Davidson–Harel simulated annealing: a random walk over a layout energy,
    /// cooling to a minimum.
    Anneal,
    /// No law: bodies hold where the arrangement (or a hand) put them.
    Still,
}

impl PhysicsLaw {
    pub const ALL: [PhysicsLaw; 11] = [
        PhysicsLaw::Springs,
        PhysicsLaw::Charge,
        PhysicsLaw::Stress,
        PhysicsLaw::Energy,
        PhysicsLaw::Orbit,
        PhysicsLaw::Kinds,
        PhysicsLaw::Flock,
        PhysicsLaw::Sync,
        PhysicsLaw::Flow,
        PhysicsLaw::Anneal,
        PhysicsLaw::Still,
    ];

    pub fn id(self) -> &'static str {
        match self {
            PhysicsLaw::Springs => "spring.rapier",
            PhysicsLaw::Charge => "charge.barnes-hut",
            PhysicsLaw::Stress => "stress.kamada-kawai",
            PhysicsLaw::Energy => "energy.linlog",
            PhysicsLaw::Orbit => "orbit.gravity",
            PhysicsLaw::Kinds => "kinds.particle-life",
            PhysicsLaw::Flock => "flock.boids",
            PhysicsLaw::Sync => "sync.kuramoto",
            PhysicsLaw::Flow => "flow.magnetic",
            PhysicsLaw::Anneal => "anneal.davidson-harel",
            PhysicsLaw::Still => "still.default",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PhysicsLaw::Springs => "Springs",
            PhysicsLaw::Charge => "Charge",
            PhysicsLaw::Stress => "Stress",
            PhysicsLaw::Energy => "Energy",
            PhysicsLaw::Orbit => "Orbit",
            PhysicsLaw::Kinds => "Kinds",
            PhysicsLaw::Flock => "Flock",
            PhysicsLaw::Sync => "Sync",
            PhysicsLaw::Flow => "Flow",
            PhysicsLaw::Anneal => "Anneal",
            PhysicsLaw::Still => "Still",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|law| law.id() == id)
    }

    /// Whether the law is a living display that never comes to rest (Orbit,
    /// Flock, Sync), so the host keeps ticking rather than settling.
    pub fn never_rests(self) -> bool {
        matches!(
            self,
            PhysicsLaw::Orbit | PhysicsLaw::Flock | PhysicsLaw::Sync
        )
    }

    /// Whether the law snapshots graph structure at build (and so is rebuilt on
    /// a topology change): Stress's hop distances, Orbit's masses, Kinds' kinds.
    pub fn graph_bound(self) -> bool {
        matches!(
            self,
            PhysicsLaw::Stress | PhysicsLaw::Orbit | PhysicsLaw::Kinds
        )
    }
}

/// An overlay: one extra force composed onto any law.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhysicsOverlay {
    /// Hubs push their surroundings apart, by log degree.
    DegreeRepulsion,
    /// Nodes drift toward the centroid of their group (by site).
    DomainCluster,
    /// Everything is drawn toward the hubs.
    HubGravity,
    /// Depth from the roots drives the vertical: roots up, leaves down.
    DepthGravity,
    /// A spring to the nearest grid point.
    GridSnap,
    /// A gentle pull toward the canvas centre.
    GravityLocus,
    /// A centre that rides a slow sine, so the graph never fully settles.
    Tide,
}

impl PhysicsOverlay {
    pub const ALL: [PhysicsOverlay; 7] = [
        PhysicsOverlay::DegreeRepulsion,
        PhysicsOverlay::DomainCluster,
        PhysicsOverlay::HubGravity,
        PhysicsOverlay::DepthGravity,
        PhysicsOverlay::GridSnap,
        PhysicsOverlay::GravityLocus,
        PhysicsOverlay::Tide,
    ];

    pub fn id(self) -> &'static str {
        match self {
            PhysicsOverlay::DegreeRepulsion => "degree-repulsion",
            PhysicsOverlay::DomainCluster => "domain-cluster",
            PhysicsOverlay::HubGravity => "hub-gravity",
            PhysicsOverlay::DepthGravity => "depth-gravity",
            PhysicsOverlay::GridSnap => "grid-snap",
            PhysicsOverlay::GravityLocus => "gravity-locus",
            PhysicsOverlay::Tide => "tide",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PhysicsOverlay::DegreeRepulsion => "Hub room",
            PhysicsOverlay::DomainCluster => "Group pull",
            PhysicsOverlay::HubGravity => "Hub pull",
            PhysicsOverlay::DepthGravity => "Depth",
            PhysicsOverlay::GridSnap => "Grid",
            PhysicsOverlay::GravityLocus => "Centre",
            PhysicsOverlay::Tide => "Tide",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|overlay| overlay.id() == id)
    }

    /// Whether the overlay snapshots graph structure at build (the grouping, the
    /// depths), and so is rebuilt on a topology change.
    pub fn graph_bound(self) -> bool {
        matches!(
            self,
            PhysicsOverlay::DomainCluster | PhysicsOverlay::DepthGravity
        )
    }

    /// Whether the overlay keeps the graph moving on its own (the tide).
    pub fn never_rests(self) -> bool {
        matches!(self, PhysicsOverlay::Tide)
    }
}

/// Where the Kinds law reads a node's kind from — the host's choice per scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhysicsKindSource {
    /// The URL host: every site a kind.
    Site,
    /// The Louvain community: every cluster a kind.
    Cluster,
    /// Degree bands: isolated, leaf, connected, hub.
    Degree,
}

impl PhysicsKindSource {
    pub const ALL: [PhysicsKindSource; 3] = [
        PhysicsKindSource::Site,
        PhysicsKindSource::Cluster,
        PhysicsKindSource::Degree,
    ];

    pub fn id(self) -> &'static str {
        match self {
            PhysicsKindSource::Site => "site",
            PhysicsKindSource::Cluster => "cluster",
            PhysicsKindSource::Degree => "degree",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PhysicsKindSource::Site => "By site",
            PhysicsKindSource::Cluster => "By cluster",
            PhysicsKindSource::Degree => "By degree",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.id() == id)
    }
}

/// A named (law, overlays) pair: what a picker offers as one choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicsProfile {
    pub id: &'static str,
    pub label: &'static str,
    pub law: PhysicsLaw,
    pub overlays: &'static [PhysicsOverlay],
}

/// The law catalog for the picker: `(id, label)`, every law.
pub const CANVAS_PHYSICS_LAWS: &[(&str, &str)] = &[
    ("spring.rapier", "Springs"),
    ("charge.barnes-hut", "Charge"),
    ("stress.kamada-kawai", "Stress"),
    ("energy.linlog", "Energy"),
    ("orbit.gravity", "Orbit"),
    ("kinds.particle-life", "Kinds"),
    ("flock.boids", "Flock"),
    ("sync.kuramoto", "Sync"),
    ("flow.magnetic", "Flow"),
    ("anneal.davidson-harel", "Anneal"),
    ("still.default", "Still"),
];

/// The overlay catalog for the toggles: `(id, label)`.
pub const CANVAS_PHYSICS_OVERLAYS: &[(&str, &str)] = &[
    ("degree-repulsion", "Hub room"),
    ("domain-cluster", "Group pull"),
    ("hub-gravity", "Hub pull"),
    ("depth-gravity", "Depth"),
    ("grid-snap", "Grid"),
    ("gravity-locus", "Centre"),
    ("tide", "Tide"),
];

/// The kind-source catalog: `(id, label)`.
pub const CANVAS_PHYSICS_KIND_SOURCES: &[(&str, &str)] = &[
    ("site", "By site"),
    ("cluster", "By cluster"),
    ("degree", "By degree"),
];

/// The profile catalog: the donor's ten presets first (each a law + overlays,
/// under the donor's own name), then one bare profile per law the ten do not
/// already offer bare — Gas *is* Charge alone, Magnet Flow alone, Void Still
/// alone, so those three are not repeated: every (law, overlays) pair names
/// exactly one profile, which is what lets the picker show the live choice.
pub const CANVAS_PHYSICS_PROFILES: &[PhysicsProfile] = &[
    PhysicsProfile {
        id: "liquid",
        label: "Liquid",
        law: PhysicsLaw::Springs,
        overlays: &[PhysicsOverlay::GravityLocus],
    },
    PhysicsProfile {
        id: "gas",
        label: "Gas",
        law: PhysicsLaw::Charge,
        overlays: &[],
    },
    PhysicsProfile {
        id: "solid",
        label: "Solid",
        law: PhysicsLaw::Springs,
        overlays: &[
            PhysicsOverlay::DomainCluster,
            PhysicsOverlay::DegreeRepulsion,
        ],
    },
    PhysicsProfile {
        id: "archipelago",
        label: "Archipelago",
        law: PhysicsLaw::Energy,
        overlays: &[
            PhysicsOverlay::DomainCluster,
            PhysicsOverlay::DegreeRepulsion,
        ],
    },
    PhysicsProfile {
        id: "constellation",
        label: "Constellation",
        law: PhysicsLaw::Springs,
        overlays: &[PhysicsOverlay::DegreeRepulsion, PhysicsOverlay::HubGravity],
    },
    PhysicsProfile {
        id: "crystal",
        label: "Crystal",
        law: PhysicsLaw::Stress,
        overlays: &[PhysicsOverlay::GridSnap],
    },
    PhysicsProfile {
        id: "tide",
        label: "Tide",
        law: PhysicsLaw::Springs,
        overlays: &[PhysicsOverlay::Tide],
    },
    PhysicsProfile {
        id: "sediment",
        label: "Sediment",
        law: PhysicsLaw::Springs,
        overlays: &[PhysicsOverlay::DepthGravity],
    },
    PhysicsProfile {
        id: "magnet",
        label: "Magnet",
        law: PhysicsLaw::Flow,
        overlays: &[],
    },
    PhysicsProfile {
        id: "void",
        label: "Void",
        law: PhysicsLaw::Still,
        overlays: &[],
    },
    PhysicsProfile {
        id: "law.springs",
        label: "Springs",
        law: PhysicsLaw::Springs,
        overlays: &[],
    },
    PhysicsProfile {
        id: "law.stress",
        label: "Stress",
        law: PhysicsLaw::Stress,
        overlays: &[],
    },
    PhysicsProfile {
        id: "law.energy",
        label: "Energy",
        law: PhysicsLaw::Energy,
        overlays: &[],
    },
    PhysicsProfile {
        id: "law.orbit",
        label: "Orbit",
        law: PhysicsLaw::Orbit,
        overlays: &[],
    },
    PhysicsProfile {
        id: "law.kinds",
        label: "Kinds",
        law: PhysicsLaw::Kinds,
        overlays: &[],
    },
    PhysicsProfile {
        id: "law.flock",
        label: "Flock",
        law: PhysicsLaw::Flock,
        overlays: &[],
    },
    PhysicsProfile {
        id: "law.sync",
        label: "Sync",
        law: PhysicsLaw::Sync,
        overlays: &[],
    },
    PhysicsProfile {
        id: "law.anneal",
        label: "Anneal",
        law: PhysicsLaw::Anneal,
        overlays: &[],
    },
];

/// The profile with this id, if any.
pub fn physics_profile(id: &str) -> Option<&'static PhysicsProfile> {
    CANVAS_PHYSICS_PROFILES
        .iter()
        .find(|profile| profile.id == id)
}

/// The graph inputs a law or overlay snapshots at build: the node set, the
/// visible spring edges, degree, and (on demand) the site grouping and the
/// Louvain partition.
pub(crate) struct LawInputs<'a> {
    graph: &'a Graph,
    edges: Vec<(NodeKey, NodeKey)>,
    clusters: Option<&'a signals::ClusterSet>,
}

impl<'a> LawInputs<'a> {
    pub(crate) fn new(
        graph: &'a Graph,
        hidden_edges: &HashSet<crate::EdgeCell>,
        clusters: Option<&'a signals::ClusterSet>,
    ) -> Self {
        Self {
            graph,
            edges: visible_relation_edges(graph, hidden_edges),
            clusters,
        }
    }

    fn nodes(&self) -> Vec<NodeKey> {
        self.graph.nodes().map(|(key, _)| key).collect()
    }

    fn degrees(&self) -> HashMap<NodeKey, u32> {
        let mut degree: HashMap<NodeKey, u32> = HashMap::new();
        for (a, b) in &self.edges {
            *degree.entry(*a).or_default() += 1;
            *degree.entry(*b).or_default() += 1;
        }
        degree
    }

    /// Every node's group by URL host, as a dense id in first-seen order.
    fn site_groups(&self) -> Vec<(NodeKey, u32)> {
        let mut ids: HashMap<String, u32> = HashMap::new();
        self.graph
            .nodes()
            .map(|(key, node)| {
                let host = url_host(node.url());
                let next = ids.len() as u32;
                (key, *ids.entry(host).or_insert(next))
            })
            .collect()
    }

    /// Every node's Louvain community index (`None` without a partition).
    fn cluster_groups(&self) -> Option<Vec<(NodeKey, u32)>> {
        let clusters = self.clusters?;
        let mut groups = Vec::new();
        for (i, cluster) in clusters.clusters.iter().enumerate() {
            for &member in &cluster.members {
                groups.push((member, i as u32));
            }
        }
        Some(groups)
    }

    /// A kind per node for the Kinds law, from the chosen source, plus how many
    /// kinds there are.
    fn kinds(&self, source: PhysicsKindSource) -> (Vec<(NodeKey, u8)>, usize) {
        let groups: Vec<(NodeKey, u32)> = match source {
            PhysicsKindSource::Site => self.site_groups(),
            PhysicsKindSource::Cluster => {
                self.cluster_groups().unwrap_or_else(|| self.site_groups())
            }
            PhysicsKindSource::Degree => {
                let degree = self.degrees();
                self.nodes()
                    .into_iter()
                    .map(|key| {
                        let d = degree.get(&key).copied().unwrap_or(0);
                        (
                            key,
                            match d {
                                0 => 0,
                                1 => 1,
                                2..=4 => 2,
                                _ => 3,
                            },
                        )
                    })
                    .collect()
            }
        };
        // Particle life reads best with a handful of kinds; fold a long tail of
        // sites into eight, keeping a small catalog its own size.
        let distinct = groups
            .iter()
            .map(|(_, g)| *g)
            .max()
            .map_or(0, |g| g as usize + 1);
        let kind_count = distinct.clamp(1, 8);
        let kinds = groups
            .into_iter()
            .map(|(key, g)| (key, (g as usize % kind_count) as u8))
            .collect();
        (kinds, kind_count)
    }

    /// BFS depth from the roots — the nodes with no incoming visible edge, or
    /// every node when the graph is all cycles.
    fn depths(&self) -> Vec<(NodeKey, u32)> {
        let nodes = self.nodes();
        let mut incoming: HashSet<NodeKey> = HashSet::new();
        let mut out: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
        for (a, b) in &self.edges {
            incoming.insert(*b);
            out.entry(*a).or_default().push(*b);
        }
        let mut roots: Vec<NodeKey> = nodes
            .iter()
            .copied()
            .filter(|k| !incoming.contains(k))
            .collect();
        if roots.is_empty() {
            roots = nodes.clone();
        }
        let mut depth: HashMap<NodeKey, u32> = HashMap::new();
        let mut queue: VecDeque<NodeKey> = VecDeque::new();
        for root in roots {
            depth.insert(root, 0);
            queue.push_back(root);
        }
        while let Some(key) = queue.pop_front() {
            let d = depth[&key];
            if let Some(children) = out.get(&key) {
                for &child in children {
                    if !depth.contains_key(&child) {
                        depth.insert(child, d + 1);
                        queue.push_back(child);
                    }
                }
            }
        }
        // A node reachable only through a cycle off the roots gets the depth of
        // its nearest placed neighbour plus one, so nothing is left unplaced.
        for key in nodes {
            depth.entry(key).or_insert(1);
        }
        depth.into_iter().collect()
    }

    /// Build the law's own forces against these inputs.
    pub(crate) fn law_forces(
        &self,
        law: PhysicsLaw,
        kind_source: PhysicsKindSource,
    ) -> Vec<Box<dyn Force>> {
        match law {
            PhysicsLaw::Springs => vec![
                Box::new(NodeExclusion::default()),
                Box::new(EdgeSpring::default()),
                Box::new(Boundary::default()),
            ],
            PhysicsLaw::Charge => vec![
                Box::new(BarnesHutRepulsion::default()),
                Box::new(EdgeSpring::default()),
                Box::new(Boundary::default()),
            ],
            PhysicsLaw::Stress => {
                let nodes = self.nodes();
                let distances = graph_distances(nodes.iter().copied(), &self.edges);
                vec![
                    Box::new(NodeExclusion::default()),
                    Box::new(StressSpring::from_distances(
                        distances,
                        EdgeSpring::default().rest_length,
                    )),
                    Box::new(Boundary::default()),
                ]
            }
            PhysicsLaw::Energy => vec![
                Box::new(NodeExclusion::default()),
                Box::new(LinLogForce::default()),
            ],
            PhysicsLaw::Orbit => {
                let degree = self.degrees();
                let masses = self
                    .nodes()
                    .into_iter()
                    .map(|key| (key, 1.0 + degree.get(&key).copied().unwrap_or(0) as f32));
                vec![
                    Box::new(NodeExclusion::default()),
                    Box::new(Gravity::new(masses)),
                ]
            }
            PhysicsLaw::Kinds => {
                let (kinds, kind_count) = self.kinds(kind_source);
                vec![
                    Box::new(NodeExclusion::default()),
                    Box::new(ParticleLife::seeded(kinds, kind_count, LAW_SEED)),
                ]
            }
            PhysicsLaw::Flock => vec![
                Box::new(NodeExclusion::default()),
                Box::new(Boids::default()),
            ],
            PhysicsLaw::Sync => {
                // Every node on one ring; communities become arcs of it.
                let radii = self.nodes().into_iter().map(|key| (key, 240.0));
                vec![
                    Box::new(NodeExclusion::default()),
                    Box::new(Kuramoto::new(radii)),
                ]
            }
            PhysicsLaw::Flow => vec![
                Box::new(NodeExclusion::default()),
                Box::new(MagneticSpring::default()),
                Box::new(Boundary::default()),
            ],
            PhysicsLaw::Anneal => vec![Box::new(Anneal::seeded(LAW_SEED))],
            PhysicsLaw::Still => Vec::new(),
        }
    }

    /// Build one overlay's force against these inputs.
    pub(crate) fn overlay_force(&self, overlay: PhysicsOverlay) -> Box<dyn Force> {
        match overlay {
            PhysicsOverlay::DegreeRepulsion => Box::new(DegreeRepulsion::default()),
            PhysicsOverlay::DomainCluster => Box::new(DomainCluster::new(self.site_groups())),
            PhysicsOverlay::HubGravity => Box::new(HubGravity::default()),
            PhysicsOverlay::DepthGravity => Box::new(DepthGravity::new(self.depths())),
            PhysicsOverlay::GridSnap => Box::new(GridSnap::default()),
            PhysicsOverlay::GravityLocus => Box::new(GravityLocus::at((0.0, 0.0))),
            PhysicsOverlay::Tide => Box::new(GravityLocus::tidal((0.0, 0.0), 240.0, 24.0)),
        }
    }

    /// The whole force set: the law, then the overlays in order.
    pub(crate) fn forces(
        &self,
        law: PhysicsLaw,
        overlays: &[PhysicsOverlay],
        kind_source: PhysicsKindSource,
    ) -> Vec<Box<dyn Force>> {
        let mut forces = self.law_forces(law, kind_source);
        forces.extend(overlays.iter().map(|overlay| self.overlay_force(*overlay)));
        forces
    }
}

impl Canvas {
    /// The physics law the graph moves under.
    pub fn physics_law(&self) -> PhysicsLaw {
        self.physics_law
    }

    /// The overlays composed onto the law, in run order.
    pub fn physics_overlays(&self) -> &[PhysicsOverlay] {
        &self.physics_overlays
    }

    /// Where the Kinds law reads a node's kind from.
    pub fn physics_kind_source(&self) -> PhysicsKindSource {
        self.physics_kind_source
    }

    /// Switch the law. The force set is replaced wholesale; no body moves until
    /// the next tick, then a settle (or, for a law that never rests, a
    /// continuous run) lets the new dynamics express themselves. Physics stays
    /// paused if it was paused. (Physics catalog — P1.)
    pub fn set_physics_law(&mut self, law: PhysicsLaw) {
        self.physics_law = law;
        self.rebuild_law_forces();
        self.settle_for_law();
    }

    /// Replace the overlay set (order is run order; duplicates collapse).
    pub fn set_physics_overlays(&mut self, overlays: Vec<PhysicsOverlay>) {
        let mut seen = HashSet::new();
        self.physics_overlays = overlays.into_iter().filter(|o| seen.insert(*o)).collect();
        self.rebuild_law_forces();
        self.settle_for_law();
    }

    /// Toggle one overlay on or off, returning whether it is now on.
    pub fn toggle_physics_overlay(&mut self, overlay: PhysicsOverlay) -> bool {
        let mut overlays = self.physics_overlays.clone();
        let on = if let Some(i) = overlays.iter().position(|o| *o == overlay) {
            overlays.remove(i);
            false
        } else {
            overlays.push(overlay);
            true
        };
        self.set_physics_overlays(overlays);
        on
    }

    /// Choose where the Kinds law reads kinds from; rebuilds only if Kinds is live.
    pub fn set_physics_kind_source(&mut self, source: PhysicsKindSource) {
        self.physics_kind_source = source;
        if self.physics_law == PhysicsLaw::Kinds {
            self.rebuild_law_forces();
            self.settle_for_law();
        }
    }

    /// Apply a named profile: its law and its overlays. `false` for an unknown id.
    pub fn apply_physics_profile(&mut self, id: &str) -> bool {
        let Some(profile) = physics_profile(id) else {
            return false;
        };
        self.physics_law = profile.law;
        self.physics_overlays = profile.overlays.to_vec();
        self.rebuild_law_forces();
        self.settle_for_law();
        true
    }

    /// The profile whose law and overlays match the live choice, if any.
    pub fn physics_profile_id(&self) -> Option<&'static str> {
        CANVAS_PHYSICS_PROFILES
            .iter()
            .find(|profile| {
                profile.law == self.physics_law
                    && profile.overlays == self.physics_overlays.as_slice()
            })
            .map(|profile| profile.id)
    }

    /// Whether the live law or an overlay snapshots graph structure, so a
    /// topology change must rebuild it.
    pub(crate) fn physics_forces_are_graph_bound(&self) -> bool {
        self.physics_law.graph_bound() || self.physics_overlays.iter().any(|o| o.graph_bound())
    }

    /// Whether the live law or an overlay keeps the graph moving on its own.
    pub fn physics_never_rests(&self) -> bool {
        self.physics_law.never_rests() || self.physics_overlays.iter().any(|o| o.never_rests())
    }

    /// Rebuild the law + overlay force set against the current graph and hand it
    /// to the physics backend. Position-preserving.
    pub(crate) fn rebuild_law_forces(&mut self) {
        let wants_clusters = self.physics_law == PhysicsLaw::Kinds
            && self.physics_kind_source == PhysicsKindSource::Cluster;
        if wants_clusters {
            self.ensure_community_fresh();
        }
        let forces = {
            let inputs = LawInputs::new(
                &self.graph,
                &self.hidden_edges,
                if wants_clusters {
                    self.community_cache.as_ref()
                } else {
                    None
                },
            );
            inputs.forces(
                self.physics_law,
                &self.physics_overlays,
                self.physics_kind_source,
            )
        };
        self.physics.set_forces(forces);
    }

    /// The settle a law switch earns: a living law runs until paused, the rest
    /// settle for the usual budget.
    fn settle_for_law(&mut self) {
        if self.physics_never_rests() {
            self.settle_physics(u32::MAX);
        } else {
            self.settle_physics(SETTLE_TICKS);
        }
    }

    /// The number of forces in the live law slot (inline backend only). Test introspection.
    #[cfg(test)]
    pub(crate) fn law_force_count(&self) -> usize {
        self.physics.force_count()
    }
}
