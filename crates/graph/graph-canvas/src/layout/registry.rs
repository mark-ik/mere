/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Layout registry — a pluggable-mod catalog of every `Layout<N>` impl
//! available to the host.
//!
//! Hosts surface the registry as a user-visible picker: built-in layouts
//! ship pre-registered, and (future) third-party layouts register
//! alongside them on the same footing. Each registered layout carries a
//! stable URN id, human-visible metadata (display name, category, tags,
//! recommended node count), and a factory for creating fresh instances
//! of the layout + its default state.
//!
//! The registry uses dynamic dispatch via [`DynLayout`] — an
//! object-safe shim over [`Layout`] with the associated `State` type
//! erased to `Box<dyn Any + Send>`. This enables both built-in and
//! third-party layouts to coexist behind one trait object.
//!
//! See [2026-04-19_layouts_as_pluggable_mods_plan.md](../../../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/graph/2026-04-19_layouts_as_pluggable_mods_plan.md).

use std::any::Any;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;

use euclid::default::Vector2D;
use serde::{Deserialize, Serialize};

use super::{Layout, LayoutExtras};
use crate::camera::CanvasViewport;
use crate::scene::CanvasSceneInput;

/// URN-style layout identifier. Format: `<namespace>:<family>[:<variant>]`.
/// Examples: `graph_layout:force_directed`, `graph_layout:penrose`,
/// `graph_layout:lsystem:hilbert`, `mod:acme:butterfly`.
///
/// The id is the persistence key; changing a layout's id is a breaking
/// migration. Config schema can evolve independently.
pub type LayoutId = String;

/// High-level category for the layout picker UI and for recommendation
/// logic. Hosts group layouts visually by category; users switch between
/// layouts within a category more often than across.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayoutCategory {
    /// Force-based iterative physics. Examples: FR and Barnes-Hut.
    Force,
    /// Iterative similarity-driven projection. Example: SemanticEdgeWeight.
    Projection,
    /// Stateless positional / structural layouts. Examples: Grid,
    /// Radial, Phyllotaxis, Penrose, L-system, Timeline, Kanban,
    /// SemanticEmbedding.
    Positional,
    /// Composition passes applied alongside a primary layout. Examples:
    /// DegreeRepulsion, DomainClustering, SemanticClustering, HubPull,
    /// FrameAffinity.
    Extras,
}

/// Where a registered layout originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayoutProvenance {
    /// Ships in graph-canvas itself.
    Builtin,
    /// Loaded from a compiled native Rust mod at the host level.
    NativeMod,
    /// Loaded from a WASM guest through the pluggable-mods / WASM runtime
    /// lane (tracked in `2026-04-03_wasm_layout_runtime_plan.md`).
    WasmMod,
}

/// Metadata attached to every registered layout. Drives the picker UI,
/// recommendation / fallback logic, and diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutCapability {
    pub id: LayoutId,
    pub display_name: String,
    pub description: Option<String>,
    pub category: LayoutCategory,
    /// True if the layout produces identical output for identical input
    /// modulo floating-point noise.
    pub is_deterministic: bool,
    /// True if the layout reads graph edges meaningfully. False for pure
    /// positional layouts (Grid, Phyllotaxis).
    pub is_topology_sensitive: bool,
    /// True if the layout can produce meaningful 3D output. All built-ins
    /// are 2D today; reserved for future variants.
    pub supports_3d: bool,
    /// Recommended maximum node count for acceptable perf. `None` is
    /// unbounded / not-measured.
    pub recommended_max_node_count: Option<usize>,
    pub provenance: LayoutProvenance,
    /// Free-form tags for filtering. Examples: `"spatial-memory"`,
    /// `"semantic"`, `"time-axis"`, `"hierarchical"`, `"organic"`.
    pub capability_tags: Vec<String>,
}

// ── DynLayout — object-safe Layout ───────────────────────────────────────────

/// Erased state for layouts stored behind a trait object. Concrete
/// `Layout::State` types erase to `Box<dyn Any + Send>`; the blanket
/// [`DynLayout`] impl downcasts back on each call.
pub type ErasedState = Box<dyn Any + Send>;

/// Object-safe analogue of [`Layout`]. Every concrete `Layout<N>` whose
/// `State` is `Any + Default + Send` gets a blanket `DynLayout<N>` impl.
pub trait DynLayout<N: Clone + Eq + Hash + Send>: Send {
    fn step_dyn(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut ErasedState,
        dt: f32,
        viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>>;

    fn is_converged_dyn(&self, state: &ErasedState) -> bool;

    fn default_state_erased(&self) -> ErasedState;
}

impl<N, L> DynLayout<N> for L
where
    N: Clone + Eq + Hash + Send + 'static,
    L: Layout<N> + Send,
    L::State: Any + Default + Send,
{
    fn step_dyn(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut ErasedState,
        dt: f32,
        viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        let state_typed = state
            .downcast_mut::<L::State>()
            .expect("DynLayout: state type mismatch for this provider");
        self.step(scene, state_typed, dt, viewport, extras)
    }

    fn is_converged_dyn(&self, state: &ErasedState) -> bool {
        state
            .downcast_ref::<L::State>()
            .map(|s| self.is_converged(s))
            .unwrap_or(false)
    }

    fn default_state_erased(&self) -> ErasedState {
        Box::new(L::State::default())
    }
}

// ── Providers ────────────────────────────────────────────────────────────────

/// A producer of a particular layout. Hosts register providers; users
/// select a layout by id; the registry resolves to a provider; the
/// provider creates a fresh layout + state pair.
pub trait LayoutProvider<N: Clone + Eq + Hash + Send + 'static>: Send + Sync {
    fn capability(&self) -> LayoutCapability;
    /// Construct a fresh layout instance using the provider's default
    /// configuration. Hosts that want custom config construct concrete
    /// types directly and bypass the registry.
    fn create_default(&self) -> Box<dyn DynLayout<N>>;
}

/// A zero-sized built-in provider parameterized by the layout type `L`
/// and a capability-builder function. Used to register every built-in
/// layout with one line each.
pub struct BuiltinProvider<L, N>
where
    L: Default + Layout<N> + Send + 'static,
    L::State: Any + Default + Send,
    N: Clone + Eq + Hash + Send + 'static,
{
    capability_fn: fn() -> LayoutCapability,
    _layout: PhantomData<fn() -> L>,
    _node: PhantomData<fn() -> N>,
}

impl<L, N> BuiltinProvider<L, N>
where
    L: Default + Layout<N> + Send + 'static,
    L::State: Any + Default + Send,
    N: Clone + Eq + Hash + Send + 'static,
{
    pub const fn new(capability_fn: fn() -> LayoutCapability) -> Self {
        Self {
            capability_fn,
            _layout: PhantomData,
            _node: PhantomData,
        }
    }
}

impl<L, N> LayoutProvider<N> for BuiltinProvider<L, N>
where
    L: Default + Layout<N> + Send + 'static,
    L::State: Any + Default + Send,
    N: Clone + Eq + Hash + Send + 'static,
{
    fn capability(&self) -> LayoutCapability {
        (self.capability_fn)()
    }

    fn create_default(&self) -> Box<dyn DynLayout<N>> {
        Box::new(L::default())
    }
}

// ── Registry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum RegisterError {
    /// `LayoutId` was empty or all whitespace.
    InvalidId(String),
    /// A provider with this id is already registered. Unregister first
    /// if replacement is intended.
    DuplicateId(LayoutId),
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId(id) => write!(f, "invalid layout id: {id:?}"),
            Self::DuplicateId(id) => write!(f, "layout id already registered: {id:?}"),
        }
    }
}

impl std::error::Error for RegisterError {}

/// Catalog of layout providers keyed by [`LayoutId`].
///
/// `Default` registers every built-in layout. Hosts can then
/// [`register`](Self::register) additional providers (native mods, WASM
/// mods) on top, or [`unregister`](Self::unregister) built-ins they
/// don't want to surface.
pub struct LayoutRegistry<N: Clone + Eq + Hash + Send + 'static> {
    providers: HashMap<LayoutId, Arc<dyn LayoutProvider<N>>>,
}

impl<N: Clone + Eq + Hash + Send + 'static> LayoutRegistry<N> {
    /// Construct an empty registry with no providers.
    pub fn empty() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Add a provider. Returns an error if the id is empty or already
    /// registered.
    pub fn register(&mut self, provider: Arc<dyn LayoutProvider<N>>) -> Result<(), RegisterError> {
        let capability = provider.capability();
        let id = capability.id;
        if id.trim().is_empty() {
            return Err(RegisterError::InvalidId(id));
        }
        if self.providers.contains_key(&id) {
            return Err(RegisterError::DuplicateId(id));
        }
        self.providers.insert(id, provider);
        Ok(())
    }

    /// Remove a provider by id. Returns true if one was present.
    pub fn unregister(&mut self, id: &str) -> bool {
        self.providers.remove(id).is_some()
    }

    /// Look up a provider by id without cloning metadata.
    pub fn resolve(&self, id: &str) -> Option<Arc<dyn LayoutProvider<N>>> {
        self.providers.get(id).cloned()
    }

    /// Iterate capabilities of every registered provider.
    pub fn capabilities(&self) -> Vec<LayoutCapability> {
        self.providers.values().map(|p| p.capability()).collect()
    }

    /// Capabilities filtered by exact tag match.
    pub fn filter_by_tag(&self, tag: &str) -> Vec<LayoutCapability> {
        self.providers
            .values()
            .map(|p| p.capability())
            .filter(|cap| cap.capability_tags.iter().any(|t| t == tag))
            .collect()
    }

    /// Capabilities filtered by category.
    pub fn filter_by_category(&self, category: LayoutCategory) -> Vec<LayoutCapability> {
        self.providers
            .values()
            .map(|p| p.capability())
            .filter(|cap| cap.category == category)
            .collect()
    }

    /// Capabilities filtered by provenance.
    pub fn filter_by_provenance(&self, provenance: LayoutProvenance) -> Vec<LayoutCapability> {
        self.providers
            .values()
            .map(|p| p.capability())
            .filter(|cap| cap.provenance == provenance)
            .collect()
    }

    /// Number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

// ── Built-in registrations ───────────────────────────────────────────────────

impl<N> Default for LayoutRegistry<N>
where
    N: Clone + Eq + Hash + Send + Ord + 'static,
{
    fn default() -> Self {
        let mut registry = Self::empty();
        register_builtins::<N>(&mut registry);
        registry
    }
}

/// Register every built-in layout provider. Called by [`LayoutRegistry::default`].
/// Public so hosts that construct empty registries can opt back in.
///
pub fn register_builtins<N>(registry: &mut LayoutRegistry<N>)
where
    N: Clone + Eq + Hash + Send + Ord + 'static,
{
    // Force-based iterative layouts.
    let _ = registry.register(Arc::new(BuiltinProvider::<super::ForceDirected, N>::new(
        force_directed_capability,
    )));
    let _ = registry.register(Arc::new(BuiltinProvider::<super::BarnesHut, N>::new(
        barnes_hut_capability,
    )));

    // Projection (similarity-driven iterative).
    let _ = registry.register(Arc::new(
        BuiltinProvider::<super::SemanticEdgeWeight, N>::new(semantic_edge_weight_capability),
    ));

    // Positional layouts (stateless / delta-to-target).
    let _ = registry.register(Arc::new(BuiltinProvider::<super::Grid, N>::new(
        grid_capability,
    )));
    let _ = registry.register(Arc::new(BuiltinProvider::<super::Radial<N>, N>::new(
        radial_capability,
    )));
    let _ = registry.register(Arc::new(BuiltinProvider::<super::Phyllotaxis, N>::new(
        phyllotaxis_capability,
    )));
    let _ = registry.register(Arc::new(BuiltinProvider::<super::Timeline, N>::new(
        timeline_capability,
    )));
    let _ = registry.register(Arc::new(BuiltinProvider::<super::Kanban, N>::new(
        kanban_capability,
    )));
    let _ = registry.register(Arc::new(BuiltinProvider::<super::Penrose, N>::new(
        penrose_capability,
    )));
    let _ = registry.register(Arc::new(BuiltinProvider::<super::LSystem, N>::new(
        l_system_capability,
    )));
    let _ = registry.register(Arc::new(
        BuiltinProvider::<super::SemanticEmbedding, N>::new(semantic_embedding_capability),
    ));

    // Extras / composition passes.
    let _ = registry.register(Arc::new(BuiltinProvider::<super::DegreeRepulsion, N>::new(
        degree_repulsion_capability,
    )));
    let _ = registry.register(Arc::new(
        BuiltinProvider::<super::DomainClustering<N>, N>::new(domain_clustering_capability),
    ));
    let _ = registry.register(Arc::new(
        BuiltinProvider::<super::SemanticClustering, N>::new(semantic_clustering_capability),
    ));
    let _ = registry.register(Arc::new(BuiltinProvider::<super::HubPull, N>::new(
        hub_pull_capability,
    )));
    let _ = registry.register(Arc::new(BuiltinProvider::<super::FrameAffinity, N>::new(
        frame_affinity_capability,
    )));
}

fn tags(slice: &[&str]) -> Vec<String> {
    slice.iter().map(|s| s.to_string()).collect()
}

fn force_directed_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:force_directed".into(),
        display_name: "Force Directed".into(),
        description: Some(
            "Fruchterman–Reingold with center gravity. Classic organic layout.".into(),
        ),
        category: LayoutCategory::Force,
        is_deterministic: true,
        is_topology_sensitive: true,
        supports_3d: false,
        recommended_max_node_count: Some(500),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["force", "physics", "organic", "iterative"]),
    }
}

fn barnes_hut_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:force_directed_barnes_hut".into(),
        display_name: "Force Directed (Barnes-Hut)".into(),
        description: Some(
            "O(n log n) quadtree-accelerated force-directed layout. Use at scale.".into(),
        ),
        category: LayoutCategory::Force,
        is_deterministic: true,
        is_topology_sensitive: true,
        supports_3d: false,
        recommended_max_node_count: Some(5_000),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["force", "physics", "organic", "iterative", "scale"]),
    }
}

fn semantic_edge_weight_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:semantic_edge_weight".into(),
        display_name: "Semantic Edge Weight".into(),
        description: Some(
            "Iterative projection driven by pairwise semantic similarity. Below real UMAP/t-SNE quality; no ML pipeline required."
                .into(),
        ),
        category: LayoutCategory::Projection,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: Some(400),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["semantic", "iterative", "projection"]),
    }
}

fn grid_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:grid".into(),
        display_name: "Grid".into(),
        description: Some("Row-major grid with configurable traversal.".into()),
        category: LayoutCategory::Positional,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: None,
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["structured", "positional", "snap"]),
    }
}

fn radial_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:radial".into(),
        display_name: "Radial".into(),
        description: Some("BFS rings around a focal node.".into()),
        category: LayoutCategory::Positional,
        is_deterministic: true,
        is_topology_sensitive: true,
        supports_3d: false,
        recommended_max_node_count: Some(1_000),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["radial", "hierarchical", "focus", "positional"]),
    }
}

fn phyllotaxis_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:phyllotaxis".into(),
        display_name: "Phyllotaxis".into(),
        description: Some(
            "Fibonacci-family spiral placement. Golden angle by default; configurable for other arm counts."
                .into(),
        ),
        category: LayoutCategory::Positional,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: None,
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["spiral", "positional", "organic", "priority-queue"]),
    }
}

fn timeline_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:timeline".into(),
        display_name: "Timeline".into(),
        description: Some(
            "Numeric x-axis placement driven by a host-provided time coordinate.".into(),
        ),
        category: LayoutCategory::Positional,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: None,
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["time-axis", "axial", "positional", "temporal"]),
    }
}

fn kanban_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:kanban".into(),
        display_name: "Kanban".into(),
        description: Some("Categorical column bucketing by host-provided tag.".into()),
        category: LayoutCategory::Positional,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: None,
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["bucketed", "axial", "positional", "workflow"]),
    }
}

fn penrose_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:penrose".into(),
        display_name: "Penrose".into(),
        description: Some(
            "Aperiodic tiling (P2 kite-dart or P3 rhombus) via Robinson subdivision.".into(),
        ),
        category: LayoutCategory::Positional,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: Some(2_000),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["aperiodic", "fractal", "positional", "spatial-memory"]),
    }
}

fn l_system_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:lsystem".into(),
        display_name: "L-System Fractal Path".into(),
        description: Some("Turtle-walked Lindenmayer grammar (Hilbert, Koch, or Dragon).".into()),
        category: LayoutCategory::Positional,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: Some(4_000),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["fractal", "space-filling", "positional", "locality"]),
    }
}

fn semantic_embedding_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:semantic_embedding".into(),
        display_name: "Semantic Embedding".into(),
        description: Some(
            "Places nodes at host-precomputed 2D embeddings (UMAP / t-SNE / PCA supplied by the host's ML pipeline)."
                .into(),
        ),
        category: LayoutCategory::Positional,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: None,
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["semantic", "precomputed", "positional", "ml"]),
    }
}

fn degree_repulsion_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:extras:degree_repulsion".into(),
        display_name: "Degree Repulsion".into(),
        description: Some(
            "Post-physics pass: high-degree nodes push their neighbors apart.".into(),
        ),
        category: LayoutCategory::Extras,
        is_deterministic: true,
        is_topology_sensitive: true,
        supports_3d: false,
        recommended_max_node_count: Some(500),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["extras", "force", "composable"]),
    }
}

fn domain_clustering_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:extras:domain_clustering".into(),
        display_name: "Domain Clustering".into(),
        description: Some(
            "Post-physics pass: same-domain nodes pulled toward a shared target.".into(),
        ),
        category: LayoutCategory::Extras,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: Some(500),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["extras", "clustering", "composable"]),
    }
}

fn semantic_clustering_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:extras:semantic_clustering".into(),
        display_name: "Semantic Clustering".into(),
        description: Some("Post-physics pass: semantically similar nodes pulled together.".into()),
        category: LayoutCategory::Extras,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: Some(500),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["extras", "semantic", "composable"]),
    }
}

fn hub_pull_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:extras:hub_pull".into(),
        display_name: "Hub Pull".into(),
        description: Some("Post-physics pass: low-degree leaves pulled toward nearby hubs.".into()),
        category: LayoutCategory::Extras,
        is_deterministic: true,
        is_topology_sensitive: true,
        supports_3d: false,
        recommended_max_node_count: Some(500),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["extras", "force", "composable"]),
    }
}

fn frame_affinity_capability() -> LayoutCapability {
    LayoutCapability {
        id: "graph_layout:extras:frame_affinity".into(),
        display_name: "Frame Affinity".into(),
        description: Some(
            "Post-physics pass: frame members pulled toward a per-frame target (centroid / medoid / anchor)."
                .into(),
        ),
        category: LayoutCategory::Extras,
        is_deterministic: true,
        is_topology_sensitive: false,
        supports_3d: false,
        recommended_max_node_count: Some(500),
        provenance: LayoutProvenance::Builtin,
        capability_tags: tags(&["extras", "grouping", "composable"]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Id = u32;

    #[test]
    fn default_registry_includes_force_directed() {
        let registry = LayoutRegistry::<Id>::default();
        let resolved = registry
            .resolve("graph_layout:force_directed")
            .expect("FR must be present in default registry");
        let capability = resolved.capability();
        assert_eq!(capability.category, LayoutCategory::Force);
        assert_eq!(capability.provenance, LayoutProvenance::Builtin);
    }

    #[test]
    fn default_registry_surfaces_all_builtins() {
        let registry = LayoutRegistry::<Id>::default();
        // 16 built-ins: 11 layouts + 5 extras.
        assert_eq!(registry.len(), 16);
    }

    #[test]
    fn filter_by_category_partitions_cleanly() {
        let registry = LayoutRegistry::<Id>::default();
        let force = registry.filter_by_category(LayoutCategory::Force);
        let positional = registry.filter_by_category(LayoutCategory::Positional);
        let extras = registry.filter_by_category(LayoutCategory::Extras);
        let projection = registry.filter_by_category(LayoutCategory::Projection);
        // 2 Force (FR, Barnes-Hut), 8 Positional, 5 Extras, 1 Projection.
        assert!(force.len() >= 2);
        assert_eq!(positional.len(), 8);
        assert_eq!(extras.len(), 5);
        assert_eq!(projection.len(), 1);
    }

    #[test]
    fn filter_by_tag_matches_expected_members() {
        let registry = LayoutRegistry::<Id>::default();
        let organic = registry.filter_by_tag("organic");
        // FR, Barnes-Hut, Phyllotaxis carry the `organic` tag.
        assert!(organic.len() >= 3);
        let fractal = registry.filter_by_tag("fractal");
        // Penrose + L-system.
        assert_eq!(fractal.len(), 2);
    }

    #[test]
    fn filter_by_provenance_is_all_builtin_by_default() {
        let registry = LayoutRegistry::<Id>::default();
        let builtins = registry.filter_by_provenance(LayoutProvenance::Builtin);
        assert_eq!(builtins.len(), registry.len());
        let native_mods = registry.filter_by_provenance(LayoutProvenance::NativeMod);
        assert!(native_mods.is_empty());
    }

    #[test]
    fn register_rejects_empty_id() {
        let mut registry = LayoutRegistry::<Id>::empty();
        struct BadProvider;
        impl LayoutProvider<Id> for BadProvider {
            fn capability(&self) -> LayoutCapability {
                LayoutCapability {
                    id: "".into(),
                    display_name: "x".into(),
                    description: None,
                    category: LayoutCategory::Force,
                    is_deterministic: true,
                    is_topology_sensitive: false,
                    supports_3d: false,
                    recommended_max_node_count: None,
                    provenance: LayoutProvenance::Builtin,
                    capability_tags: vec![],
                }
            }
            fn create_default(&self) -> Box<dyn DynLayout<Id>> {
                Box::new(super::super::ForceDirected::default())
            }
        }
        let err = registry
            .register(Arc::new(BadProvider))
            .expect_err("empty id must be rejected");
        assert!(matches!(err, RegisterError::InvalidId(_)));
    }

    #[test]
    fn register_rejects_duplicate_id() {
        let mut registry = LayoutRegistry::<Id>::default();
        let provider = Arc::new(BuiltinProvider::<super::super::ForceDirected, Id>::new(
            force_directed_capability,
        ));
        let err = registry
            .register(provider)
            .expect_err("duplicate FR registration must fail");
        assert!(matches!(err, RegisterError::DuplicateId(_)));
    }

    #[test]
    fn unregister_removes_provider() {
        let mut registry = LayoutRegistry::<Id>::default();
        assert!(registry.unregister("graph_layout:force_directed"));
        assert!(registry.resolve("graph_layout:force_directed").is_none());
    }

    #[test]
    fn resolved_provider_creates_usable_layout() {
        use crate::projection::ProjectionMode;
        use crate::scene::{CanvasNode, SceneMode, ViewId};
        use euclid::default::{Point2D, Rect, Size2D};

        let registry = LayoutRegistry::<Id>::default();
        let provider = registry
            .resolve("graph_layout:grid")
            .expect("grid must be registered");

        let capability = provider.capability();
        assert_eq!(capability.category, LayoutCategory::Positional);

        let mut layout = provider.create_default();
        let mut state = layout.default_state_erased();
        let viewport = CanvasViewport {
            rect: Rect::new(Point2D::new(0.0, 0.0), Size2D::new(1000.0, 1000.0)),
            scale_factor: 1.0,
        };
        let scene = CanvasSceneInput::<Id> {
            view_id: ViewId(0),
            nodes: (0..4u32)
                .map(|id| CanvasNode {
                    id,
                    position: Point2D::new(500.0 + id as f32 * 10.0, 500.0),
                    radius: 16.0,
                    label: None,
                })
                .collect(),
            edges: vec![],
            scene_objects: vec![],
            overlays: vec![],
            scene_mode: SceneMode::Browse,
            projection: ProjectionMode::default(),
        };
        let deltas = layout.step_dyn(&scene, &mut state, 0.0, &viewport, &LayoutExtras::default());
        assert!(!deltas.is_empty(), "grid should produce deltas");
    }
}
