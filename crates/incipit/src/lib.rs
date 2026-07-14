/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # incipit
//!
//! The names by which a work is found: mere's workspace identity vocabulary.
//! An *incipit* is the opening words that identified a manuscript before books
//! carried title pages.
//!
//! Two ids, and nothing else: [`GraphId`] (a graph at app scope) and
//! [`SessionId`] (a durable session). They live apart from the graph they name
//! on purpose. `crawl` needs to tag material with a `GraphId` without taking a
//! dependency on `kernel`, and `frisket` needs to bind a pane to a graph without
//! reaching for graph truth. Both get the id, neither gets the graph.
//!
//! Extracted from the former `frame` crate (2026-07-14), which fused this
//! vocabulary with the pane model. The panes are now [`frisket`]; the ids are
//! here.
//!
//! [`frisket`]: https://docs.rs/frisket

#![doc(html_root_url = "https://docs.rs/incipit/0.0.1")]

use serde::{Deserialize, Serialize};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// Stable identifier for a graph at app scope. Every leaf in a
/// `frisket::FrisketLayout` carries one so the host can resolve "which graph
/// does this pane render?" against the app's `GraphRegistry`.
///
/// Pane layouts persist with serialized graph ids so a saved arrangement
/// reattaches to the right graphs on next launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphId(pub uuid::Uuid);

impl GraphId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    /// The nil (all-zero) id: the "unbound" marker a window-chrome leaf carries
    /// (it follows no graph), distinct from any real graph's id. (Multi-graph MG5.)
    pub fn nil() -> Self {
        Self(uuid::Uuid::nil())
    }

    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl Default for GraphId {
    fn default() -> Self {
        Self::new()
    }
}

/// Durable session identity. Wraps the runtime/session shape: a
/// session owns a root graph (and may grow sub-graph references),
/// holds the worker manifest, engine profile binding, and policy
/// overrides. v0 of session-persistence maps one `SessionId` 1:1
/// to one root `GraphId`; the type distinction is enforced from
/// day one so later phases (sub-graphs, fork-on-divergence,
/// multi-graph-per-session) don't require a painful retrofit.
///
/// See `design_docs/mere_docs/research/2026-05-11_browser_multiplexer_framing.md`
/// §2 (identity matrix) for the broader identity model and
/// `design_docs/mere_docs/implementation_strategy/2026-05-11_graph_session_manifest_plan.md`
/// for storage / lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub uuid::Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_and_session_ids_are_distinct_types_over_the_same_uuid() {
        let uuid = uuid::Uuid::new_v4();
        let graph = GraphId::from_uuid(uuid);
        let session = SessionId::from_uuid(uuid);
        assert_eq!(graph.as_uuid(), session.as_uuid());
    }

    #[test]
    fn nil_graph_id_is_the_unbound_marker() {
        assert_eq!(GraphId::nil().as_uuid(), &uuid::Uuid::nil());
        assert_ne!(GraphId::nil(), GraphId::new());
    }

    #[test]
    fn ids_round_trip_through_serde() {
        let graph = GraphId::new();
        let json = serde_json::to_string(&graph).expect("serialize");
        let back: GraphId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(graph, back);
    }
}
