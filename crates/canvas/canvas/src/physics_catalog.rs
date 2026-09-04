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
//! grid, a centre, a tide, the skeleton). A **profile** is a named (law,
//! overlays) pair; the donor Graphshell's ten presets return as the first
//! ten, and a bare profile per law puts every law one pick away.
//!
//! A **source** is where a law or overlay reads a node attribute from: the
//! Kinds law's kind (site, cluster, colouring, island, degree), Orbit's mass
//! and the hub overlays' weight (degree, PageRank), the Depth overlay's depth
//! (roots, layers, the focus). Sources are tunables, not laws.
//!
//! The canvas builds the seiche force set from the chosen law + overlays
//! against the current graph and hands it to the physics backend wholesale —
//! the coupling / affinity / anchor slots are separate and untouched. The
//! attribute builders run over a petgraph view of the *visible* edges
//! ([`LawInputs`]), so hidden relations relax the physics as they relax the
//! springs. The choice rides the saved scene by id. Sibling to the
//! arrangement catalog in [`cartography_scene`](crate::cartography_scene):
//! that is *where* nodes go, this is *how* they move.
//!
//! Plan: `design_docs/mere_docs/implementation_strategy/2026-09-02_physics_catalog_plan.md`.

use std::collections::{HashMap, HashSet, VecDeque};

use kernel::graph::{Graph, NodeKey};
use petgraph::Direction;
use petgraph::algo::{
    dijkstra, dominators, dsatur_coloring, greedy_feedback_arc_set, min_spanning_tree, page_rank,
    tarjan_scc, toposort,
};
use petgraph::data::Element;
use petgraph::graph::{DiGraph, EdgeIndex, NodeIndex, UnGraph};
use petgraph::visit::EdgeRef;
use seiche::{
    Anneal, BarnesHutRepulsion, Boids, Boundary, DegreeRepulsion, DepthGravity, DomainCluster,
    EdgeSpring, Force, Gravity, GravityLocus, GridSnap, Hold, HubGravity, Kuramoto, LinLogForce,
    MagneticSpring, NodeExclusion, ParticleLife, StressSpring,
};

use crate::cartography_scene::url_host;
use crate::seiche_bridge::visible_relation_edges;
use crate::{Canvas, SETTLE_TICKS};

/// The seed every seeded law (Kinds' rule matrix, Anneal's walk) starts from,
/// so a scene reopens to the same rules.
const LAW_SEED: u64 = 0x5EED_CA7A_1064;
/// PageRank's damping and iteration budget.
const PAGE_RANK_DAMPING: f32 = 0.85;
const PAGE_RANK_ITERATIONS: usize = 50;
/// The Skeleton overlay's tree-edge stiffness, against `EdgeSpring`'s 10.
const SKELETON_STIFFNESS: f32 = 60.0;
/// Charge's repulsion, calibrated so that at contact (a node diameter, 36)
/// the `1/d` push matches `NodeExclusion`'s inverse-square one
/// (`220_000 / 36² ≈ 170`): the seiche default of 2 400 left bodies
/// touching under the edge springs. (Physics catalog — the Charge receipt.)
const CHARGE_STRENGTH: f32 = 6_000.0;

/// The physics law: which dynamics the graph moves under. Ids are technical
/// (`family.method`), labels plain, as the arrangement catalog does it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhysicsLaw {
    /// Exclusion + edge springs + a centering boundary: the force-directed default.
    Springs,
    /// Barnes–Hut charge repulsion + edge springs: every body repels every other,
    /// distant ones by their cell's centre of mass.
    Charge,
    /// Kamada–Kawai stress: a spring between every connected pair at its
    /// shortest-path distance (relation multiplicity shortens a hop), so the
    /// picture is a metric map of the graph.
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
    /// No law: bodies hold where the arrangement (or a hand) put them
    /// (velocity zeroed each tick, so contacts can only nudge).
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
    /// a topology change): Stress's distances, Orbit's masses, Kinds' kinds.
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
    /// Hubs push their surroundings apart, by weight.
    DegreeRepulsion,
    /// Nodes drift toward the centroid of their group (by site).
    DomainCluster,
    /// Everything is drawn toward the hubs, by weight.
    HubGravity,
    /// Depth drives the vertical: roots up, leaves down.
    DepthGravity,
    /// A spring to the nearest grid point.
    GridSnap,
    /// A gentle pull toward the canvas centre.
    GravityLocus,
    /// A centre that rides a slow sine, so the graph never fully settles.
    Tide,
    /// The spanning tree's edges held stiff, so the backbone shows under any law.
    Skeleton,
}

impl PhysicsOverlay {
    pub const ALL: [PhysicsOverlay; 8] = [
        PhysicsOverlay::DegreeRepulsion,
        PhysicsOverlay::DomainCluster,
        PhysicsOverlay::HubGravity,
        PhysicsOverlay::DepthGravity,
        PhysicsOverlay::GridSnap,
        PhysicsOverlay::GravityLocus,
        PhysicsOverlay::Tide,
        PhysicsOverlay::Skeleton,
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
            PhysicsOverlay::Skeleton => "skeleton",
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
            PhysicsOverlay::Skeleton => "Skeleton",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|overlay| overlay.id() == id)
    }

    /// Whether the overlay snapshots graph structure at build (the grouping,
    /// the depths, the tree), and so is rebuilt on a topology change.
    pub fn graph_bound(self) -> bool {
        matches!(
            self,
            PhysicsOverlay::DomainCluster | PhysicsOverlay::DepthGravity | PhysicsOverlay::Skeleton
        )
    }

    /// Whether the overlay reads the mass source's weights (and so is
    /// graph-bound whenever that source is computed from the topology).
    pub fn weighted(self) -> bool {
        matches!(
            self,
            PhysicsOverlay::DegreeRepulsion | PhysicsOverlay::HubGravity
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
    /// A proper colouring (DSATUR): a kind never touches its own kind, so the
    /// rule matrix plays out between neighbours.
    Coloring,
    /// The connected component: every island a kind.
    Component,
    /// Degree bands: isolated, leaf, connected, hub.
    Degree,
}

impl PhysicsKindSource {
    pub const ALL: [PhysicsKindSource; 5] = [
        PhysicsKindSource::Site,
        PhysicsKindSource::Cluster,
        PhysicsKindSource::Coloring,
        PhysicsKindSource::Component,
        PhysicsKindSource::Degree,
    ];

    pub fn id(self) -> &'static str {
        match self {
            PhysicsKindSource::Site => "site",
            PhysicsKindSource::Cluster => "cluster",
            PhysicsKindSource::Coloring => "coloring",
            PhysicsKindSource::Component => "component",
            PhysicsKindSource::Degree => "degree",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PhysicsKindSource::Site => "By site",
            PhysicsKindSource::Cluster => "By cluster",
            PhysicsKindSource::Coloring => "By colouring",
            PhysicsKindSource::Component => "By island",
            PhysicsKindSource::Degree => "By degree",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.id() == id)
    }
}

/// Where Orbit's masses and the hub overlays' weights come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhysicsMassSource {
    /// Degree: the most-connected node is the heaviest.
    Degree,
    /// PageRank: the most linked-to node is the heaviest, links from heavy
    /// nodes counting for more.
    PageRank,
}

impl PhysicsMassSource {
    pub const ALL: [PhysicsMassSource; 2] =
        [PhysicsMassSource::Degree, PhysicsMassSource::PageRank];

    pub fn id(self) -> &'static str {
        match self {
            PhysicsMassSource::Degree => "degree",
            PhysicsMassSource::PageRank => "pagerank",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PhysicsMassSource::Degree => "By degree",
            PhysicsMassSource::PageRank => "By rank",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.id() == id)
    }
}

/// Where the Depth overlay reads a node's depth from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhysicsDepthSource {
    /// Breadth-first from the nodes nothing points at.
    Roots,
    /// Sugiyama's layer step: cut a feedback arc set, then longest-path layers
    /// over the remaining order — works on cycles.
    Layers,
    /// Dominator-tree depth from the focused node: what you must pass through
    /// to reach a node from the selection. Roots when nothing is focused.
    Focus,
}

impl PhysicsDepthSource {
    pub const ALL: [PhysicsDepthSource; 3] = [
        PhysicsDepthSource::Roots,
        PhysicsDepthSource::Layers,
        PhysicsDepthSource::Focus,
    ];

    pub fn id(self) -> &'static str {
        match self {
            PhysicsDepthSource::Roots => "roots",
            PhysicsDepthSource::Layers => "layers",
            PhysicsDepthSource::Focus => "focus",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PhysicsDepthSource::Roots => "From roots",
            PhysicsDepthSource::Layers => "By layer",
            PhysicsDepthSource::Focus => "From focus",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.id() == id)
    }
}

/// What the layout looks like right now, in numbers a receipt can assert
/// on: the laws' signatures. (Physics catalog — P2.)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutStats {
    /// The node bodies' kinetic energy (inline backend; zero offloaded).
    pub energy: f32,
    /// Root-mean-square distance of the nodes from their centroid.
    pub spread: f32,
    /// Node pairs closer than a node body's diameter.
    pub overlaps: usize,
    /// Canvas distance between the two graph-farthest connected nodes,
    /// divided by the mean visible edge length — a metric layout (Stress)
    /// reads near the diameter in hops, a local one well under it. Zero
    /// without edges.
    pub stretch: f32,
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
    ("skeleton", "Skeleton"),
];

/// The kind-source catalog: `(id, label)`.
pub const CANVAS_PHYSICS_KIND_SOURCES: &[(&str, &str)] = &[
    ("site", "By site"),
    ("cluster", "By cluster"),
    ("coloring", "By colouring"),
    ("component", "By island"),
    ("degree", "By degree"),
];

/// The mass-source catalog: `(id, label)`.
pub const CANVAS_PHYSICS_MASS_SOURCES: &[(&str, &str)] =
    &[("degree", "By degree"), ("pagerank", "By rank")];

/// The depth-source catalog: `(id, label)`.
pub const CANVAS_PHYSICS_DEPTH_SOURCES: &[(&str, &str)] = &[
    ("roots", "From roots"),
    ("layers", "By layer"),
    ("focus", "From focus"),
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
    PhysicsProfile {
        id: "skeleton",
        label: "Skeleton",
        law: PhysicsLaw::Charge,
        overlays: &[PhysicsOverlay::Skeleton],
    },
];

/// The profile with this id, if any.
pub fn physics_profile(id: &str) -> Option<&'static PhysicsProfile> {
    CANVAS_PHYSICS_PROFILES
        .iter()
        .find(|profile| profile.id == id)
}

/// The sources a build reads attributes through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LawSources {
    pub kind: PhysicsKindSource,
    pub mass: PhysicsMassSource,
    pub depth: PhysicsDepthSource,
    /// The focused node, for the Focus depth source.
    pub focus: Option<NodeKey>,
}

/// A petgraph view of the visible topology: the directed multigraph (one
/// edge per visible relation cell) for PageRank, layering, dominators and
/// components; the undirected simple graph (one edge per pair, cost
/// `1 / multiplicity`) for the colouring, the spanning tree and the
/// shortest paths. Built per force rebuild; linear in the graph.
pub(crate) struct TopologyView {
    directed: DiGraph<NodeKey, f32>,
    undirected: UnGraph<NodeKey, f32>,
    index_of: HashMap<NodeKey, NodeIndex>,
}

impl TopologyView {
    fn new(nodes: &[NodeKey], edges: &[(NodeKey, NodeKey)]) -> Self {
        let mut directed = DiGraph::new();
        let mut undirected = UnGraph::new_undirected();
        let mut index_of = HashMap::with_capacity(nodes.len());
        for &key in nodes {
            let d = directed.add_node(key);
            let u = undirected.add_node(key);
            debug_assert_eq!(d, u);
            index_of.insert(key, d);
        }
        let mut multiplicity: HashMap<(NodeIndex, NodeIndex), u32> = HashMap::new();
        for &(a, b) in edges {
            let (Some(&ia), Some(&ib)) = (index_of.get(&a), index_of.get(&b)) else {
                continue;
            };
            if ia == ib {
                continue;
            }
            directed.add_edge(ia, ib, 1.0);
            let pair = if ia < ib { (ia, ib) } else { (ib, ia) };
            *multiplicity.entry(pair).or_default() += 1;
        }
        let mut pairs: Vec<_> = multiplicity.into_iter().collect();
        pairs.sort_by_key(|((a, b), _)| (a.index(), b.index()));
        for ((a, b), m) in pairs {
            undirected.add_edge(a, b, 1.0 / m as f32);
        }
        Self {
            directed,
            undirected,
            index_of,
        }
    }

    fn key(&self, index: NodeIndex) -> NodeKey {
        self.directed[index]
    }
}

/// The graph inputs a law or overlay snapshots at build: the node set, the
/// visible spring edges, and (on demand) degree, the site grouping, the
/// Louvain partition and the petgraph view.
pub(crate) struct LawInputs<'a> {
    nodes: Vec<NodeKey>,
    edges: Vec<(NodeKey, NodeKey)>,
    /// Each node's site (the URL host for a graph node; whatever grouping a
    /// board's host names), the Kinds law's and the group overlay's default.
    sites: HashMap<NodeKey, String>,
    clusters: Option<&'a signals::ClusterSet>,
}

impl<'a> LawInputs<'a> {
    pub(crate) fn new(
        graph: &Graph,
        hidden_edges: &HashSet<crate::EdgeCell>,
        clusters: Option<&'a signals::ClusterSet>,
    ) -> Self {
        let sites = graph
            .nodes()
            .map(|(key, node)| (key, url_host(node.url())))
            .collect();
        let mut inputs = Self::from_parts(
            graph.nodes().map(|(key, _)| key).collect(),
            visible_relation_edges(graph, hidden_edges),
            sites,
        );
        inputs.clusters = clusters;
        inputs
    }

    /// Graph-free inputs: a node list, an edge list, and a site per node —
    /// what a [`PhysicsBoard`](crate::PhysicsBoard) has for a scene's items.
    pub(crate) fn from_parts(
        mut nodes: Vec<NodeKey>,
        edges: Vec<(NodeKey, NodeKey)>,
        sites: HashMap<NodeKey, String>,
    ) -> Self {
        nodes.sort_by_key(|key| key.index());
        Self {
            nodes,
            edges,
            sites,
            clusters: None,
        }
    }

    pub(crate) fn topology(&self) -> TopologyView {
        TopologyView::new(&self.nodes, &self.edges)
    }

    fn degrees(&self) -> HashMap<NodeKey, u32> {
        let mut degree: HashMap<NodeKey, u32> = HashMap::new();
        for (a, b) in &self.edges {
            *degree.entry(*a).or_default() += 1;
            *degree.entry(*b).or_default() += 1;
        }
        degree
    }

    /// Every node's group by site, as a dense id in first-seen order.
    fn site_groups(&self) -> Vec<(NodeKey, u32)> {
        let mut ids: HashMap<String, u32> = HashMap::new();
        self.nodes
            .iter()
            .map(|&key| {
                let site = self.sites.get(&key).cloned().unwrap_or_default();
                let next = ids.len() as u32;
                (key, *ids.entry(site).or_insert(next))
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

    /// A proper colouring of the visible graph (DSATUR): adjacent nodes never
    /// share a group.
    pub(crate) fn coloring_groups(&self) -> Vec<(NodeKey, u32)> {
        let view = self.topology();
        let (colors, _) = dsatur_coloring(&view.undirected);
        self.nodes
            .iter()
            .map(|&key| {
                let color = view
                    .index_of
                    .get(&key)
                    .and_then(|index| colors.get(index))
                    .copied()
                    .unwrap_or(0);
                (key, color as u32)
            })
            .collect()
    }

    /// Every node's connected component (island), as a dense id.
    pub(crate) fn component_groups(&self) -> Vec<(NodeKey, u32)> {
        let view = self.topology();
        // The strongly-connected components of an undirected graph are its
        // connected components.
        let mut of: HashMap<NodeKey, u32> = HashMap::new();
        let mut components = tarjan_scc(&view.undirected);
        components.sort_by_key(|members| members.iter().map(|i| i.index()).min());
        for (i, members) in components.iter().enumerate() {
            for &member in members {
                of.insert(view.key(member), i as u32);
            }
        }
        self.nodes
            .iter()
            .map(|&key| (key, of.get(&key).copied().unwrap_or(0)))
            .collect()
    }

    /// PageRank over the directed view, scaled so the mean weight is one
    /// (comparable to log degree on a sparse graph).
    pub(crate) fn page_rank_weights(&self) -> Vec<(NodeKey, f32)> {
        let view = self.topology();
        let n = view.directed.node_count();
        if n == 0 {
            return Vec::new();
        }
        let ranks = page_rank(&view.directed, PAGE_RANK_DAMPING, PAGE_RANK_ITERATIONS);
        self.nodes
            .iter()
            .map(|&key| {
                let rank = view
                    .index_of
                    .get(&key)
                    .and_then(|index| ranks.get(index.index()))
                    .copied()
                    .unwrap_or(0.0);
                (key, rank * n as f32)
            })
            .collect()
    }

    /// Masses for Orbit: `1 + degree`, or `1 + rank` with the mean rank one.
    fn masses(&self, source: PhysicsMassSource) -> Vec<(NodeKey, f32)> {
        match source {
            PhysicsMassSource::Degree => {
                let degree = self.degrees();
                self.nodes
                    .iter()
                    .map(|&key| (key, 1.0 + degree.get(&key).copied().unwrap_or(0) as f32))
                    .collect()
            }
            PhysicsMassSource::PageRank => self
                .page_rank_weights()
                .into_iter()
                .map(|(key, rank)| (key, 1.0 + rank))
                .collect(),
        }
    }

    /// A kind per node for the Kinds law, from the chosen source, plus how many
    /// kinds there are.
    pub(crate) fn kinds(&self, source: PhysicsKindSource) -> (Vec<(NodeKey, u8)>, usize) {
        let groups: Vec<(NodeKey, u32)> = match source {
            PhysicsKindSource::Site => self.site_groups(),
            PhysicsKindSource::Cluster => {
                self.cluster_groups().unwrap_or_else(|| self.site_groups())
            }
            PhysicsKindSource::Coloring => self.coloring_groups(),
            PhysicsKindSource::Component => self.component_groups(),
            PhysicsKindSource::Degree => {
                let degree = self.degrees();
                self.nodes
                    .iter()
                    .map(|&key| {
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
    pub(crate) fn root_depths(&self) -> Vec<(NodeKey, u32)> {
        let mut incoming: HashSet<NodeKey> = HashSet::new();
        let mut out: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
        for (a, b) in &self.edges {
            incoming.insert(*b);
            out.entry(*a).or_default().push(*b);
        }
        let mut roots: Vec<NodeKey> = self
            .nodes
            .iter()
            .copied()
            .filter(|k| !incoming.contains(k))
            .collect();
        if roots.is_empty() {
            roots = self.nodes.clone();
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
        // A node reachable only through a cycle off the roots gets depth one,
        // so nothing is left unplaced.
        self.nodes
            .iter()
            .map(|&key| (key, depth.get(&key).copied().unwrap_or(1)))
            .collect()
    }

    /// Sugiyama's layer step: cut a greedy feedback arc set so the directed
    /// view is acyclic, then longest-path layering in topological order.
    pub(crate) fn layer_depths(&self) -> Vec<(NodeKey, u32)> {
        let view = self.topology();
        let mut dag = view.directed.clone();
        let cut: HashSet<EdgeIndex> = greedy_feedback_arc_set(&dag).map(|e| e.id()).collect();
        dag.retain_edges(|_, e| !cut.contains(&e));
        let Ok(order) = toposort(&dag, None) else {
            return self.root_depths();
        };
        let mut layer: HashMap<NodeIndex, u32> = HashMap::with_capacity(order.len());
        for v in order {
            let l = dag
                .neighbors_directed(v, Direction::Incoming)
                .filter_map(|u| layer.get(&u).map(|d| d + 1))
                .max()
                .unwrap_or(0);
            layer.insert(v, l);
        }
        self.nodes
            .iter()
            .map(|&key| {
                let d = view
                    .index_of
                    .get(&key)
                    .and_then(|index| layer.get(index))
                    .copied()
                    .unwrap_or(0);
                (key, d)
            })
            .collect()
    }

    /// Dominator-tree depth from `focus`: the number of nodes every path from
    /// the focus must pass through. Unreachable nodes sit one level below the
    /// deepest reachable one; without a focus, the roots.
    pub(crate) fn focus_depths(&self, focus: Option<NodeKey>) -> Vec<(NodeKey, u32)> {
        let view = self.topology();
        let Some(root) = focus.and_then(|key| view.index_of.get(&key).copied()) else {
            return self.root_depths();
        };
        let dom = dominators::simple_fast(&view.directed, root);
        let mut depth: HashMap<NodeKey, u32> = HashMap::new();
        let mut deepest = 0;
        for &key in &self.nodes {
            let Some(index) = view.index_of.get(&key) else {
                continue;
            };
            if let Some(chain) = dom.dominators(*index) {
                let d = chain.count().saturating_sub(1) as u32;
                deepest = deepest.max(d);
                depth.insert(key, d);
            }
        }
        self.nodes
            .iter()
            .map(|&key| (key, depth.get(&key).copied().unwrap_or(deepest + 1)))
            .collect()
    }

    fn depths(&self, source: PhysicsDepthSource, focus: Option<NodeKey>) -> Vec<(NodeKey, u32)> {
        match source {
            PhysicsDepthSource::Roots => self.root_depths(),
            PhysicsDepthSource::Layers => self.layer_depths(),
            PhysicsDepthSource::Focus => self.focus_depths(focus),
        }
    }

    /// The minimum spanning tree's edges over the undirected view (a pair
    /// with more relations is a shorter edge, so the tree prefers it).
    pub(crate) fn skeleton_edges(&self) -> Vec<(NodeKey, NodeKey)> {
        let view = self.topology();
        min_spanning_tree(&view.undirected)
            .filter_map(|element| match element {
                Element::Edge { source, target, .. } => Some((
                    view.key(NodeIndex::new(source)),
                    view.key(NodeIndex::new(target)),
                )),
                Element::Node { .. } => None,
            })
            .collect()
    }

    /// Shortest-path distances in hops over the undirected view, each hop
    /// costing `1 / multiplicity`, every connected pair once.
    pub(crate) fn weighted_distances(&self) -> Vec<(NodeKey, NodeKey, f32)> {
        let view = self.topology();
        let mut out = Vec::new();
        for (i, &a) in self.nodes.iter().enumerate() {
            let Some(&ia) = view.index_of.get(&a) else {
                continue;
            };
            let reach = dijkstra(&view.undirected, ia, None, |e| *e.weight());
            for &b in &self.nodes[i + 1..] {
                if let Some(d) = view.index_of.get(&b).and_then(|ib| reach.get(ib)) {
                    out.push((a, b, *d));
                }
            }
        }
        out
    }

    /// Build the law's own forces against these inputs.
    pub(crate) fn law_forces(&self, law: PhysicsLaw, sources: LawSources) -> Vec<Box<dyn Force>> {
        match law {
            PhysicsLaw::Springs => vec![
                Box::new(NodeExclusion::default()),
                Box::new(EdgeSpring::default()),
                Box::new(Boundary::default()),
            ],
            PhysicsLaw::Charge => vec![
                Box::new(BarnesHutRepulsion {
                    strength: CHARGE_STRENGTH,
                    ..BarnesHutRepulsion::default()
                }),
                Box::new(EdgeSpring::default()),
                Box::new(Boundary::default()),
            ],
            PhysicsLaw::Stress => vec![
                Box::new(NodeExclusion::default()),
                Box::new(StressSpring::from_weighted_distances(
                    self.weighted_distances(),
                    EdgeSpring::default().rest_length,
                )),
                Box::new(Boundary::default()),
            ],
            PhysicsLaw::Energy => vec![
                Box::new(NodeExclusion::default()),
                Box::new(LinLogForce::default()),
            ],
            PhysicsLaw::Orbit => vec![
                Box::new(NodeExclusion::default()),
                Box::new(Gravity::new(self.masses(sources.mass))),
            ],
            PhysicsLaw::Kinds => {
                let (kinds, kind_count) = self.kinds(sources.kind);
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
                let radii = self.nodes.iter().map(|&key| (key, 240.0));
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
            // Held, not empty: with no force at all rapier's contact solver
            // blasts an overlapping seed apart (the Still receipt found it).
            PhysicsLaw::Still => vec![Box::new(Hold)],
        }
    }

    /// Build one overlay's force against these inputs.
    pub(crate) fn overlay_force(
        &self,
        overlay: PhysicsOverlay,
        sources: LawSources,
    ) -> Box<dyn Force> {
        match overlay {
            PhysicsOverlay::DegreeRepulsion => match sources.mass {
                PhysicsMassSource::Degree => Box::new(DegreeRepulsion::default()),
                PhysicsMassSource::PageRank => {
                    Box::new(DegreeRepulsion::default().with_weights(self.page_rank_weights()))
                }
            },
            PhysicsOverlay::DomainCluster => Box::new(DomainCluster::new(self.site_groups())),
            PhysicsOverlay::HubGravity => match sources.mass {
                PhysicsMassSource::Degree => Box::new(HubGravity::default()),
                PhysicsMassSource::PageRank => {
                    Box::new(HubGravity::default().with_weights(self.page_rank_weights()))
                }
            },
            PhysicsOverlay::DepthGravity => {
                Box::new(DepthGravity::new(self.depths(sources.depth, sources.focus)))
            }
            PhysicsOverlay::GridSnap => Box::new(GridSnap::default()),
            PhysicsOverlay::GravityLocus => Box::new(GravityLocus::at((0.0, 0.0))),
            PhysicsOverlay::Tide => Box::new(GravityLocus::tidal((0.0, 0.0), 240.0, 24.0)),
            PhysicsOverlay::Skeleton => Box::new(
                StressSpring::from_distances(
                    self.skeleton_edges().into_iter().map(|(a, b)| (a, b, 1)),
                    EdgeSpring::default().rest_length,
                )
                .with_stiffness(SKELETON_STIFFNESS),
            ),
        }
    }

    /// The whole force set: the law, then the overlays in order.
    pub(crate) fn forces(
        &self,
        law: PhysicsLaw,
        overlays: &[PhysicsOverlay],
        sources: LawSources,
    ) -> Vec<Box<dyn Force>> {
        let mut forces = self.law_forces(law, sources);
        forces.extend(
            overlays
                .iter()
                .map(|overlay| self.overlay_force(*overlay, sources)),
        );
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

    /// Where Orbit's masses and the hub overlays' weights come from.
    pub fn physics_mass_source(&self) -> PhysicsMassSource {
        self.physics_mass_source
    }

    /// Where the Depth overlay reads a node's depth from.
    pub fn physics_depth_source(&self) -> PhysicsDepthSource {
        self.physics_depth_source
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

    /// Choose where masses and hub weights come from; rebuilds only if Orbit
    /// or a weighted overlay is live.
    pub fn set_physics_mass_source(&mut self, source: PhysicsMassSource) {
        self.physics_mass_source = source;
        if self.physics_law == PhysicsLaw::Orbit
            || self.physics_overlays.iter().any(|o| o.weighted())
        {
            self.rebuild_law_forces();
            self.settle_for_law();
        }
    }

    /// Choose where the Depth overlay reads depth from; rebuilds only if it is live.
    pub fn set_physics_depth_source(&mut self, source: PhysicsDepthSource) {
        self.physics_depth_source = source;
        if self
            .physics_overlays
            .contains(&PhysicsOverlay::DepthGravity)
        {
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
        self.physics_law.graph_bound()
            || self.physics_overlays.iter().any(|o| o.graph_bound())
            || (self.physics_mass_source == PhysicsMassSource::PageRank
                && self.physics_overlays.iter().any(|o| o.weighted()))
    }

    /// Whether the live law or an overlay keeps the graph moving on its own.
    pub fn physics_never_rests(&self) -> bool {
        self.physics_law.never_rests() || self.physics_overlays.iter().any(|o| o.never_rests())
    }

    /// The sources the next build reads through.
    fn law_sources(&self) -> LawSources {
        LawSources {
            kind: self.physics_kind_source,
            mass: self.physics_mass_source,
            depth: self.physics_depth_source,
            focus: self.focused_key(),
        }
    }

    /// Rebuild the law + overlay force set against the current graph and hand it
    /// to the physics backend. Position-preserving.
    pub(crate) fn rebuild_law_forces(&mut self) {
        let wants_clusters = self.physics_law == PhysicsLaw::Kinds
            && self.physics_kind_source == PhysicsKindSource::Cluster;
        if wants_clusters {
            self.ensure_community_fresh();
        }
        let sources = self.law_sources();
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
            inputs.forces(self.physics_law, &self.physics_overlays, sources)
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

    /// The node bodies' kinetic energy right now (inline backend; zero offloaded).
    pub fn physics_energy(&self) -> f32 {
        self.physics.kinetic_energy()
    }

    /// The layout's signature numbers: energy, spread, overlaps, stretch.
    pub fn layout_stats(&self) -> LayoutStats {
        let positions: Vec<(NodeKey, euclid::default::Point2D<f32>)> =
            self.view.positions().collect();
        let n = positions.len();
        if n == 0 {
            return LayoutStats {
                energy: self.physics_energy(),
                ..LayoutStats::default()
            };
        }
        let centroid = positions
            .iter()
            .fold(euclid::default::Vector2D::<f32>::zero(), |acc, (_, p)| {
                acc + p.to_vector()
            })
            / n as f32;
        let spread = (positions
            .iter()
            .map(|(_, p)| (p.to_vector() - centroid).square_length())
            .sum::<f32>()
            / n as f32)
            .sqrt();
        let diameter = 2.0 * crate::NODE_HALF;
        let mut overlaps = 0;
        for i in 0..n {
            for j in (i + 1)..n {
                if (positions[i].1 - positions[j].1).length() < diameter {
                    overlaps += 1;
                }
            }
        }
        let at: HashMap<NodeKey, euclid::default::Point2D<f32>> = positions.into_iter().collect();
        let edges = visible_relation_edges(&self.graph, &self.hidden_edges);
        let lengths: Vec<f32> = edges
            .iter()
            .filter_map(|(a, b)| Some((*at.get(a)? - *at.get(b)?).length()))
            .collect();
        let stretch = if lengths.is_empty() {
            0.0
        } else {
            let mean_edge = lengths.iter().sum::<f32>() / lengths.len() as f32;
            let keys: Vec<NodeKey> = at.keys().copied().collect();
            // The graph-farthest connected pair, by BFS from every node.
            let mut farthest: Option<(NodeKey, NodeKey, u32)> = None;
            for (a, b, hops) in seiche::graph_distances(keys.iter().copied(), &edges) {
                if farthest.is_none_or(|(_, _, best)| hops > best) {
                    farthest = Some((a, b, hops));
                }
            }
            match farthest {
                Some((a, b, _)) if mean_edge > 0.0 => (at[&a] - at[&b]).length() / mean_edge,
                _ => 0.0,
            }
        };
        LayoutStats {
            energy: self.physics_energy(),
            spread,
            overlaps,
            stretch,
        }
    }

    /// The number of forces in the live law slot (inline backend only). Test introspection.
    #[cfg(test)]
    pub(crate) fn law_force_count(&self) -> usize {
        self.physics.force_count()
    }

    /// The attribute builders over the current graph. Test introspection.
    #[cfg(test)]
    pub(crate) fn law_inputs(&self) -> LawInputs<'_> {
        LawInputs::new(&self.graph, &self.hidden_edges, None)
    }
}
