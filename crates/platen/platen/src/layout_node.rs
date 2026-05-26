// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Morphorm glue — the minimal [`morphorm::Node`] / [`morphorm::Cache`]
//! implementation platen layers over its own tree.
//!
//! Morphorm stores layout properties and the tree externally from the node
//! type; here we take the *self-owned children* route ([`LayoutNode`] owns a
//! `Vec` of children, so `Store` and `Tree` are both `()`). Only the handful
//! of properties the between-tiles arrangement uses — layout type, size, and
//! gap — are read from the node; every other layout property returns `None`
//! (morphorm treats that as unset). The cache is a flat `id → rect` map read
//! back after [`morphorm::Node::layout`] runs.
//!
//! This file is deliberately mechanical. The mapping from a forme arrangement
//! (via platen's [`WorkbenchPlan`](crate::WorkbenchPlan)) into a tree of these
//! nodes — the part that carries meaning — lives in [`crate::layout`].

use std::collections::HashMap;

use morphorm::{Cache, LayoutType, Node, Units};

/// One node in platen's between-tiles layout tree. Owns its children directly
/// (morphorm's optional-external-tree contract with `Tree = ()`), so the whole
/// arrangement is a plain recursive value.
#[derive(Debug, Clone)]
pub(crate) struct LayoutNode {
    /// Cache key — unique within one layout pass.
    pub id: usize,
    pub layout_type: LayoutType,
    pub width: Units,
    pub height: Units,
    /// Gap between children on both axes (splitter / tab-strip spacing).
    pub gap: Units,
    pub children: Vec<LayoutNode>,
}

impl LayoutNode {
    pub(crate) fn leaf(id: usize, width: Units, height: Units) -> Self {
        Self { id, layout_type: LayoutType::Column, width, height, gap: Units::Pixels(0.0), children: Vec::new() }
    }

    pub(crate) fn container(
        id: usize,
        layout_type: LayoutType,
        width: Units,
        height: Units,
        gap: Units,
        children: Vec<LayoutNode>,
    ) -> Self {
        Self { id, layout_type, width, height, gap, children }
    }
}

/// Flat `id → bounds` store. Morphorm writes through [`Cache::set_bounds`]; the
/// mapping reads bounds back by node id after the pass.
#[derive(Debug, Default)]
pub(crate) struct RectCache {
    bounds: HashMap<usize, MorphormRect>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct MorphormRect {
    pub posx: f32,
    pub posy: f32,
    pub width: f32,
    pub height: f32,
}

impl RectCache {
    pub(crate) fn bounds(&self, id: usize) -> MorphormRect {
        self.bounds.get(&id).copied().unwrap_or_default()
    }
}

impl Cache for RectCache {
    type Node = LayoutNode;

    fn set_bounds(&mut self, node: &Self::Node, posx: f32, posy: f32, width: f32, height: f32) {
        self.bounds.insert(node.id, MorphormRect { posx, posy, width, height });
    }

    fn width(&self, node: &Self::Node) -> f32 {
        self.bounds(node.id).width
    }

    fn height(&self, node: &Self::Node) -> f32 {
        self.bounds(node.id).height
    }

    fn posx(&self, node: &Self::Node) -> f32 {
        self.bounds(node.id).posx
    }

    fn posy(&self, node: &Self::Node) -> f32 {
        self.bounds(node.id).posy
    }
}

impl Node for LayoutNode {
    type Store = ();
    type Tree = ();
    type ChildIter<'t> = std::slice::Iter<'t, LayoutNode>;
    type CacheKey = usize;
    type SubLayout<'a> = ();

    fn key(&self) -> Self::CacheKey {
        self.id
    }

    fn children<'t>(&'t self, _tree: &'t Self::Tree) -> Self::ChildIter<'t> {
        self.children.iter()
    }

    fn visible(&self, _store: &Self::Store) -> bool {
        true
    }

    fn layout_type(&self, _store: &Self::Store) -> Option<LayoutType> {
        Some(self.layout_type)
    }

    fn width(&self, _store: &Self::Store) -> Option<Units> {
        Some(self.width)
    }

    fn height(&self, _store: &Self::Store) -> Option<Units> {
        Some(self.height)
    }

    fn horizontal_gap(&self, _store: &Self::Store) -> Option<Units> {
        Some(self.gap)
    }

    fn vertical_gap(&self, _store: &Self::Store) -> Option<Units> {
        Some(self.gap)
    }

    // ── Properties the between-tiles arrangement does not use. Morphorm reads
    //    `None` as unset and falls back to its own defaults. ────────────────
    fn position_type(&self, _store: &Self::Store) -> Option<morphorm::PositionType> { None }
    fn alignment(&self, _store: &Self::Store) -> Option<morphorm::Alignment> { None }
    fn left(&self, _store: &Self::Store) -> Option<Units> { None }
    fn right(&self, _store: &Self::Store) -> Option<Units> { None }
    fn top(&self, _store: &Self::Store) -> Option<Units> { None }
    fn bottom(&self, _store: &Self::Store) -> Option<Units> { None }
    fn content_size(
        &self,
        _store: &Self::Store,
        _sublayout: &mut Self::SubLayout<'_>,
        _width: Option<f32>,
        _height: Option<f32>,
    ) -> Option<(f32, f32)> {
        None
    }
    fn padding_left(&self, _store: &Self::Store) -> Option<Units> { None }
    fn padding_right(&self, _store: &Self::Store) -> Option<Units> { None }
    fn padding_top(&self, _store: &Self::Store) -> Option<Units> { None }
    fn padding_bottom(&self, _store: &Self::Store) -> Option<Units> { None }
    fn min_vertical_gap(&self, _store: &Self::Store) -> Option<Units> { None }
    fn min_horizontal_gap(&self, _store: &Self::Store) -> Option<Units> { None }
    fn max_vertical_gap(&self, _store: &Self::Store) -> Option<Units> { None }
    fn max_horizontal_gap(&self, _store: &Self::Store) -> Option<Units> { None }
    fn min_width(&self, _store: &Self::Store) -> Option<Units> { None }
    fn min_height(&self, _store: &Self::Store) -> Option<Units> { None }
    fn max_width(&self, _store: &Self::Store) -> Option<Units> { None }
    fn max_height(&self, _store: &Self::Store) -> Option<Units> { None }
    fn border_left(&self, _store: &Self::Store) -> Option<Units> { None }
    fn border_right(&self, _store: &Self::Store) -> Option<Units> { None }
    fn border_top(&self, _store: &Self::Store) -> Option<Units> { None }
    fn border_bottom(&self, _store: &Self::Store) -> Option<Units> { None }
    fn vertical_scroll(&self, _store: &Self::Store) -> Option<f32> { None }
    fn horizontal_scroll(&self, _store: &Self::Store) -> Option<f32> { None }
    fn grid_columns(&self, _store: &Self::Store) -> Option<Vec<Units>> { None }
    fn grid_rows(&self, _store: &Self::Store) -> Option<Vec<Units>> { None }
    fn column_start(&self, _store: &Self::Store) -> Option<usize> { None }
    fn row_start(&self, _store: &Self::Store) -> Option<usize> { None }
    fn column_span(&self, _store: &Self::Store) -> Option<usize> { None }
    fn row_span(&self, _store: &Self::Store) -> Option<usize> { None }
}
