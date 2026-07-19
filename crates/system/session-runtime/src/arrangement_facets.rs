// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `arrangement.*` facet namespace — cartography's per-node data as facets.
//!
//! The facet-convergence ruling (design doc
//! `2026-07-18_node_dissolution_facets_plan.md`, "Facet convergence of the
//! per-node sidecars"): cartography's per-node arrangement data (position,
//! size, sprite, sprite-hull, material, face) is typed metadata keyed by node
//! id, which is the facet store's definition. It persists as `arrangement.*`
//! facets in [`facets.json`](crate::facet_store), not as a bespoke
//! `cartography.json` — and since no host ever wired that bespoke sidecar,
//! the durable layout store is **born** as facets; there is nothing to migrate.
//!
//! This module defines the namespace's facet ids + payload shapes and the
//! read/write helpers a host uses around the canvas seams:
//!
//! - save: `Canvas::cartography_geometry().iter()` →
//!   [`write_arrangement_positions`] → `save_node_facets`.
//! - load: `load_node_facets` → [`read_arrangement_positions`] →
//!   `Canvas::seed_cartography`.
//!
//! Boundaries (per the ruling): the *live* position lives in seiche and is
//! never a facet — only the durable save-time position lands here, so the
//! store holds cold data and the hot loop stays out of JSON. Graph-scoped
//! canvas flags (`size_by_degree`, `importance_metric`, …) are view settings,
//! not per-node facets; they belong to the view-intent/settings stores.
//!
//! `arrangement.position` is the first family member wired end-to-end; size /
//! sprite / sprite-hull / material / face follow the same pattern (one facet
//! id + payload shape + write/read pair each) as they gain persistence.

use std::collections::BTreeSet;

use serde_json::json;
use uuid::Uuid;

use crate::facet_store::{AcceptAll, FacetId, NodeFacetStore};

/// Facet id of a node's durable world position. Payload: `{"x": f32, "y": f32}`
/// (world coordinates, the semantic geometry — not pixels).
pub const ARRANGEMENT_POSITION: &str = "arrangement.position";

/// The [`ARRANGEMENT_POSITION`] facet id, constructed.
pub fn arrangement_position_facet() -> FacetId {
    FacetId::new(ARRANGEMENT_POSITION)
}

/// Replace the store's `arrangement.position` facets with `positions` — the
/// save-time write. Every existing position facet is cleared first, so a node
/// absent from `positions` (deleted, or never placed) carries none afterwards;
/// facets in other namespaces are untouched. Payloads are written through
/// [`AcceptAll`]: the shape is fixed here, so schema validation adds nothing.
pub fn write_arrangement_positions(
    store: &mut NodeFacetStore,
    positions: impl IntoIterator<Item = (Uuid, (f32, f32))>,
) {
    let facet = arrangement_position_facet();
    let stale: Vec<Uuid> = store
        .iter()
        .filter(|(_, facets)| facets.has(&facet))
        .map(|(id, _)| *id)
        .collect();
    for id in stale {
        store.remove(&id, &facet);
    }
    for (id, (x, y)) in positions {
        let _ = store.set(id, facet.clone(), json!({ "x": x, "y": y }), &AcceptAll);
    }
}

/// The store's `arrangement.position` facets as `(node, (x, y))` pairs — the
/// load-time read, shaped for `Canvas::seed_cartography`. A malformed payload
/// (wrong shape, non-finite numbers) is skipped rather than failing the load:
/// one bad facet must not cost the whole session its layout.
pub fn read_arrangement_positions(store: &NodeFacetStore) -> Vec<(Uuid, (f32, f32))> {
    let facet = arrangement_position_facet();
    store
        .iter()
        .filter_map(|(id, facets)| {
            let value = facets.get(&facet)?;
            let x = value.get("x")?.as_f64()? as f32;
            let y = value.get("y")?.as_f64()? as f32;
            (x.is_finite() && y.is_finite()).then_some((*id, (x, y)))
        })
        .collect()
}

/// Drop every facet of every node not in `present` — the load-time reconcile,
/// mirroring the graph-membership retain the bespoke geometry sidecar did. A
/// node deleted between sessions takes its whole facet record with it (all
/// namespaces: its facets describe a node that no longer exists); facets of
/// live nodes — including foreign/mod namespaces — are untouched.
pub fn retain_present_nodes(store: &mut NodeFacetStore, present: &BTreeSet<Uuid>) {
    let departed: Vec<Uuid> = store
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !present.contains(id))
        .collect();
    for id in departed {
        store.remove_node(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_round_trips() {
        let mut store = NodeFacetStore::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        write_arrangement_positions(&mut store, [(a, (10.0, 20.0)), (b, (-30.5, 0.0))]);
        let mut read = read_arrangement_positions(&store);
        read.sort_by_key(|(id, _)| *id);
        assert_eq!(read, vec![(a, (10.0, 20.0)), (b, (-30.5, 0.0))]);
    }

    #[test]
    fn a_rewrite_clears_positions_absent_from_the_new_set() {
        let mut store = NodeFacetStore::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        write_arrangement_positions(&mut store, [(a, (1.0, 1.0)), (b, (2.0, 2.0))]);
        // The next save has no position for b (deleted / unplaced).
        write_arrangement_positions(&mut store, [(a, (5.0, 5.0))]);
        assert_eq!(read_arrangement_positions(&store), vec![(a, (5.0, 5.0))]);
    }

    #[test]
    fn a_rewrite_leaves_other_namespaces_untouched() {
        let mut store = NodeFacetStore::new();
        let a = Uuid::from_u128(1);
        store
            .set(a, FacetId::new("web.viewer"), json!({ "mode": "reader" }), &AcceptAll)
            .unwrap();
        write_arrangement_positions(&mut store, [(a, (3.0, 4.0))]);
        write_arrangement_positions(&mut store, []);
        assert_eq!(
            store.get(&a, &FacetId::new("web.viewer")),
            Some(&json!({ "mode": "reader" })),
            "the position rewrite must not disturb web.* facets"
        );
        assert!(read_arrangement_positions(&store).is_empty());
    }

    #[test]
    fn malformed_and_non_finite_payloads_are_skipped() {
        let mut store = NodeFacetStore::new();
        let good = Uuid::from_u128(1);
        let bad_shape = Uuid::from_u128(2);
        let bad_nan = Uuid::from_u128(3);
        let facet = arrangement_position_facet();
        store
            .set(good, facet.clone(), json!({ "x": 7.0, "y": 8.0 }), &AcceptAll)
            .unwrap();
        store
            .set(bad_shape, facet.clone(), json!([7.0, 8.0]), &AcceptAll)
            .unwrap();
        // JSON has no NaN literal; a null coordinate stands in for the
        // unparsable case (as_f64 fails the same way).
        store
            .set(bad_nan, facet.clone(), json!({ "x": null, "y": 1.0 }), &AcceptAll)
            .unwrap();
        assert_eq!(
            read_arrangement_positions(&store),
            vec![(good, (7.0, 8.0))]
        );
    }

    #[test]
    fn retain_present_drops_departed_nodes_entirely() {
        let mut store = NodeFacetStore::new();
        let live = Uuid::from_u128(1);
        let departed = Uuid::from_u128(2);
        write_arrangement_positions(&mut store, [(live, (1.0, 2.0)), (departed, (3.0, 4.0))]);
        store
            .set(departed, FacetId::new("some-mod.exotic"), json!(true), &AcceptAll)
            .unwrap();
        let present: BTreeSet<Uuid> = [live].into_iter().collect();
        retain_present_nodes(&mut store, &present);
        assert_eq!(read_arrangement_positions(&store), vec![(live, (1.0, 2.0))]);
        assert!(
            store.facets_of(&departed).is_none(),
            "a departed node's whole facet record goes with it"
        );
    }
}
