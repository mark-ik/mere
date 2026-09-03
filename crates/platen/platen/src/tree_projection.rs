// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Tree projection — platen's first projection of a forme [`Arrangement`]
//! into a workbench presentation plan.
//!
//! Per the composition spine: `forme` owns *what a workbench is* (a
//! graph-capable, geometry-free arrangement); **platen projects** it into a
//! concrete plan a host renders. The *tree* projection is the conventional
//! tiled workbench — splits of tab-stacks. Other projections (cartography,
//! lattice, corridor) are separate functions, added when a surface needs one.
//!
//! v1 scope (small, grows on demand): projects the **root-level** tile-bearing
//! members into an ordered list of slots laid side-by-side (split); members
//! joined by `StackedWith` collapse into a tab-stack. Groups / nested splits
//! are a later refinement (tiles inside a `Group` aren't surfaced yet).
//! Geometry (split ratios) is **not** here — it's projection-state keyed
//! `(FormeRef, ProjectionKind)`; this plan is geometry-free structure.

use std::collections::HashMap;

use forme::{
    Arrangement, ArrangementEdgeKind, ArrangementNode, ArrangementNodeId, ArrangementNodeKind,
    GraphMemberId,
};
use serde::{Deserialize, Serialize};

/// Which projection a forme is presented through. Keys projection geometry
/// (`(FormeRef, ProjectionKind)`) and selects platen's projection function.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectionKind {
    /// Conventional tiled workbench — splits of tab-stacks ([`project_tree`]).
    Tree,
    /// The orrery — graph members positioned spatially (cartography).
    Cartography,
}

/// One tile in a tree-projected workbench.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TilePlan {
    /// The arrangement node this tile realizes.
    pub node: ArrangementNodeId,
    /// The graph member it represents, if bound.
    pub member: Option<GraphMemberId>,
    pub label: Option<String>,
}

/// A top-level workbench slot, laid side-by-side (split) with its siblings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanSlot {
    /// A single tile.
    Tile(TilePlan),
    /// A tab-stack (`StackedWith`); the first is the default-active tab.
    Tabs(Vec<TilePlan>),
}

/// The tree projection of a forme arrangement: slots laid left-to-right.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchPlan {
    pub slots: Vec<PlanSlot>,
}

impl WorkbenchPlan {
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Total tiles across all slots.
    pub fn tile_count(&self) -> usize {
        self.slots
            .iter()
            .map(|s| match s {
                PlanSlot::Tile(_) => 1,
                PlanSlot::Tabs(v) => v.len(),
            })
            .sum()
    }
}

/// Project `arrangement` into a tree-shaped [`WorkbenchPlan`].
pub fn project_tree(arrangement: &Arrangement) -> WorkbenchPlan {
    let root = arrangement.root();
    // Root-level tile-bearing members, in arrangement order (members_of
    // preserves MemberOf-edge insertion order).
    let ordered: Vec<TilePlan> = arrangement
        .members_of(root)
        .filter_map(|id| arrangement.node(id).and_then(tile_plan))
        .collect();

    let mut slots: Vec<PlanSlot> = Vec::new();
    let mut slot_of: HashMap<ArrangementNodeId, usize> = HashMap::new();

    for tile in ordered {
        match stacked_slot(arrangement, tile.node, &slot_of) {
            Some(i) => {
                match &mut slots[i] {
                    PlanSlot::Tile(first) => {
                        let first = first.clone();
                        slots[i] = PlanSlot::Tabs(vec![first, tile.clone()]);
                    }
                    PlanSlot::Tabs(v) => v.push(tile.clone()),
                }
                slot_of.insert(tile.node, i);
            }
            None => {
                let i = slots.len();
                slot_of.insert(tile.node, i);
                slots.push(PlanSlot::Tile(tile));
            }
        }
    }

    WorkbenchPlan { slots }
}

/// Project a [`WorkbenchPlan`] onto Genet's [`TileTree`] contract (V5/V6), the seam
/// that lets a host use Genet's tile surface as its workbench pane.
///
/// The plan owns the **structure** (the left-to-right slots, each a tile or a
/// tab-stack); the host owns each tile's **resolution** — its stable [`TileId`], title,
/// and content lane (a member's actor texture is an
/// [`ExternalTexture`](workbench::ContentSource::ExternalTexture), a document is a
/// [`Document`](workbench::ContentSource::Document)) — supplied by `tile_for`. So
/// this stays a pure projection: forme remains the arrangement authority, this never
/// writes back (mere applies tile events to forme and re-projects).
///
/// An empty plan yields `None` (the surface has nothing to show). A single slot maps to
/// that slot's stack directly (no enclosing split); multiple slots map to a `Row` split
/// with **equal** fractions — the plan is geometry-free, so split ratios start even and
/// a [`DividerMoved`](workbench::TileEvent::DividerMoved) refines them (the real
/// ratios live in platen's projection-state, applied by the host).
pub fn tile_tree_from_plan(
    plan: &WorkbenchPlan,
    mut tile_for: impl FnMut(&TilePlan) -> workbench::Tile,
) -> Option<workbench::TileTree> {
    use workbench::{SplitAxis, TileBranch, TileTree};

    if plan.slots.is_empty() {
        return None;
    }
    let slot_tree =
        |slot: &PlanSlot, tile_for: &mut dyn FnMut(&TilePlan) -> workbench::Tile| match slot {
            PlanSlot::Tile(t) => TileTree::single(tile_for(t)),
            PlanSlot::Tabs(v) => TileTree::stack(v.iter().map(|t| tile_for(t)).collect(), 0),
        };

    if plan.slots.len() == 1 {
        return Some(slot_tree(&plan.slots[0], &mut tile_for));
    }
    let fraction = 1.0 / plan.slots.len() as f32;
    let children = plan
        .slots
        .iter()
        .map(|slot| TileBranch::new(fraction, slot_tree(slot, &mut tile_for)))
        .collect();
    Some(TileTree::split(SplitAxis::Row, children))
}

/// A leaf tile plan for member/tile-intent nodes; `None` for non-leaf kinds
/// (root, group, portal, collapsed-graphlet — not surfaced in the v1 tree).
fn tile_plan(node: &ArrangementNode) -> Option<TilePlan> {
    let member = match &node.kind {
        ArrangementNodeKind::MemberIntent { member } => Some(*member),
        ArrangementNodeKind::TileIntent { member } => *member,
        _ => return None,
    };
    Some(TilePlan {
        node: node.id,
        member,
        label: node.label.clone(),
    })
}

/// If `node` is `StackedWith` an already-placed tile, return that tile's slot.
fn stacked_slot(
    arrangement: &Arrangement,
    node: ArrangementNodeId,
    slot_of: &HashMap<ArrangementNodeId, usize>,
) -> Option<usize> {
    arrangement.edges().find_map(|e| {
        if e.kind != ArrangementEdgeKind::StackedWith {
            return None;
        }
        if e.from == node {
            slot_of.get(&e.to).copied()
        } else if e.to == node {
            slot_of.get(&e.from).copied()
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn empty_arrangement_projects_no_slots() {
        let plan = project_tree(&Arrangement::new());
        assert!(plan.is_empty());
        assert_eq!(plan.tile_count(), 0);
    }

    #[test]
    fn unstacked_tiles_become_split_slots_in_order() {
        let mut a = Arrangement::new();
        a.add_tile_intent(Some(Uuid::from_u128(1)));
        a.add_tile_intent(Some(Uuid::from_u128(2)));
        a.add_member_intent(Uuid::from_u128(3));
        let plan = project_tree(&a);
        assert_eq!(plan.slots.len(), 3);
        assert_eq!(plan.tile_count(), 3);
        assert!(plan.slots.iter().all(|s| matches!(s, PlanSlot::Tile(_))));
    }

    #[test]
    fn stacked_tiles_collapse_into_one_tab_slot() {
        let mut a = Arrangement::new();
        let t1 = a.add_tile_intent(Some(Uuid::from_u128(1)));
        let t2 = a.add_tile_intent(Some(Uuid::from_u128(2)));
        let t3 = a.add_tile_intent(Some(Uuid::from_u128(3)));
        a.stack(t1, t2);
        a.stack(t2, t3);
        let plan = project_tree(&a);
        // All three stacked → one Tabs slot.
        assert_eq!(plan.slots.len(), 1);
        match &plan.slots[0] {
            PlanSlot::Tabs(v) => assert_eq!(v.len(), 3),
            other => panic!("expected Tabs, got {other:?}"),
        }
    }

    #[test]
    fn mix_of_split_and_tab() {
        let mut a = Arrangement::new();
        let _solo = a.add_tile_intent(Some(Uuid::from_u128(1)));
        let t2 = a.add_tile_intent(Some(Uuid::from_u128(2)));
        let t3 = a.add_tile_intent(Some(Uuid::from_u128(3)));
        a.stack(t2, t3);
        let plan = project_tree(&a);
        // solo Tile + (t2,t3) Tabs = 2 slots, 3 tiles.
        assert_eq!(plan.slots.len(), 2);
        assert_eq!(plan.tile_count(), 3);
        assert!(matches!(plan.slots[0], PlanSlot::Tile(_)));
        assert!(matches!(plan.slots[1], PlanSlot::Tabs(_)));
    }

    #[test]
    fn groups_and_root_are_not_leaf_tiles() {
        let mut a = Arrangement::new();
        a.add_group(Some("g".into()));
        let plan = project_tree(&a);
        // Group isn't a leaf tile; root isn't either.
        assert!(plan.is_empty());
    }

    #[test]
    fn plan_round_trips_through_json() {
        let mut a = Arrangement::new();
        a.add_tile_intent(Some(Uuid::from_u128(7)));
        let plan = project_tree(&a);
        let json = serde_json::to_string(&plan).unwrap();
        let back: WorkbenchPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan, back);
    }

    // ── WorkbenchPlan -> Genet TileTree projection (the V6 surface seam) ──

    use workbench::{ContentSource, DocumentRef, SplitAxis, Tile, TileId, TileTree};

    /// A host resolver standing in for meerkat's: sequential tile ids, the label as
    /// title, a document lane. (The real host resolves a member to its actor-texture
    /// key or document subtree; the projection is agnostic to which.)
    fn make_tile_for() -> impl FnMut(&TilePlan) -> Tile {
        let mut next = 0u64;
        move |t: &TilePlan| {
            next += 1;
            Tile {
                id: TileId(next),
                title: t.label.clone().unwrap_or_default(),
                content: ContentSource::Document(DocumentRef(String::new())),
                accent: None,
            }
        }
    }

    /// An empty arrangement projects to no tree (the surface shows nothing).
    #[test]
    fn empty_plan_projects_to_no_tree() {
        let plan = project_tree(&Arrangement::new());
        assert!(tile_tree_from_plan(&plan, make_tile_for()).is_none());
    }

    /// A single slot maps to its stack directly — no enclosing split.
    #[test]
    fn single_slot_projects_to_a_bare_stack() {
        let mut a = Arrangement::new();
        a.add_tile_intent(Some(Uuid::from_u128(1)));
        let plan = project_tree(&a);
        let tree = tile_tree_from_plan(&plan, make_tile_for()).expect("one slot -> a tree");
        assert!(
            matches!(tree, TileTree::Stack(_)),
            "a lone slot is the stack itself"
        );
        assert_eq!(tree.tiles().len(), 1);
    }

    /// A `StackedWith` slot maps to a tabbed stack with the first tab active.
    #[test]
    fn stacked_slot_projects_to_a_tabbed_stack() {
        let mut a = Arrangement::new();
        let t1 = a.add_tile_intent(Some(Uuid::from_u128(1)));
        let t2 = a.add_tile_intent(Some(Uuid::from_u128(2)));
        a.stack(t1, t2);
        let plan = project_tree(&a);
        let tree = tile_tree_from_plan(&plan, make_tile_for()).expect("tree");
        match tree {
            TileTree::Stack(s) => {
                assert_eq!(s.tabs.len(), 2, "both stacked tiles are tabs");
                assert_eq!(s.active, 0, "the first is active");
            }
            other => panic!("expected a stack, got {other:?}"),
        }
    }

    /// Several unstacked slots map to an even `Row` split, one branch each.
    #[test]
    fn multiple_slots_project_to_an_even_row_split() {
        let mut a = Arrangement::new();
        a.add_tile_intent(Some(Uuid::from_u128(1)));
        a.add_tile_intent(Some(Uuid::from_u128(2)));
        a.add_tile_intent(Some(Uuid::from_u128(3)));
        let plan = project_tree(&a);
        let tree = tile_tree_from_plan(&plan, make_tile_for()).expect("tree");
        match tree {
            TileTree::Split { axis, children } => {
                assert_eq!(axis, SplitAxis::Row, "side-by-side slots are a row");
                assert_eq!(children.len(), 3);
                for b in &children {
                    assert!(
                        (b.fraction - 1.0 / 3.0).abs() < 1e-6,
                        "even shares, got {}",
                        b.fraction
                    );
                }
            }
            other => panic!("expected a row split, got {other:?}"),
        }
        assert_eq!(
            tile_tree_from_plan(&plan, make_tile_for())
                .unwrap()
                .tiles()
                .len(),
            3
        );
    }
}
