// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! `FormeDocument` + `FormeId` + `FormeRef` — a forme as a durable,
//! session-scoped workbench object.
//!
//! Per the composition spine (§14): a forme is an **independent curated
//! arrangement instance bound to a graph**, not the singleton for a graph-view.
//! A graph carries many formes (curated workbenches, compare benches, …) plus
//! the implicit identity arrangement the orrery uses.
//!
//! Ownership: `forme` owns these *types* + the [`crate::Arrangement`] schema +
//! pure mutation + the on-disk store ([`crate::store`], behind the `store`
//! feature so portable consumers stay `std::fs`-free). The *host* supplies
//! persistence policy — which session directory, and when to save. Geometry is
//! **not** here — it is a sibling store keyed `(FormeRef, ProjectionKind)`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::arrangement::Arrangement;

/// Stable id for a stored forme.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FormeId(Uuid);

impl FormeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for FormeId {
    fn default() -> Self {
        Self::new()
    }
}

/// The graph a forme is bound to. UUID-backed to keep forme framework-free —
/// it equals the host's `GraphId`/kernel graph UUID; the host converts at the
/// boundary.
pub type GraphId = Uuid;

/// What a graph-bearing pane projects.
///
/// - `Stored` — a persisted, curated forme (a workbench / compare bench).
/// - `Identity` — the implicit "all graph members" arrangement the orrery
///   uses. No copied roster is persisted; only its projection geometry
///   (cartography positions) persists, keyed by this ref.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FormeRef {
    Stored(FormeId),
    Identity(GraphId),
}

impl FormeRef {
    /// The graph this ref belongs to, for a `Stored` ref the caller must supply
    /// it separately (the `FormeDocument` carries it). `Identity` carries it
    /// inline.
    pub fn identity_graph(self) -> Option<GraphId> {
        match self {
            FormeRef::Identity(g) => Some(g),
            FormeRef::Stored(_) => None,
        }
    }
}

/// A stored forme: identity, the graph it's bound to, a label, lifecycle
/// timestamps (Unix millis, stamped by the store), and the geometry-free
/// arrangement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormeDocument {
    pub id: FormeId,
    pub graph_id: GraphId,
    pub label: Option<String>,
    /// Unix millis. `0` until the store stamps it on first save.
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub arrangement: Arrangement,
}

impl FormeDocument {
    /// A fresh, empty curated forme bound to `graph_id`. Timestamps are `0`
    /// until `pandect`'s store stamps them.
    pub fn new(graph_id: GraphId, label: Option<String>) -> Self {
        Self {
            id: FormeId::new(),
            graph_id,
            label,
            created_at_ms: 0,
            updated_at_ms: 0,
            arrangement: Arrangement::new(),
        }
    }

    /// A `Stored` ref to this forme.
    pub fn forme_ref(&self) -> FormeRef {
        FormeRef::Stored(self.id)
    }

    /// Fork: a new forme with a fresh id, the same graph + arrangement, cleared
    /// timestamps. The "Fork Workbench" gesture.
    pub fn fork(&self, label: Option<String>) -> Self {
        Self {
            id: FormeId::new(),
            graph_id: self.graph_id,
            label,
            created_at_ms: 0,
            updated_at_ms: 0,
            arrangement: self.arrangement.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_is_empty_arrangement_bound_to_graph() {
        let g = Uuid::from_u128(42);
        let doc = FormeDocument::new(g, Some("research".into()));
        assert_eq!(doc.graph_id, g);
        assert_eq!(doc.label.as_deref(), Some("research"));
        assert!(doc.arrangement.is_empty());
        assert_eq!(doc.forme_ref(), FormeRef::Stored(doc.id));
    }

    #[test]
    fn fork_gets_new_id_same_graph_and_arrangement() {
        let g = Uuid::from_u128(7);
        let mut doc = FormeDocument::new(g, Some("a".into()));
        doc.arrangement.add_tile_intent(Some(Uuid::from_u128(1)));
        let forked = doc.fork(Some("b".into()));
        assert_ne!(forked.id, doc.id);
        assert_eq!(forked.graph_id, g);
        assert_eq!(forked.arrangement, doc.arrangement);
        assert_eq!(forked.label.as_deref(), Some("b"));
    }

    #[test]
    fn forme_ref_identity_carries_graph() {
        let g = Uuid::from_u128(9);
        assert_eq!(FormeRef::Identity(g).identity_graph(), Some(g));
        assert_eq!(FormeRef::Stored(FormeId::new()).identity_graph(), None);
    }

    #[test]
    fn document_round_trips_through_json() {
        let mut doc = FormeDocument::new(Uuid::from_u128(3), None);
        let g = doc.arrangement.add_group(Some("g".into()));
        let t = doc.arrangement.add_tile_intent(Some(Uuid::from_u128(5)));
        doc.arrangement.attach(t, g);
        let json = serde_json::to_string(&doc).unwrap();
        let back: FormeDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, back);
    }
}
