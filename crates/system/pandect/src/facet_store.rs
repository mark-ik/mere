// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Per-node facet-store sidecar (`facets.json`) — the durable home for atomic,
//! typed per-node metadata.
//!
//! Facets are the **runtime tier** of the one-node facet system (design doc
//! `2026-07-18_one_node_facets_layer_map.md`): optional, typed, schema-validated
//! records keyed by node id, so a node carries content-specific metadata (and a
//! modder defines new metadata) without changing the kernel `Node`. The store
//! type and its validation seam live in [`chartulary::facet`]; this module is the
//! mere-side **wiring**: it persists a [`FacetStore<Uuid>`](chartulary::FacetStore)
//! (mere's node id is a `Uuid`) beside the session graph, on the same sidecar
//! pattern as [`browser_node_state`](crate::browser_node_state) and
//! [`denizen_bindings`](crate::denizen_bindings).
//!
//! ```text
//! <sessions_dir>/<session_id>/
//! ├── graph.json               ← session_graph_store
//! ├── browser_nodes.json       ← browser_node_state   (web.* facets, later)
//! ├── denizen_bindings.json    ← denizen_bindings     (denizen.* facets, later)
//! └── facets.json              ← this module
//! ```
//!
//! **The convergence target.** The bespoke per-node sidecars (browser state,
//! denizen bindings, and cartography's per-node arrangement data) are facets
//! avant la lettre: typed metadata keyed by node id. This store is the one
//! mechanism they fold into over time (namespaces `web.*`, `denizen.*`,
//! `arrangement.*`), replacing N hand-rolled documents. This wiring is the
//! enabling rung; the migrations follow behind it, opportunistically.
//!
//! Validation (a [`FacetValidator`](chartulary::FacetValidator)) is a write-time
//! host concern (mere backs it with eidetic schema codicils); persistence here is
//! schema-agnostic. One JSON document, atomic write, `Ok(None)` when absent —
//! deleting it drops facets, never graph truth.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use uuid::Uuid;

pub use chartulary::{AcceptAll, ExpiringFacet, FacetError, FacetId, FacetValidator, NodeFacets};

/// The mere node-facet store: a [`chartulary::FacetStore`] keyed by the node's
/// stable `Uuid`.
pub type NodeFacetStore = chartulary::FacetStore<Uuid>;

/// Filename of the sidecar document, sibling to `graph.json`.
pub const NODE_FACETS_FILE: &str = "facets.json";

/// Read one revision-expiring facet at an explicit graph revision.
///
/// The stored value remains untouched on both sides of the boundary. An
/// expired or malformed envelope reads as absent, matching the fail-closed
/// convention of Mere's other typed facet readers. The caller supplies the
/// revision so replay can use the journal prefix's revision rather than a
/// process clock.
pub fn read_expiring_facet<Id, T>(
    store: &chartulary::FacetStore<Id>,
    node: &Id,
    facet: &FacetId,
    revision: u64,
) -> Option<T>
where
    Id: Ord + Clone,
    T: DeserializeOwned,
{
    let stored = store.get(node, facet)?.clone();
    let envelope: ExpiringFacet<serde_json::Value> = serde_json::from_value(stored).ok()?;
    serde_json::from_value(envelope.into_value_at(revision)?).ok()
}

/// Path of the sidecar document under the per-session root. Pure — callers can
/// use it for existence checks.
pub fn node_facets_path(session_dir: &Path) -> PathBuf {
    session_dir.join(NODE_FACETS_FILE)
}

/// Serialize the store to JSON and write atomically (tmp + rename).
pub fn save_node_facets(session_dir: &Path, facets: &NodeFacetStore) -> io::Result<()> {
    let target = node_facets_path(session_dir);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(facets)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Copy every facet of every donor node named in `remap` onto its fork
/// counterpart — the fork's facet-carry (tear-out G4-R R1). Whole-record on
/// purpose: a forked node keeps its entire per-node character (`arrangement.*`
/// layout, `web.*` browser state, foreign/mod namespaces) without this module
/// knowing any namespace. `remap` is the `(source id, new id)` list the
/// kernel's `copy_component_from` returns; donor nodes absent from the store
/// contribute nothing. The container-scoped `scene.*` carry is separate
/// (`scene_facets::copy_scene_facets`) — container ids are not graph nodes, so
/// no remap names them.
pub fn copy_node_facets(donor: &NodeFacetStore, fork: &mut NodeFacetStore, remap: &[(Uuid, Uuid)]) {
    for (source, new) in remap {
        let Some(facets) = donor.facets_of(source) else {
            continue;
        };
        for (facet, value) in facets.iter() {
            let _ = fork.set(*new, facet.clone(), value.clone(), &AcceptAll);
        }
    }
}

/// Read the sidecar document. Returns `Ok(None)` when it doesn't exist (a
/// session with no facets yet).
pub fn load_node_facets(session_dir: &Path) -> io::Result<Option<NodeFacetStore>> {
    let path = node_facets_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let facets: NodeFacetStore =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(facets))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chartulary::{Author, Container, GraphLog, Relation};
    use serde_json::json;
    use servitor::{
        Cap, CascadeBudget, CascadeOutcome, CommittedEntry, Grant, GrantTable, Mode, ScopePath,
        Subject, WatchTable, run_cascade,
    };

    fn temp_session_dir(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("mere-facet-store-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> NodeFacetStore {
        let mut store = NodeFacetStore::new();
        let node = Uuid::from_u128(0xa);
        store
            .set(
                node,
                FacetId::new("web.viewer"),
                json!({ "mode": "reader" }),
                &AcceptAll,
            )
            .unwrap();
        store
            .set(
                node,
                FacetId::new("arrangement.pin"),
                json!(true),
                &AcceptAll,
            )
            .unwrap();
        store
    }

    fn scope(raw: &str) -> ScopePath {
        ScopePath::parse(raw).expect("valid test scope")
    }

    #[test]
    fn the_reader_filters_at_the_revision_boundary_without_mutating_storage() {
        let node = Uuid::from_u128(0xa);
        let facet = FacetId::new("example.level");
        let stored = serde_json::to_value(ExpiringFacet::new(json!(7), 4)).unwrap();
        let mut store = NodeFacetStore::new();
        store
            .set(node, facet.clone(), stored.clone(), &AcceptAll)
            .unwrap();

        assert_eq!(
            read_expiring_facet::<_, u64>(&store, &node, &facet, 3),
            Some(7)
        );
        assert_eq!(
            read_expiring_facet::<_, u64>(&store, &node, &facet, 4),
            None
        );
        assert_eq!(
            store.get(&node, &facet),
            Some(&stored),
            "expiry is a read predicate, not an implicit removal"
        );

        let malformed = FacetId::new("example.malformed");
        store
            .set(node, malformed.clone(), json!(7), &AcceptAll)
            .unwrap();
        assert_eq!(
            read_expiring_facet::<_, u64>(&store, &node, &malformed, 0),
            None,
            "a non-envelope value fails closed"
        );
    }

    #[test]
    fn a_recorded_cascade_replays_identically_across_expiry() {
        let node = "trail/signal".to_string();
        let signal = FacetId::new("example.pulse");
        let answer = FacetId::new("example.answer");
        let user = Author::new("user");
        let mut graph = GraphLog::<Container, Relation>::new();
        graph.insert_node(&user, Container::new(node.clone()));
        graph.set_facet(
            &user,
            &node,
            signal.clone(),
            serde_json::to_value(ExpiringFacet::new("pulse", 3)).unwrap(),
        );
        assert_eq!(graph.revision(), 2);
        let before_expiry = graph.log().clone();

        let subject = Subject::new([7; 32]);
        let authority = GrantTable::new().with_grant(Grant::new(
            subject,
            Cap::scope("trail").unwrap(),
            Mode::Read,
        ));
        let self_author = subject.to_author().as_str().to_string();
        let mut watches = WatchTable::new();
        watches
            .register(&authority, subject, scope("trail"), self_author.clone())
            .unwrap();

        let mut live_reads = vec![read_expiring_facet::<_, String>(
            graph.facets(),
            &node,
            &signal,
            graph.revision(),
        )];
        let cascade = run_cascade(
            &mut watches,
            CascadeBudget::DEFAULT,
            vec![CommittedEntry::new(
                graph.revision() - 1,
                "user",
                vec![scope("trail/signal")],
            )],
            |wakes| {
                assert_eq!(wakes.len(), 1);
                assert!(graph.set_facet(&subject.to_author(), &node, answer.clone(), json!(true),));
                live_reads.push(read_expiring_facet::<_, String>(
                    graph.facets(),
                    &node,
                    &signal,
                    graph.revision(),
                ));
                vec![CommittedEntry::new(
                    graph.revision() - 1,
                    self_author.clone(),
                    vec![scope("trail/signal")],
                )]
            },
        );
        assert_eq!(cascade.rounds.len(), 1);
        assert_eq!(cascade.outcome, CascadeOutcome::Settled);
        assert_eq!(
            graph.revision(),
            3,
            "the answer crosses the expiry boundary"
        );
        let after_expiry = graph.log().clone();

        let replay_before = GraphLog::replay(before_expiry);
        let replay_after = GraphLog::replay(after_expiry);
        let replay_reads = vec![
            read_expiring_facet::<_, String>(
                replay_before.facets(),
                &node,
                &signal,
                replay_before.revision(),
            ),
            read_expiring_facet::<_, String>(
                replay_after.facets(),
                &node,
                &signal,
                replay_after.revision(),
            ),
        ];

        assert_eq!(live_reads, vec![Some("pulse".into()), None]);
        assert_eq!(replay_reads, live_reads);
        assert!(
            replay_after.facets().get(&node, &signal).is_some(),
            "replay retains the expired envelope as historical state"
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_session_dir("round-trip");
        let original = sample();
        save_node_facets(&dir, &original).unwrap();
        let restored = load_node_facets(&dir).unwrap().expect("sidecar present");
        assert_eq!(restored, original);
        let node = Uuid::from_u128(0xa);
        assert_eq!(
            restored.get(&node, &FacetId::new("web.viewer")),
            Some(&json!({ "mode": "reader" }))
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let dir = temp_session_dir("no-file");
        assert!(load_node_facets(&dir).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_foreign_facet_survives_the_round_trip() {
        // A facet namespace this build has no schema for — a mod's, or a future
        // convergence namespace. The store persists it untouched.
        let dir = temp_session_dir("foreign");
        let mut store = NodeFacetStore::new();
        let node = Uuid::from_u128(0xb);
        store
            .set(
                node,
                FacetId::new("some-mod.exotic"),
                json!({ "k": [1, 2] }),
                &AcceptAll,
            )
            .unwrap();
        save_node_facets(&dir, &store).unwrap();
        let restored = load_node_facets(&dir).unwrap().unwrap();
        assert_eq!(
            restored.get(&node, &FacetId::new("some-mod.exotic")),
            Some(&json!({ "k": [1, 2] }))
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn copy_node_facets_carries_the_whole_record_through_the_remap() {
        let mut donor = NodeFacetStore::new();
        let (src_a, src_b, unforked) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let (new_a, new_b) = (Uuid::from_u128(0x11), Uuid::from_u128(0x12));
        donor
            .set(
                src_a,
                FacetId::new("arrangement.position"),
                json!({"x": 5.0, "y": 6.0}),
                &AcceptAll,
            )
            .unwrap();
        donor
            .set(
                src_a,
                FacetId::new("some-mod.exotic"),
                json!({"k": 1}),
                &AcceptAll,
            )
            .unwrap();
        donor
            .set(src_b, FacetId::new("web.content"), json!(true), &AcceptAll)
            .unwrap();
        donor
            .set(
                unforked,
                FacetId::new("web.content"),
                json!(true),
                &AcceptAll,
            )
            .unwrap();

        let mut fork = NodeFacetStore::new();
        copy_node_facets(&donor, &mut fork, &[(src_a, new_a), (src_b, new_b)]);

        assert_eq!(
            fork.get(&new_a, &FacetId::new("arrangement.position")),
            Some(&json!({"x": 5.0, "y": 6.0})),
            "layout rides the carry"
        );
        assert_eq!(
            fork.get(&new_a, &FacetId::new("some-mod.exotic")),
            Some(&json!({"k": 1})),
            "foreign namespaces ride too — the whole character"
        );
        assert_eq!(
            fork.get(&new_b, &FacetId::new("web.content")),
            Some(&json!(true))
        );
        assert!(
            fork.facets_of(&unforked).is_none() && fork.facets_of(&src_a).is_none(),
            "nodes outside the remap (and donor ids themselves) do not appear"
        );
    }

    #[test]
    fn malformed_json_returns_invalid_data_error() {
        let dir = temp_session_dir("malformed");
        fs::write(node_facets_path(&dir), "{ not json").unwrap();
        match load_node_facets(&dir) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("expected malformed JSON to fail parsing"),
        }
        fs::remove_dir_all(&dir).ok();
    }
}
