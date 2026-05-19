// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Scene contract — the minimal substrate-side types renderers see.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

use kurbo::{Affine, Size};

use crate::identity::RendererId;

/// Stable per-node identity in the spatial scene graph.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct NodeIdentity(NonZeroU64);

impl NodeIdentity {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Self(NonZeroU64::new(n).expect("identity counter never zero"))
    }

    pub fn as_u64(self) -> u64 {
        self.0.get()
    }
}

/// Spatial placement of a node within its parent scene's coordinate space.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Placement {
    pub transform: Affine,
}

impl Placement {
    pub const IDENTITY: Self = Self {
        transform: Affine::IDENTITY,
    };

    pub fn new(transform: Affine) -> Self {
        Self { transform }
    }

    pub fn translate(x: f64, y: f64) -> Self {
        Self::new(Affine::translate((x, y)))
    }
}

impl Default for Placement {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Level-of-detail at which a node should render.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum LodLevel {
    Thumbnail,
    Card,
    FullPane,
    DeepZoom,
}

/// What kind of content a scene node holds.
///
/// Dispatch key for the renderer registry.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeContentKind {
    WebPage,
    GraphView,
    /// A single graph node as a first-class spatial entity (Path B of
    /// the cartography projection). Whereas `GraphView` is one pane
    /// that paints an entire projection internally (Path A), a
    /// `GraphNode` is one substrate node per graph node — the host
    /// explodes a projection into these so each node gets its own
    /// hit-test target, placement, and (future) drag handling.
    GraphNode,
    Panel,
    Knot,
    DocumentTile,
    CustomCanvas,
    Composite,
    EdgeRendering,
    /// Chrome region between two sibling panes in a frametree — a
    /// narrow draggable strip. Painted by a substrate-side splitter
    /// renderer; drag state is host-owned (snapshot at mouse-down,
    /// updated on move, cleared on release).
    Splitter,
}

/// Set of [`NodeContentKind`]s a renderer handles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeContentKindSet(Vec<NodeContentKind>);

impl NodeContentKindSet {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn from_one(kind: NodeContentKind) -> Self {
        Self(vec![kind])
    }

    pub fn from_iter<I: IntoIterator<Item = NodeContentKind>>(iter: I) -> Self {
        let mut v: Vec<_> = iter.into_iter().collect();
        v.sort_by_key(|k| *k as u8);
        v.dedup();
        Self(v)
    }

    pub fn contains(&self, kind: NodeContentKind) -> bool {
        self.0.contains(&kind)
    }

    pub fn iter(&self) -> impl Iterator<Item = &NodeContentKind> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Borrowed view of a substrate scene node, passed to renderers each frame.
///
/// Not `Copy` because [`renderer_pin`](Self::renderer_pin) carries a
/// `RendererId` (a `Cow<'static, str>` under the hood). `Clone` is
/// cheap when the pin is `None` or built from a static string;
/// degrades to a `String` clone if built from a dynamic name.
#[derive(Clone, Debug)]
pub struct SceneNodeRef {
    pub identity: NodeIdentity,
    pub placement: Placement,
    pub lod: LodLevel,
    pub size: Size,
    pub content_kind: NodeContentKind,
    /// Optional per-node renderer pin. When set, the registry's
    /// selector chain (step 1 of [renderer-registry contract brief
    /// §5]'s five-step chain) honors the pin if a renderer with that
    /// id is registered and handles the node's content kind; the
    /// chain falls through to subsequent steps when the pin doesn't
    /// match a registered renderer.
    ///
    /// [renderer-registry contract brief §5]: ../../../../design_docs/mere_docs/research/2026-05-15_renderer_registry_contract_brief.md
    pub renderer_pin: Option<RendererId>,
}
