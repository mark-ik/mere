// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Scene contract — the minimal substrate-side types renderers see.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

use kurbo::{Affine, Size};

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
    pub const IDENTITY: Self = Self { transform: Affine::IDENTITY };

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
    Panel,
    Knot,
    DocumentTile,
    CustomCanvas,
    Composite,
    EdgeRendering,
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
#[derive(Copy, Clone, Debug)]
pub struct SceneNodeRef {
    pub identity: NodeIdentity,
    pub placement: Placement,
    pub lod: LodLevel,
    pub size: Size,
    pub content_kind: NodeContentKind,
}
