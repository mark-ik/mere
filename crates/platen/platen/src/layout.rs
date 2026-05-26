// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Between-tiles layout — platen's geometry authority for the workbench.
//!
//! Per the composition spine, `forme` owns *what a workbench is* and platen's
//! [`project_tree`](crate::project_tree) projects it into a geometry-free
//! [`WorkbenchPlan`](crate::WorkbenchPlan) (slots of tab-stacks). This module
//! takes the final step: it assigns each slot a concrete rect inside a
//! viewport, using [morphorm](https://crates.io/crates/morphorm) as the layout
//! engine. The seam is deliberate — **platen owns the arrangement *between*
//! tiles; the host's Masonry widgets own the content *within* each tile.**
//!
//! The mapping is node-for-node:
//!
//! | `WorkbenchPlan`            | morphorm tree                                   |
//! |---------------------------|-------------------------------------------------|
//! | root (slots side by side) | `Row`, children stretch equally, gap = splitter |
//! | `PlanSlot::Tile`          | a stretch leaf                                  |
//! | `PlanSlot::Tabs`          | a `Column`: a fixed-height strip + a stretch content leaf |
//!
//! Split ratios (per-slot weights) are projection-state, not structure; v1
//! lays every slot out with equal weight. Nested splits arrive when
//! [`project_tree`](crate::project_tree) surfaces groups.

use kernel::geometry::{PortablePoint, PortableRect, PortableSize};
use morphorm::{LayoutType, Node, Units};

use crate::layout_node::{LayoutNode, MorphormRect, RectCache};
use crate::tree_projection::{PlanSlot, TilePlan, WorkbenchPlan};

/// Tunables for the between-tiles layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutConfig {
    /// Space between sibling slots — the splitter gutter, in pixels.
    pub gap: f32,
    /// Height of a tab-stack's tab strip, in pixels.
    pub tab_strip_height: f32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self { gap: 6.0, tab_strip_height: 28.0 }
    }
}

/// A slot placed in the viewport. Mirrors [`PlanSlot`] with concrete rects.
#[derive(Clone, Debug, PartialEq)]
pub enum LaidOutSlot {
    /// A single tile occupying its rect.
    Tile { tile: TilePlan, rect: PortableRect },
    /// A tab-stack: the whole slot rect, the tab strip across the top, and the
    /// content rect below it where the active tab (the first) renders.
    Tabs { tabs: Vec<TilePlan>, rect: PortableRect, strip: PortableRect, content: PortableRect },
}

/// A fully placed workbench: every slot with its viewport rect.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidOutPlan {
    pub viewport: PortableRect,
    pub slots: Vec<LaidOutSlot>,
}

impl LaidOutPlan {
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// How to read one slot's morphorm node(s) back into a [`LaidOutSlot`].
enum SlotIds {
    Tile { tile: TilePlan, node: usize },
    Tabs { tabs: Vec<TilePlan>, node: usize, strip: usize, content: usize },
}

/// Lay `plan` out inside `viewport`, returning a rect per slot.
pub fn layout_plan(plan: &WorkbenchPlan, viewport: PortableSize, config: &LayoutConfig) -> LaidOutPlan {
    let mut next_id = 0usize;
    let mut id = || {
        let i = next_id;
        next_id += 1;
        i
    };

    let root_id = id();
    let mut children = Vec::with_capacity(plan.slots.len());
    let mut slot_ids = Vec::with_capacity(plan.slots.len());

    for slot in &plan.slots {
        match slot {
            PlanSlot::Tile(tile) => {
                let node = id();
                children.push(LayoutNode::leaf(node, Units::Stretch(1.0), Units::Stretch(1.0)));
                slot_ids.push(SlotIds::Tile { tile: tile.clone(), node });
            }
            PlanSlot::Tabs(tabs) => {
                let node = id();
                let strip = id();
                let content = id();
                let column = LayoutNode::container(
                    node,
                    LayoutType::Column,
                    Units::Stretch(1.0),
                    Units::Stretch(1.0),
                    Units::Pixels(0.0),
                    vec![
                        LayoutNode::leaf(strip, Units::Stretch(1.0), Units::Pixels(config.tab_strip_height)),
                        LayoutNode::leaf(content, Units::Stretch(1.0), Units::Stretch(1.0)),
                    ],
                );
                children.push(column);
                slot_ids.push(SlotIds::Tabs { tabs: tabs.clone(), node, strip, content });
            }
        }
    }

    let root = LayoutNode::container(
        root_id,
        LayoutType::Row,
        Units::Pixels(viewport.width),
        Units::Pixels(viewport.height),
        Units::Pixels(config.gap),
        children,
    );

    let mut cache = RectCache::default();
    root.layout(&mut cache, &(), &(), &mut ());

    // Root sits at the origin, so its direct children's rects are already
    // viewport-absolute. A tab-stack's strip/content are relative to their
    // slot column, so offset them by the slot's origin.
    let slots = slot_ids
        .into_iter()
        .map(|s| match s {
            SlotIds::Tile { tile, node } => LaidOutSlot::Tile { tile, rect: rect_of(&cache, node) },
            SlotIds::Tabs { tabs, node, strip, content } => {
                let slot_rect = rect_of(&cache, node);
                let origin = slot_rect.origin;
                LaidOutSlot::Tabs {
                    tabs,
                    rect: slot_rect,
                    strip: offset_rect(rect_of(&cache, strip), origin),
                    content: offset_rect(rect_of(&cache, content), origin),
                }
            }
        })
        .collect();

    LaidOutPlan {
        viewport: PortableRect::new(PortablePoint::zero(), viewport),
        slots,
    }
}

fn rect_of(cache: &RectCache, node: usize) -> PortableRect {
    to_portable(cache.bounds(node))
}

fn to_portable(r: MorphormRect) -> PortableRect {
    PortableRect::new(PortablePoint::new(r.posx, r.posy), PortableSize::new(r.width, r.height))
}

fn offset_rect(rect: PortableRect, by: PortablePoint) -> PortableRect {
    PortableRect::new(PortablePoint::new(rect.origin.x + by.x, rect.origin.y + by.y), rect.size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forme::Arrangement;
    use uuid::Uuid;

    use crate::tree_projection::project_tree;

    fn viewport(w: f32, h: f32) -> PortableSize {
        PortableSize::new(w, h)
    }

    #[test]
    fn empty_plan_lays_out_nothing() {
        let plan = project_tree(&Arrangement::new());
        let out = layout_plan(&plan, viewport(800.0, 600.0), &LayoutConfig::default());
        assert!(out.is_empty());
    }

    #[test]
    fn two_tiles_split_the_width_with_a_gap() {
        let mut a = Arrangement::new();
        a.add_tile_intent(Some(Uuid::from_u128(1)));
        a.add_tile_intent(Some(Uuid::from_u128(2)));
        let plan = project_tree(&a);
        let cfg = LayoutConfig { gap: 10.0, tab_strip_height: 28.0 };
        let out = layout_plan(&plan, viewport(810.0, 600.0), &cfg);

        assert_eq!(out.slots.len(), 2);
        let rects: Vec<PortableRect> = out
            .slots
            .iter()
            .map(|s| match s {
                LaidOutSlot::Tile { rect, .. } => *rect,
                LaidOutSlot::Tabs { rect, .. } => *rect,
            })
            .collect();
        // (810 - 10 gap) / 2 = 400 each, full height.
        assert_eq!(rects[0].size.width, 400.0);
        assert_eq!(rects[1].size.width, 400.0);
        assert_eq!(rects[0].size.height, 600.0);
        // Second slot begins after the first + gap.
        assert_eq!(rects[0].origin.x, 0.0);
        assert_eq!(rects[1].origin.x, 410.0);
    }

    #[test]
    fn tab_stack_splits_strip_from_content() {
        let mut a = Arrangement::new();
        let t1 = a.add_tile_intent(Some(Uuid::from_u128(1)));
        let t2 = a.add_tile_intent(Some(Uuid::from_u128(2)));
        a.stack(t1, t2);
        let plan = project_tree(&a);
        let cfg = LayoutConfig { gap: 0.0, tab_strip_height: 30.0 };
        let out = layout_plan(&plan, viewport(500.0, 400.0), &cfg);

        assert_eq!(out.slots.len(), 1);
        match &out.slots[0] {
            LaidOutSlot::Tabs { rect, strip, content, tabs } => {
                assert_eq!(tabs.len(), 2);
                assert_eq!(rect.size, PortableSize::new(500.0, 400.0));
                // Strip across the top, content fills the rest.
                assert_eq!(strip.origin, PortablePoint::new(0.0, 0.0));
                assert_eq!(strip.size, PortableSize::new(500.0, 30.0));
                assert_eq!(content.origin, PortablePoint::new(0.0, 30.0));
                assert_eq!(content.size, PortableSize::new(500.0, 370.0));
            }
            other => panic!("expected Tabs, got {other:?}"),
        }
    }

    #[test]
    fn tab_stack_after_a_tile_is_offset_to_its_slot() {
        // A solo tile, then a two-tab stack. The stack is the *second* slot, so
        // its strip/content must be offset by the slot's x-origin — the path the
        // single-slot test above never exercises.
        let mut a = Arrangement::new();
        a.add_tile_intent(Some(Uuid::from_u128(1)));
        let t2 = a.add_tile_intent(Some(Uuid::from_u128(2)));
        let t3 = a.add_tile_intent(Some(Uuid::from_u128(3)));
        a.stack(t2, t3);
        let plan = project_tree(&a);
        let cfg = LayoutConfig { gap: 10.0, tab_strip_height: 30.0 };
        let out = layout_plan(&plan, viewport(810.0, 400.0), &cfg);

        assert_eq!(out.slots.len(), 2);
        // (810 - 10 gap) / 2 = 400; second slot starts at 410.
        let slot_x = match &out.slots[1] {
            LaidOutSlot::Tabs { rect, .. } => rect.origin.x,
            other => panic!("expected Tabs as slot 1, got {other:?}"),
        };
        assert_eq!(slot_x, 410.0);
        match &out.slots[1] {
            LaidOutSlot::Tabs { strip, content, rect, .. } => {
                assert_eq!(rect.size, PortableSize::new(400.0, 400.0));
                // Strip/content sit at the slot's x, not the viewport origin.
                assert_eq!(strip.origin, PortablePoint::new(410.0, 0.0));
                assert_eq!(strip.size, PortableSize::new(400.0, 30.0));
                assert_eq!(content.origin, PortablePoint::new(410.0, 30.0));
                assert_eq!(content.size, PortableSize::new(400.0, 370.0));
            }
            other => panic!("expected Tabs, got {other:?}"),
        }
    }

    #[test]
    fn slots_cover_the_viewport_height() {
        let mut a = Arrangement::new();
        a.add_tile_intent(Some(Uuid::from_u128(1)));
        let plan = project_tree(&a);
        let out = layout_plan(&plan, viewport(300.0, 720.0), &LayoutConfig::default());
        match &out.slots[0] {
            LaidOutSlot::Tile { rect, .. } => {
                assert_eq!(rect.size.width, 300.0);
                assert_eq!(rect.size.height, 720.0);
            }
            other => panic!("expected Tile, got {other:?}"),
        }
    }
}
