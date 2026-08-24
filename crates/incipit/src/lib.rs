// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # incipit
//!
//! The names by which a work is found: mere's workspace identity vocabulary.
//! An *incipit* is the opening words that identified a manuscript before books
//! carried title pages.
//!
//! Identity vocabulary, and no graph truth: [`GraphId`] names a graph at app
//! scope, [`SessionId`] names a durable session, and [`ShelfmarkV1`] cites the
//! authority inputs and projection needed to reconstitute one view. `crawl`
//! can tag material with a `GraphId`, `frisket` can bind panes to graphs, and a
//! projection host can preserve a shelfmark without any of them reaching for
//! graph authority.
//!
//! Extracted from the former `frame` crate (2026-07-14), which fused this
//! vocabulary with the pane model. The panes are now [`frisket`]; the ids are
//! here.
//!
//! [`frisket`]: https://docs.rs/frisket

#![doc(html_root_url = "https://docs.rs/incipit/0.0.1")]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// The first shared scene-citation envelope.
pub const SHELFMARK_V1_SCHEMA: &str = "mere.shelfmark/1";

/// A source authority the resolver can find without learning its native
/// record shape.
///
/// `adapter` names the resolver and `record` is its opaque locator. Mer3ly, for
/// example, puts its dataset and revision cursor in that record. Incipit never
/// parses either product's authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShelfmarkAuthorityV1 {
    pub adapter: String,
    pub record: String,
}

/// One named input to a projection.
///
/// The expected generation sits beside the authority and reading that produce
/// it. This is what makes a composed projection checkable input by input
/// instead of reducing several authorities to one aggregate hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShelfmarkInputV1 {
    pub authority: ShelfmarkAuthorityV1,
    pub reading: String,
    /// Adapter-owned reading parameters needed for reconstitution, serialized
    /// once by that adapter and preserved opaquely here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading_parameters: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrangement: Option<String>,
    pub expects_generation: String,
}

/// A compact citation for a reconstitutable projection.
///
/// `inputs` is an ordered map because roles such as `rows` and `columns` are
/// part of a composed recipe and deterministic serialization is receipt
/// currency. Delta sections are opaque serialized records defined by their
/// owning targets. Incipit preserves them without gaining Scenograph,
/// Chirograph, or product dependencies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShelfmarkV1 {
    pub schema: String,
    pub projection: String,
    pub inputs: BTreeMap<String, ShelfmarkInputV1>,
    #[serde(default)]
    pub delta: BTreeMap<String, String>,
}

impl ShelfmarkV1 {
    pub fn new(projection: impl Into<String>) -> Self {
        Self {
            schema: SHELFMARK_V1_SCHEMA.to_owned(),
            projection: projection.into(),
            inputs: BTreeMap::new(),
            delta: BTreeMap::new(),
        }
    }

    /// Refuse citations that cannot name and check every required input.
    pub fn validate(&self) -> Result<(), ShelfmarkError> {
        if self.schema != SHELFMARK_V1_SCHEMA {
            return Err(ShelfmarkError::UnsupportedSchema(self.schema.clone()));
        }
        if self.projection.trim().is_empty() {
            return Err(ShelfmarkError::MissingProjection);
        }
        if self.inputs.is_empty() {
            return Err(ShelfmarkError::MissingInputs);
        }
        for (role, input) in &self.inputs {
            if role.trim().is_empty()
                || input.authority.adapter.trim().is_empty()
                || input.authority.record.trim().is_empty()
                || input.reading.trim().is_empty()
                || input
                    .reading_parameters
                    .as_ref()
                    .is_some_and(|parameters| parameters.trim().is_empty())
                || input.expects_generation.trim().is_empty()
            {
                return Err(ShelfmarkError::InvalidInput(role.clone()));
            }
        }
        if let Some(section) = self.delta.keys().find(|name| name.trim().is_empty()) {
            return Err(ShelfmarkError::InvalidDeltaSection(section.clone()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShelfmarkError {
    UnsupportedSchema(String),
    MissingProjection,
    MissingInputs,
    InvalidInput(String),
    InvalidDeltaSection(String),
}

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

    #[test]
    fn composed_shelfmark_keeps_each_input_separately_checkable() {
        let mut shelfmark = ShelfmarkV1::new("matrix");
        shelfmark.inputs.insert(
            "columns".into(),
            ShelfmarkInputV1 {
                authority: ShelfmarkAuthorityV1 {
                    adapter: "mer3ly.dataset".into(),
                    record: "specimen@authored".into(),
                },
                reading: "changes".into(),
                reading_parameters: None,
                arrangement: None,
                expects_generation: "22".into(),
            },
        );
        shelfmark.inputs.insert(
            "rows".into(),
            ShelfmarkInputV1 {
                authority: ShelfmarkAuthorityV1 {
                    adapter: "mer3ly.dataset".into(),
                    record: "live@648bf19".into(),
                },
                reading: "neighbors".into(),
                reading_parameters: Some("{\"focus\":\"mere\"}".into()),
                arrangement: None,
                expects_generation: "11".into(),
            },
        );
        shelfmark.delta.insert(
            "selection".into(),
            "{\"resolution\":\"crossfilter\"}".into(),
        );

        shelfmark.validate().expect("valid composed shelfmark");
        let wire = serde_json::to_string(&shelfmark).expect("serialize shelfmark");
        assert!(wire.find("columns").unwrap() < wire.find("rows").unwrap());
        let far_side: ShelfmarkV1 = serde_json::from_str(&wire).expect("decode shelfmark");
        assert_eq!(far_side, shelfmark);
        assert_eq!(far_side.inputs["rows"].expects_generation, "11");
        assert_eq!(far_side.inputs["columns"].expects_generation, "22");
    }

    #[test]
    fn shelfmark_refuses_an_uncheckable_input() {
        let mut shelfmark = ShelfmarkV1::new("matrix");
        shelfmark.inputs.insert(
            "rows".into(),
            ShelfmarkInputV1 {
                authority: ShelfmarkAuthorityV1 {
                    adapter: "mer3ly.dataset".into(),
                    record: "live".into(),
                },
                reading: "neighbors".into(),
                reading_parameters: None,
                arrangement: None,
                expects_generation: String::new(),
            },
        );
        assert_eq!(
            shelfmark.validate(),
            Err(ShelfmarkError::InvalidInput("rows".into()))
        );
    }
}
