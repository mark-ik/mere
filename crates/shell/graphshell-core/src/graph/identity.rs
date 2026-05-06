/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

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

use euclid::default::{Point2D, Vector2D};
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

pub(crate) struct Point2DAsTuple;

impl ArchiveWith<Point2D<f32>> for Point2DAsTuple {
    type Archived = Archived<(f32, f32)>;
    type Resolver = Resolver<(f32, f32)>;

    fn resolve_with(field: &Point2D<f32>, resolver: Self::Resolver, out: Place<Self::Archived>) {
        let value = (field.x, field.y);
        value.resolve(resolver, out);
    }
}

impl<S> SerializeWith<Point2D<f32>, S> for Point2DAsTuple
where
    S: Fallible + ?Sized,
    (f32, f32): Serialize<S>,
{
    fn serialize_with(
        field: &Point2D<f32>,
        serializer: &mut S,
    ) -> Result<Self::Resolver, S::Error> {
        let value = (field.x, field.y);
        value.serialize(serializer)
    }
}

impl<D> DeserializeWith<Archived<(f32, f32)>, Point2D<f32>, D> for Point2DAsTuple
where
    D: Fallible + ?Sized,
    Archived<(f32, f32)>: Deserialize<(f32, f32), D>,
{
    fn deserialize_with(
        field: &Archived<(f32, f32)>,
        deserializer: &mut D,
    ) -> Result<Point2D<f32>, D::Error> {
        let (x, y) = field.deserialize(deserializer)?;
        Ok(Point2D::new(x, y))
    }
}

pub(crate) struct Vector2DAsTuple;

impl ArchiveWith<Vector2D<f32>> for Vector2DAsTuple {
    type Archived = Archived<(f32, f32)>;
    type Resolver = Resolver<(f32, f32)>;

    fn resolve_with(field: &Vector2D<f32>, resolver: Self::Resolver, out: Place<Self::Archived>) {
        let value = (field.x, field.y);
        value.resolve(resolver, out);
    }
}

impl<S> SerializeWith<Vector2D<f32>, S> for Vector2DAsTuple
where
    S: Fallible + ?Sized,
    (f32, f32): Serialize<S>,
{
    fn serialize_with(
        field: &Vector2D<f32>,
        serializer: &mut S,
    ) -> Result<Self::Resolver, S::Error> {
        let value = (field.x, field.y);
        value.serialize(serializer)
    }
}

impl<D> DeserializeWith<Archived<(f32, f32)>, Vector2D<f32>, D> for Vector2DAsTuple
where
    D: Fallible + ?Sized,
    Archived<(f32, f32)>: Deserialize<(f32, f32), D>,
{
    fn deserialize_with(
        field: &Archived<(f32, f32)>,
        deserializer: &mut D,
    ) -> Result<Vector2D<f32>, D::Error> {
        let (x, y) = field.deserialize(deserializer)?;
        Ok(Vector2D::new(x, y))
    }
}
