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

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

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
    /// Iterative similarity-driven projection (no built-in members since the
    /// `SemanticEdgeWeight` projection was retired; kept for external mods).
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

pub mod dyn_layout;
pub use dyn_layout::*;

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
}

fn tags(slice: &[&str]) -> Vec<String> {
    slice.iter().map(|s| s.to_string()).collect()
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

#[cfg(test)]
mod tests;
