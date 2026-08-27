// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
//! The family: `arrangement.position` (first wired), then size / sprite /
//! sprite-hull / material / face — one facet id + payload shape + write/read
//! pair each, all with the same rewrite-clears-stale save semantics. The
//! graph-scoped canvas flags the bespoke geometry carried (`size_by_degree`,
//! `size_by_importance`, `importance_metric`) are deliberately NOT here; they
//! await their view-settings home.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::facet_store::{AcceptAll, FacetId, NodeFacetStore};

/// Facet id of a node's durable world position. Payload: `{"x": f32, "y": f32}`
/// (world coordinates, the semantic geometry — not pixels).
pub const ARRANGEMENT_POSITION: &str = "arrangement.position";

/// Facet id of a node's explicit face-size override. Payload: a number (px).
/// Only deliberate overrides persist; degree/importance-derived sizes recompute.
pub const ARRANGEMENT_SIZE: &str = "arrangement.size";

/// Facet id of a node's custom sprite face. Payload: a string (PNG data-URI).
pub const ARRANGEMENT_SPRITE: &str = "arrangement.sprite";

/// Facet id of a sprite's collider hull. Payload: `[[x, y], …]` — a
/// face-normalized convex polygon (coordinates in `[-0.5, 0.5]`), persisted
/// beside the sprite so the traced collider survives without re-decoding.
pub const ARRANGEMENT_SPRITE_HULL: &str = "arrangement.sprite_hull";

/// Facet id of a node's physical material override. Payload:
/// `{"restitution": f32, "friction": f32, "density": f32}`.
pub const ARRANGEMENT_MATERIAL: &str = "arrangement.material";

/// Facet id of a node's face override. Payload: a string code
/// (`favicon` / `sprite` / `bare` — the canvas's `Face::from_code` vocabulary).
pub const ARRANGEMENT_FACE: &str = "arrangement.face";

/// The [`ARRANGEMENT_POSITION`] facet id, constructed.
pub fn arrangement_position_facet() -> FacetId {
    FacetId::new(ARRANGEMENT_POSITION)
}

/// Replace every facet carrying `facet` with the given `(node, payload)` set —
/// the shared save-time semantics of the whole family: existing facets of this
/// id are cleared first, so a node absent from the new set carries none
/// afterwards; facets in other namespaces (and other family members) are
/// untouched. Payloads go through [`AcceptAll`]: the shapes are fixed by this
/// module, so schema validation adds nothing.
fn rewrite_family(store: &mut NodeFacetStore, facet: FacetId, entries: Vec<(Uuid, Value)>) {
    let stale: Vec<Uuid> = store
        .iter()
        .filter(|(_, facets)| facets.has(&facet))
        .map(|(id, _)| *id)
        .collect();
    for id in stale {
        store.remove(&id, &facet);
    }
    for (id, payload) in entries {
        let _ = store.set(id, facet.clone(), payload, &AcceptAll);
    }
}

/// The shared load-time read: every node carrying `facet`, its payload parsed
/// by `parse`. A malformed payload is skipped rather than failing the load:
/// one bad facet must not cost the whole session its layout.
fn read_family<T>(
    store: &NodeFacetStore,
    facet: &str,
    parse: impl Fn(&Value) -> Option<T>,
) -> Vec<(Uuid, T)> {
    let facet = FacetId::new(facet);
    store
        .iter()
        .filter_map(|(id, facets)| parse(facets.get(&facet)?).map(|t| (*id, t)))
        .collect()
}

/// Save-time write of [`ARRANGEMENT_POSITION`] (rewrite-clears-stale).
pub fn write_arrangement_positions(
    store: &mut NodeFacetStore,
    positions: impl IntoIterator<Item = (Uuid, (f32, f32))>,
) {
    rewrite_family(
        store,
        arrangement_position_facet(),
        positions
            .into_iter()
            .map(|(id, (x, y))| (id, json!({ "x": x, "y": y })))
            .collect(),
    );
}

/// Load-time read of [`ARRANGEMENT_POSITION`], shaped for
/// `Canvas::seed_cartography`.
pub fn read_arrangement_positions(store: &NodeFacetStore) -> Vec<(Uuid, (f32, f32))> {
    read_family(store, ARRANGEMENT_POSITION, |value| {
        let x = value.get("x")?.as_f64()? as f32;
        let y = value.get("y")?.as_f64()? as f32;
        (x.is_finite() && y.is_finite()).then_some((x, y))
    })
}

/// Save-time write of [`ARRANGEMENT_SIZE`] (rewrite-clears-stale).
pub fn write_arrangement_sizes(
    store: &mut NodeFacetStore,
    sizes: impl IntoIterator<Item = (Uuid, f32)>,
) {
    rewrite_family(
        store,
        FacetId::new(ARRANGEMENT_SIZE),
        sizes.into_iter().map(|(id, px)| (id, json!(px))).collect(),
    );
}

/// Load-time read of [`ARRANGEMENT_SIZE`], shaped for
/// `Canvas::apply_cartography_sizing` (which clamps).
pub fn read_arrangement_sizes(store: &NodeFacetStore) -> Vec<(Uuid, f32)> {
    read_family(store, ARRANGEMENT_SIZE, |value| {
        let px = value.as_f64()? as f32;
        px.is_finite().then_some(px)
    })
}

/// Save-time write of [`ARRANGEMENT_SPRITE`] (rewrite-clears-stale).
pub fn write_arrangement_sprites<S: AsRef<str>>(
    store: &mut NodeFacetStore,
    sprites: impl IntoIterator<Item = (Uuid, S)>,
) {
    rewrite_family(
        store,
        FacetId::new(ARRANGEMENT_SPRITE),
        sprites
            .into_iter()
            .map(|(id, uri)| (id, json!(uri.as_ref())))
            .collect(),
    );
}

/// Load-time read of [`ARRANGEMENT_SPRITE`] (owned data-URIs; the host lends
/// them to `Canvas::apply_cartography_sprites`).
pub fn read_arrangement_sprites(store: &NodeFacetStore) -> Vec<(Uuid, String)> {
    read_family(store, ARRANGEMENT_SPRITE, |value| {
        Some(value.as_str()?.to_string())
    })
}

/// Save-time write of [`ARRANGEMENT_SPRITE_HULL`] (rewrite-clears-stale).
pub fn write_arrangement_sprite_hulls(
    store: &mut NodeFacetStore,
    hulls: impl IntoIterator<Item = (Uuid, Vec<(f32, f32)>)>,
) {
    rewrite_family(
        store,
        FacetId::new(ARRANGEMENT_SPRITE_HULL),
        hulls
            .into_iter()
            .map(|(id, hull)| {
                let points: Vec<Value> = hull.iter().map(|(x, y)| json!([x, y])).collect();
                (id, Value::Array(points))
            })
            .collect(),
    );
}

/// Load-time read of [`ARRANGEMENT_SPRITE_HULL`], shaped for
/// `Canvas::apply_cartography_sprite_hulls`. A hull with any malformed or
/// non-finite point is skipped whole (a partial polygon is a wrong collider).
pub fn read_arrangement_sprite_hulls(store: &NodeFacetStore) -> Vec<(Uuid, Vec<(f32, f32)>)> {
    read_family(store, ARRANGEMENT_SPRITE_HULL, |value| {
        value
            .as_array()?
            .iter()
            .map(|point| {
                let pair = point.as_array()?;
                let x = pair.first()?.as_f64()? as f32;
                let y = pair.get(1)?.as_f64()? as f32;
                (x.is_finite() && y.is_finite()).then_some((x, y))
            })
            .collect::<Option<Vec<(f32, f32)>>>()
    })
}

/// Save-time write of [`ARRANGEMENT_MATERIAL`] (rewrite-clears-stale). The
/// tuple is `(restitution, friction, density)`, the geometry sidecar's order.
pub fn write_arrangement_materials(
    store: &mut NodeFacetStore,
    materials: impl IntoIterator<Item = (Uuid, (f32, f32, f32))>,
) {
    rewrite_family(
        store,
        FacetId::new(ARRANGEMENT_MATERIAL),
        materials
            .into_iter()
            .map(|(id, (restitution, friction, density))| {
                (
                    id,
                    json!({
                        "restitution": restitution,
                        "friction": friction,
                        "density": density,
                    }),
                )
            })
            .collect(),
    );
}

/// Load-time read of [`ARRANGEMENT_MATERIAL`] as
/// `(restitution, friction, density)`, shaped for
/// `Canvas::apply_cartography_materials`.
pub fn read_arrangement_materials(store: &NodeFacetStore) -> Vec<(Uuid, (f32, f32, f32))> {
    read_family(store, ARRANGEMENT_MATERIAL, |value| {
        let restitution = value.get("restitution")?.as_f64()? as f32;
        let friction = value.get("friction")?.as_f64()? as f32;
        let density = value.get("density")?.as_f64()? as f32;
        (restitution.is_finite() && friction.is_finite() && density.is_finite()).then_some((
            restitution,
            friction,
            density,
        ))
    })
}

/// Save-time write of [`ARRANGEMENT_FACE`] (rewrite-clears-stale).
pub fn write_arrangement_faces<S: AsRef<str>>(
    store: &mut NodeFacetStore,
    faces: impl IntoIterator<Item = (Uuid, S)>,
) {
    rewrite_family(
        store,
        FacetId::new(ARRANGEMENT_FACE),
        faces
            .into_iter()
            .map(|(id, code)| (id, json!(code.as_ref())))
            .collect(),
    );
}

/// Load-time read of [`ARRANGEMENT_FACE`] (owned codes; the host lends them to
/// `Canvas::apply_cartography_faces`, whose `Face::from_code` owns the
/// vocabulary — an unknown code degrades there, not here).
pub fn read_arrangement_faces(store: &NodeFacetStore) -> Vec<(Uuid, String)> {
    read_family(store, ARRANGEMENT_FACE, |value| {
        Some(value.as_str()?.to_string())
    })
}

/// Drop every facet of every node not in `present` — the load-time reconcile,
/// mirroring the graph-membership retain the bespoke geometry sidecar did. A
/// node deleted between sessions takes its whole facet record with it (all
/// namespaces: its facets describe a node that no longer exists); facets of
/// live nodes — including foreign/mod namespaces — are untouched.
///
/// `present` is every id whose facets should survive — which is the live graph
/// members **plus any container id** carrying `scene.*` facets (a container is
/// not a leaf node, so a caller keying scene facets by `root_graph_id` must
/// include it here, or the reconcile would sweep the whole scene away).
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
            .set(
                a,
                FacetId::new("web.viewer"),
                json!({ "mode": "reader" }),
                &AcceptAll,
            )
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
            .set(
                good,
                facet.clone(),
                json!({ "x": 7.0, "y": 8.0 }),
                &AcceptAll,
            )
            .unwrap();
        store
            .set(bad_shape, facet.clone(), json!([7.0, 8.0]), &AcceptAll)
            .unwrap();
        // JSON has no NaN literal; a null coordinate stands in for the
        // unparsable case (as_f64 fails the same way).
        store
            .set(
                bad_nan,
                facet.clone(),
                json!({ "x": null, "y": 1.0 }),
                &AcceptAll,
            )
            .unwrap();
        assert_eq!(read_arrangement_positions(&store), vec![(good, (7.0, 8.0))]);
    }

    #[test]
    fn every_family_member_round_trips() {
        let mut store = NodeFacetStore::new();
        let a = Uuid::from_u128(1);
        write_arrangement_sizes(&mut store, [(a, 48.0)]);
        write_arrangement_sprites(&mut store, [(a, "data:image/png;base64,AAAA")]);
        write_arrangement_sprite_hulls(
            &mut store,
            [(a, vec![(-0.5, -0.5), (0.5, 0.0), (0.0, 0.5)])],
        );
        write_arrangement_materials(&mut store, [(a, (0.8, 0.2, 3.0))]);
        write_arrangement_faces(&mut store, [(a, "sprite")]);

        assert_eq!(read_arrangement_sizes(&store), vec![(a, 48.0)]);
        assert_eq!(
            read_arrangement_sprites(&store),
            vec![(a, "data:image/png;base64,AAAA".to_string())]
        );
        assert_eq!(
            read_arrangement_sprite_hulls(&store),
            vec![(a, vec![(-0.5, -0.5), (0.5, 0.0), (0.0, 0.5)])]
        );
        assert_eq!(
            read_arrangement_materials(&store),
            vec![(a, (0.8, 0.2, 3.0))]
        );
        assert_eq!(
            read_arrangement_faces(&store),
            vec![(a, "sprite".to_string())]
        );
    }

    #[test]
    fn each_family_rewrite_clears_only_its_own_facet() {
        let mut store = NodeFacetStore::new();
        let a = Uuid::from_u128(1);
        write_arrangement_sizes(&mut store, [(a, 48.0)]);
        write_arrangement_faces(&mut store, [(a, "bare")]);
        // Rewriting sizes to empty must not disturb the face facet.
        write_arrangement_sizes(&mut store, []);
        assert!(read_arrangement_sizes(&store).is_empty());
        assert_eq!(
            read_arrangement_faces(&store),
            vec![(a, "bare".to_string())]
        );
    }

    #[test]
    fn a_hull_with_a_malformed_point_is_skipped_whole() {
        let mut store = NodeFacetStore::new();
        let a = Uuid::from_u128(1);
        store
            .set(
                a,
                FacetId::new(ARRANGEMENT_SPRITE_HULL),
                json!([[0.1, 0.2], [null, 0.3]]),
                &AcceptAll,
            )
            .unwrap();
        assert!(
            read_arrangement_sprite_hulls(&store).is_empty(),
            "a partial polygon is a wrong collider; drop the hull"
        );
    }

    #[test]
    fn retain_present_drops_departed_nodes_entirely() {
        let mut store = NodeFacetStore::new();
        let live = Uuid::from_u128(1);
        let departed = Uuid::from_u128(2);
        write_arrangement_positions(&mut store, [(live, (1.0, 2.0)), (departed, (3.0, 4.0))]);
        store
            .set(
                departed,
                FacetId::new("some-mod.exotic"),
                json!(true),
                &AcceptAll,
            )
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
