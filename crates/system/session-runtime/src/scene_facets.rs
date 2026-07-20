// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `scene.*` facet namespace — the graph-scene's own view settings as
//! facets of the **container node**.
//!
//! The atomic-facets ruling reaches the graph-scoped settings the bespoke
//! cartography geometry carried alongside the per-node data: `size_by_degree`,
//! `size_by_importance`, `importance_metric`, and the physics `damping` are not
//! per-node (they describe the whole scene), so they are not [`arrangement`]
//! facets. But they are not homeless view-settings needing a bespoke sidecar
//! either: in the one-node model the graph **is** a container node, so these
//! are facets of that container — `scene.*`, keyed by the container's id (the
//! session's `root_graph_id`), in the same [`facets.json`](crate::facet_store).
//! One mechanism, keyed by node id, whether the node is a leaf or the container.
//!
//! Unlike [`arrangement`] facets (many leaf nodes, one facet each), `scene.*`
//! is one node (the container) carrying several facets — so the API is a
//! per-container bundle ([`SceneFacets`]) read/written whole, while on the wire
//! each field is still its own atomic, independently addressable facet (a
//! modder can read `scene.physics_damping` alone).
//!
//! [`arrangement`]: crate::arrangement_facets

use serde_json::{Value, json};
use uuid::Uuid;

use crate::facet_store::{AcceptAll, FacetId, NodeFacetStore};

/// Facet id: whether faces grow with undirected degree. Payload: a bool.
pub const SCENE_SIZE_BY_DEGREE: &str = "scene.size_by_degree";

/// Facet id: whether faces grow with the graph-signals importance. Payload: a bool.
pub const SCENE_SIZE_BY_IMPORTANCE: &str = "scene.size_by_importance";

/// Facet id: the importance metric code (`degree` / `betweenness`; empty reads
/// as `degree`). Payload: a string. The canvas owns the vocabulary
/// (`ImportanceMetric::from_code`).
pub const SCENE_IMPORTANCE_METRIC: &str = "scene.importance_metric";

/// Facet id: the layout's linear damping (the "inertia" physics setting).
/// Payload: a number. This is where physics damping lives now — it left the
/// app-wide settings store, being scene-scoped, not app-scoped.
pub const SCENE_PHYSICS_DAMPING: &str = "scene.physics_damping";

/// The layout engine's tuned default linear damping (mirrors seiche's
/// `DEFAULT_LINEAR_DAMPING`); the seed when no `scene.physics_damping` facet
/// is persisted.
pub const DEFAULT_PHYSICS_DAMPING: f32 = 2.5;

/// The container node's scene settings, bundled — the host reads/writes the
/// whole set, though each field persists as its own `scene.*` facet.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneFacets {
    pub size_by_degree: bool,
    pub size_by_importance: bool,
    /// The importance metric code; empty string reads as `degree`.
    pub importance_metric: String,
    pub physics_damping: f32,
}

impl Default for SceneFacets {
    fn default() -> Self {
        Self {
            size_by_degree: false,
            size_by_importance: false,
            importance_metric: String::new(),
            physics_damping: DEFAULT_PHYSICS_DAMPING,
        }
    }
}

/// Write the container's scene settings as `scene.*` facets under `container`
/// (the session's `root_graph_id`). Each field is set through [`AcceptAll`]
/// (the shapes are fixed here). Other facets on the container — and every leaf
/// node's facets — are untouched.
pub fn write_scene_facets(store: &mut NodeFacetStore, container: Uuid, scene: &SceneFacets) {
    let set = |store: &mut NodeFacetStore, id: &str, payload: Value| {
        let _ = store.set(container, FacetId::new(id), payload, &AcceptAll);
    };
    set(store, SCENE_SIZE_BY_DEGREE, json!(scene.size_by_degree));
    set(store, SCENE_SIZE_BY_IMPORTANCE, json!(scene.size_by_importance));
    set(store, SCENE_IMPORTANCE_METRIC, json!(scene.importance_metric));
    set(store, SCENE_PHYSICS_DAMPING, json!(scene.physics_damping));
}

/// Read the container's scene settings from its `scene.*` facets, each field
/// falling back to its [`SceneFacets::default`] when the facet is absent or
/// malformed (one bad or missing facet must not lose the rest of the scene).
pub fn read_scene_facets(store: &NodeFacetStore, container: Uuid) -> SceneFacets {
    let Some(facets) = store.facets_of(&container) else {
        return SceneFacets::default();
    };
    let default = SceneFacets::default();
    let bool_of = |id: &str, fallback: bool| {
        facets
            .get(&FacetId::new(id))
            .and_then(Value::as_bool)
            .unwrap_or(fallback)
    };
    let damping = facets
        .get(&FacetId::new(SCENE_PHYSICS_DAMPING))
        .and_then(Value::as_f64)
        .map(|d| d as f32)
        .filter(|d| d.is_finite())
        .unwrap_or(default.physics_damping);
    let metric = facets
        .get(&FacetId::new(SCENE_IMPORTANCE_METRIC))
        .and_then(Value::as_str)
        .unwrap_or(&default.importance_metric)
        .to_string();
    SceneFacets {
        size_by_degree: bool_of(SCENE_SIZE_BY_DEGREE, default.size_by_degree),
        size_by_importance: bool_of(SCENE_SIZE_BY_IMPORTANCE, default.size_by_importance),
        importance_metric: metric,
        physics_damping: damping,
    }
}

/// Carry the donor container's `scene.*` facets onto the fork's container —
/// the scene half of the fork's facet-carry (tear-out G4-R R1; the per-node
/// half is `facet_store::copy_node_facets`). The fork opens with the donor's
/// sizing mode + metric + damping. Reads through [`read_scene_facets`] (so a
/// donor with no scene facets writes the defaults) and writes whole.
pub fn copy_scene_facets(
    donor: &NodeFacetStore,
    fork: &mut NodeFacetStore,
    donor_container: Uuid,
    fork_container: Uuid,
) {
    let scene = read_scene_facets(donor, donor_container);
    write_scene_facets(fork, fork_container, &scene);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_round_trips() {
        let mut store = NodeFacetStore::new();
        let container = Uuid::from_u128(0x9);
        let scene = SceneFacets {
            size_by_degree: true,
            size_by_importance: false,
            importance_metric: "betweenness".to_string(),
            physics_damping: 4.0,
        };
        write_scene_facets(&mut store, container, &scene);
        assert_eq!(read_scene_facets(&store, container), scene);
    }

    #[test]
    fn absent_container_reads_defaults() {
        let store = NodeFacetStore::new();
        assert_eq!(
            read_scene_facets(&store, Uuid::from_u128(0x9)),
            SceneFacets::default()
        );
    }

    #[test]
    fn a_malformed_field_falls_back_to_its_default_not_the_whole_bundle() {
        let mut store = NodeFacetStore::new();
        let container = Uuid::from_u128(0x9);
        // A good degree flag, a garbage damping.
        store
            .set(container, FacetId::new(SCENE_SIZE_BY_DEGREE), json!(true), &AcceptAll)
            .unwrap();
        store
            .set(container, FacetId::new(SCENE_PHYSICS_DAMPING), json!("nope"), &AcceptAll)
            .unwrap();
        let read = read_scene_facets(&store, container);
        assert!(read.size_by_degree, "the good field survives");
        assert_eq!(
            read.physics_damping, DEFAULT_PHYSICS_DAMPING,
            "the garbage damping falls back to the default"
        );
    }

    #[test]
    fn copy_scene_facets_carries_donor_settings_to_the_fork_container() {
        let mut donor = NodeFacetStore::new();
        let donor_container = Uuid::from_u128(0x9);
        let fork_container = Uuid::from_u128(0x99);
        write_scene_facets(
            &mut donor,
            donor_container,
            &SceneFacets {
                size_by_degree: true,
                physics_damping: 6.25,
                ..SceneFacets::default()
            },
        );
        let mut fork = NodeFacetStore::new();
        copy_scene_facets(&donor, &mut fork, donor_container, fork_container);
        let carried = read_scene_facets(&fork, fork_container);
        assert!(carried.size_by_degree);
        assert_eq!(carried.physics_damping, 6.25);
        assert!(
            fork.facets_of(&donor_container).is_none(),
            "the fork keys scene facets by ITS container, not the donor's"
        );
    }

    #[test]
    fn scene_facets_do_not_collide_with_a_leaf_nodes_facets() {
        // The container id and a leaf id are distinct keys in one store.
        let mut store = NodeFacetStore::new();
        let container = Uuid::from_u128(0x9);
        let leaf = Uuid::from_u128(0x1);
        write_scene_facets(
            &mut store,
            container,
            &SceneFacets {
                physics_damping: 7.0,
                ..SceneFacets::default()
            },
        );
        crate::arrangement_facets::write_arrangement_positions(&mut store, [(leaf, (5.0, 6.0))]);
        assert_eq!(read_scene_facets(&store, container).physics_damping, 7.0);
        assert_eq!(
            crate::arrangement_facets::read_arrangement_positions(&store),
            vec![(leaf, (5.0, 6.0))]
        );
    }
}
