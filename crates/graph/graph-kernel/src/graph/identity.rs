// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Graph identity types and rkyv archive helpers.
//!
//! Extracted from `graph/mod.rs` per the 2026-04-30 renderer plan §6.4
//! decomposition target (5,459 LOC → split). Contains:
//!
//! - Stable handle types (`NodeKey`, `EdgeKey`) — petgraph index aliases.
//! - Backend type aliases (`GraphDirection`, `GraphIndex`).
//! - `GraphViewId` — per-graph-view-pane identity (UUID-backed).
//! - rkyv `with = ...` archive helpers for `Uuid`, `Point2D<f32>`,
//!   and `Vector2D<f32>` — bridge types between rkyv's primitive
//!   archive shape and the actual field types used in `Node`/`Edge`/etc.
//!
//! WASM-clean: no host-side dependencies beyond what `graph/mod.rs`
//! already imports.

use petgraph::Directed;
use petgraph::stable_graph::{EdgeIndex, NodeIndex};
use rkyv::{
    Archive, Archived, Deserialize, Place, Resolver, Serialize,
    rancor::Fallible,
    with::{ArchiveWith, DeserializeWith, SerializeWith},
};
use uuid::Uuid;

/// Stable node handle (petgraph NodeIndex — survives other deletions).
pub type NodeKey = NodeIndex;

/// Stable edge handle (petgraph EdgeIndex).
pub type EdgeKey = EdgeIndex;

/// Graph backend direction type exposed for adapter integration.
pub type GraphDirection = Directed;

/// Graph backend index type exposed for adapter integration.
pub type GraphIndex = petgraph::graph::DefaultIx;

/// Unique identifier for a graph-view pane (one projection of a graph;
/// many graph-view panes can exist concurrently, each with its own
/// camera / selection / filter state).
///
/// Pre-M4 slice 10 (2026-04-22) this lived in `app/graph_views.rs`;
/// moved here alongside the other graph-level identity types so
/// portable runtime code (`ToolSurfaceReturnTarget`, `FrameViewModel`,
/// etc.) can reference it without reaching across the app boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GraphViewId(Uuid);

impl GraphViewId {
    /// Fresh random identity. Gated to non-WASM because
    /// `Uuid::new_v4()` requires an OS randomness source that is
    /// unavailable on `wasm32-unknown-unknown`; WASM hosts construct
    /// via [`GraphViewId::from_uuid`] with a host-supplied UUID.
    #[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
impl Default for GraphViewId {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) struct UuidAsBytes;

impl ArchiveWith<Uuid> for UuidAsBytes {
    type Archived = Archived<[u8; 16]>;
    type Resolver = Resolver<[u8; 16]>;

    fn resolve_with(field: &Uuid, resolver: Self::Resolver, out: Place<Self::Archived>) {
        let bytes = *field.as_bytes();
        bytes.resolve(resolver, out);
    }
}

impl<S> SerializeWith<Uuid, S> for UuidAsBytes
where
    S: Fallible + ?Sized,
    [u8; 16]: Serialize<S>,
{
    fn serialize_with(field: &Uuid, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let bytes = *field.as_bytes();
        bytes.serialize(serializer)
    }
}

impl<D> DeserializeWith<Archived<[u8; 16]>, Uuid, D> for UuidAsBytes
where
    D: Fallible + ?Sized,
    Archived<[u8; 16]>: Deserialize<[u8; 16], D>,
{
    fn deserialize_with(
        field: &Archived<[u8; 16]>,
        deserializer: &mut D,
    ) -> Result<Uuid, D::Error> {
        let bytes = field.deserialize(deserializer)?;
        Ok(Uuid::from_bytes(bytes))
    }
}

// `Point2DAsTuple` (the rkyv with-adapter for `Node.position`) left with the
// position field (S2): position is no longer graph truth, so the node carries no
// coordinate to archive. `UuidAsBytes` remains for the node identity.
//
// `Vector2DAsTuple` (the rkyv with-adapter for `Node.velocity`) left with the
// velocity field: seiche owns live velocity, so the graph node no longer carries
// it. `Point2DAsTuple` remains for the transient projected position.

/// rkyv with-adapter archiving a codicil [`LogId`](codicil::LogId) as its
/// string form — for `Node.nested` (the borne graph's identity; the one-node
/// ruling's containment tier). Wrap `Option<LogId>` fields as
/// `#[rkyv(with = rkyv::with::Map<LogIdAsString>)]`.
pub(crate) struct LogIdAsString;

impl ArchiveWith<codicil::LogId> for LogIdAsString {
    type Archived = Archived<String>;
    type Resolver = Resolver<String>;

    fn resolve_with(field: &codicil::LogId, resolver: Self::Resolver, out: Place<Self::Archived>) {
        field.as_str().to_string().resolve(resolver, out);
    }
}

impl<S> SerializeWith<codicil::LogId, S> for LogIdAsString
where
    S: Fallible + ?Sized,
    String: Serialize<S>,
{
    fn serialize_with(
        field: &codicil::LogId,
        serializer: &mut S,
    ) -> Result<Self::Resolver, S::Error> {
        field.as_str().to_string().serialize(serializer)
    }
}

impl<D> DeserializeWith<Archived<String>, codicil::LogId, D> for LogIdAsString
where
    D: Fallible + ?Sized,
    Archived<String>: Deserialize<String, D>,
{
    fn deserialize_with(
        field: &Archived<String>,
        deserializer: &mut D,
    ) -> Result<codicil::LogId, D::Error> {
        let raw: String = field.deserialize(deserializer)?;
        Ok(codicil::LogId::new(raw))
    }
}
