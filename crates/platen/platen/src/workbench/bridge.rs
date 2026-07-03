/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The Workbench ⇄ forme bridge: derive a geometry-free [`Arrangement`] plus a
//! Tree-projection [`TreeGeometry`] from the live split tree, and rebuild the
//! split tree from that pair — losslessly.
//!
//! Per the composition spine (§9): the `(Arrangement, TreeGeometry)` pair is the
//! *persisted / canonical* form (forme owns the semantic arrangement, the
//! geometry store owns the per-projection layout), and the [`Workbench`]'s `Pane`
//! split tree is the *live working model* — the derived cache the host mutates
//! (the verified tiling logic in [`super::tree`]). [`Workbench::to_arrangement`]
//! derives the canonical pair after an edit (to persist); [`Workbench::from_arrangement`]
//! rebuilds the working tree on load. With the geometry present the rebuild is
//! exact; without it the workbench falls back to the arrangement's default flat
//! projection ([`project_tree`]) — the "responsive from one saved geometry" story.
//!
//! Boundary: forme's arrangement stays honestly semantic — membership
//! (`MemberIntent`) + tab grouping (`StackedWith`); the *tree shape* (axis,
//! nesting, order), fractions, and active tab live in the geometry sidecar,
//! because "the tree shape is a projection, not arrangement truth" (spine §2).

use std::collections::{HashMap, HashSet};

use forme::{Arrangement, ArrangementNodeId, GraphMemberId};
use serde::{Deserialize, Serialize};

use super::Workbench;
use super::tree::{Branch, Pane, Stack};
use crate::ProjectionKind;
use crate::projection_geometry::{TreeBranch, TreeGeometry};
use crate::tree_projection::{PlanSlot, project_tree};

/// The on-disk form of a tiled workbench: the canonical `(Arrangement, geometry)`
/// pair the bridge derives, written beside the session graph so split shape, tab
/// stacks, and the active tab survive a restart. The live `Pane` tree itself is
/// not serde - it is the derived working model, rebuilt from this on load.
#[derive(Serialize, Deserialize)]
struct PersistedWorkbench {
    arrangement: Arrangement,
    geometry: Option<TreeGeometry>,
}

impl Workbench {
    /// Derive the canonical `(Arrangement, TreeGeometry)` pair from the live split
    /// tree: the arrangement is the semantic content (each open member a
    /// `MemberIntent`, members sharing a cell joined by `StackedWith`), the
    /// geometry is the Tree-projection layout (the split skeleton, fractions, and
    /// active tab). The geometry is `None` for an empty workbench. The inverse of
    /// [`from_arrangement`](Self::from_arrangement); the two round-trip.
    pub fn to_arrangement(&self) -> (Arrangement, Option<TreeGeometry>) {
        let mut arr = Arrangement::new();
        if let Some(root) = &self.root {
            let mut node_of: HashMap<GraphMemberId, ArrangementNodeId> = HashMap::new();
            build_arrangement(root, &mut arr, &mut node_of);
        }
        let geom = self.root.as_ref().map(pane_to_geom);
        (arr, geom)
    }

    /// Rebuild a tiled (Tree-mode) workbench from a canonical pair. With `geom`
    /// present the split tree is reconstructed exactly (reconciled against the
    /// arrangement's membership — a geometry leaf the arrangement no longer
    /// carries is dropped, its cell collapsed); with `geom` `None` the workbench
    /// takes the arrangement's default flat projection ([`project_tree`]: each
    /// root member a side-by-side cell, `StackedWith` members collapsed into a
    /// tab-stack). Always lands in [`ProjectionKind::Tree`].
    pub fn from_arrangement(arrangement: &Arrangement, geom: Option<&TreeGeometry>) -> Self {
        let root = match geom {
            Some(g) => {
                let allowed: HashSet<GraphMemberId> =
                    arrangement.referenced_members().into_iter().collect();
                let mut g = g.clone();
                g.sanitize();
                let mut pane = geom_to_pane(&g);
                // Reconcile: drop any geometry member the arrangement dropped,
                // reusing the tree's verified remove + collapse.
                let mut members = Vec::new();
                pane.collect_members(&mut members);
                for m in members {
                    if !allowed.contains(&m) {
                        pane.remove(m);
                    }
                }
                (!pane.is_empty()).then_some(pane)
            }
            None => flat_pane(arrangement),
        };
        Workbench {
            mode: ProjectionKind::Tree,
            root,
        }
    }

    /// The A4 invariant: the live `Pane` tree holds nothing the canonical
    /// `(Arrangement, geometry)` pair cannot reproduce, i.e.
    /// `from_arrangement(to_arrangement(self))` rebuilds an identical split tree. This
    /// is what lets the Pane tree be a pure *derived working cache* over the canonical
    /// pair (the workbench half of "two projections of one arrangement"). Fuzz-proven
    /// over random gesture sequences (see the property test) and asserted on every
    /// persist in debug builds.
    pub fn canonical_roundtrips(&self) -> bool {
        let (arrangement, geometry) = self.to_arrangement();
        Workbench::from_arrangement(&arrangement, geometry.as_ref()).root == self.root
    }

    /// Serialize this workbench's canonical `(Arrangement, geometry)` pair to JSON for
    /// session persistence. An empty workbench still round-trips (a `None` geometry).
    /// The host writes the string beside the session graph.
    pub fn to_persisted_json(&self) -> Result<String, serde_json::Error> {
        debug_assert!(
            self.canonical_roundtrips(),
            "workbench Pane tree is not canonical-derivable from its (Arrangement, geometry) pair",
        );
        let (arrangement, geometry) = self.to_arrangement();
        serde_json::to_string(&PersistedWorkbench {
            arrangement,
            geometry,
        })
    }

    /// Rebuild a tiled workbench from session-persisted JSON, keeping only members in
    /// `present` (the live graph's members) so a tile whose node was deleted between
    /// sessions is reconciled away and its cell collapsed. `None` on a parse error -
    /// the host falls back to an empty [`Workbench::new`].
    pub fn from_persisted_json(json: &str, present: &HashSet<GraphMemberId>) -> Option<Self> {
        let pw: PersistedWorkbench = serde_json::from_str(json).ok()?;
        let mut wb = Workbench::from_arrangement(&pw.arrangement, pw.geometry.as_ref());
        for member in wb.open_members() {
            if !present.contains(&member) {
                wb.close_tile(member);
            }
        }
        Some(wb)
    }
}

/// A `Pane` split tree mirrored as geometry (structure + fractions + active).
fn pane_to_geom(pane: &Pane) -> TreeGeometry {
    match pane {
        Pane::Stack(s) => TreeGeometry::Stack {
            members: s.members.clone(),
            active: s.active,
        },
        Pane::Split { axis, children } => TreeGeometry::Split {
            axis: (*axis).into(),
            children: children
                .iter()
                .map(|b| TreeBranch {
                    fraction: b.fraction,
                    node: pane_to_geom(&b.pane),
                })
                .collect(),
        },
    }
}

/// Geometry mirrored back into a `Pane` split tree (the inverse of [`pane_to_geom`]).
fn geom_to_pane(geom: &TreeGeometry) -> Pane {
    match geom {
        TreeGeometry::Stack { members, active } => Pane::Stack(Stack {
            members: members.clone(),
            active: *active,
        }),
        TreeGeometry::Split { axis, children } => Pane::Split {
            axis: (*axis).into(),
            children: children
                .iter()
                .map(|b| Branch {
                    fraction: b.fraction,
                    pane: geom_to_pane(&b.node),
                })
                .collect(),
        },
    }
}

/// Walk the tree, registering each member as a `MemberIntent` and joining members
/// that share a cell with `StackedWith` (the semantic tab grouping that survives
/// re-projection). Split structure is geometry, so no `SplitWith` here.
fn build_arrangement(
    pane: &Pane,
    arr: &mut Arrangement,
    node_of: &mut HashMap<GraphMemberId, ArrangementNodeId>,
) {
    match pane {
        Pane::Stack(s) => {
            let mut prev: Option<ArrangementNodeId> = None;
            for &member in &s.members {
                let id = *node_of
                    .entry(member)
                    .or_insert_with(|| arr.add_member_intent(member));
                if let Some(prev) = prev {
                    arr.stack(prev, id);
                }
                prev = Some(id);
            }
        }
        Pane::Split { children, .. } => {
            for b in children {
                build_arrangement(&b.pane, arr, node_of);
            }
        }
    }
}

/// The arrangement's default flat projection as a `Pane` tree: root members
/// laid side-by-side along a Row, `StackedWith` members collapsed into a
/// tab-stack cell. `None` for an arrangement with no tile-bearing members.
fn flat_pane(arrangement: &Arrangement) -> Option<Pane> {
    let plan = project_tree(arrangement);
    let cells: Vec<Pane> = plan
        .slots
        .iter()
        .filter_map(|slot| {
            let members: Vec<GraphMemberId> = match slot {
                PlanSlot::Tile(t) => t.member.into_iter().collect(),
                PlanSlot::Tabs(v) => v.iter().filter_map(|t| t.member).collect(),
            };
            (!members.is_empty()).then(|| Pane::Stack(Stack { members, active: 0 }))
        })
        .collect();
    match cells.len() {
        0 => None,
        1 => cells.into_iter().next(),
        n => {
            let fraction = 1.0 / n as f32;
            Some(Pane::Split {
                axis: pelt_core::tile::SplitAxis::Row,
                children: cells
                    .into_iter()
                    .map(|pane| Branch { fraction, pane })
                    .collect(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pelt_core::tile::SplitAxis;
    use uuid::Uuid;

    fn m(n: u128) -> GraphMemberId {
        Uuid::from_u128(n)
    }

    /// A workbench built from a raw root, in Tree mode (the bridge's target mode).
    fn tiled(root: Pane) -> Workbench {
        Workbench {
            mode: ProjectionKind::Tree,
            root: Some(root),
        }
    }

    /// `wb -> (arrangement, geometry) -> wb` reproduces the split tree exactly.
    fn assert_round_trips(wb: &Workbench) {
        let (arr, geom) = wb.to_arrangement();
        let back = Workbench::from_arrangement(&arr, geom.as_ref());
        assert_eq!(back.root, wb.root, "split tree survives the round trip");
        assert_eq!(back.mode(), ProjectionKind::Tree);
    }

    #[test]
    fn lone_stack_round_trips() {
        assert_round_trips(&tiled(Pane::Stack(Stack {
            members: vec![m(1), m(2)],
            active: 1,
        })));
    }

    #[test]
    fn canonical_roundtrips_reports_the_invariant() {
        let wb = tiled(Pane::Split {
            axis: SplitAxis::Row,
            children: vec![
                Branch {
                    fraction: 0.4,
                    pane: Pane::leaf(m(1)),
                },
                Branch {
                    fraction: 0.6,
                    pane: Pane::Stack(Stack {
                        members: vec![m(2), m(3)],
                        active: 1,
                    }),
                },
            ],
        });
        assert!(
            wb.canonical_roundtrips(),
            "a tree built from valid mutators is canonical-derivable"
        );
    }

    #[test]
    fn row_split_round_trips() {
        assert_round_trips(&tiled(Pane::Split {
            axis: SplitAxis::Row,
            children: vec![
                Branch {
                    fraction: 0.3,
                    pane: Pane::leaf(m(1)),
                },
                Branch {
                    fraction: 0.7,
                    pane: Pane::leaf(m(2)),
                },
            ],
        }));
    }

    #[test]
    fn nested_split_with_tabs_round_trips() {
        // [ 1 | (2,3 tabbed, active 1) / 4 ] — a Row whose 2nd column is a Column
        // split of a 2-tab stack over a leaf.
        assert_round_trips(&tiled(Pane::Split {
            axis: SplitAxis::Row,
            children: vec![
                Branch {
                    fraction: 0.4,
                    pane: Pane::leaf(m(1)),
                },
                Branch {
                    fraction: 0.6,
                    pane: Pane::Split {
                        axis: SplitAxis::Column,
                        children: vec![
                            Branch {
                                fraction: 0.5,
                                pane: Pane::Stack(Stack {
                                    members: vec![m(2), m(3)],
                                    active: 1,
                                }),
                            },
                            Branch {
                                fraction: 0.5,
                                pane: Pane::leaf(m(4)),
                            },
                        ],
                    },
                },
            ],
        }));
    }

    #[test]
    fn arrangement_records_membership_and_stacking() {
        let wb = tiled(Pane::Split {
            axis: SplitAxis::Row,
            children: vec![
                Branch {
                    fraction: 0.5,
                    pane: Pane::leaf(m(1)),
                },
                Branch {
                    fraction: 0.5,
                    pane: Pane::Stack(Stack {
                        members: vec![m(2), m(3)],
                        active: 0,
                    }),
                },
            ],
        });
        let (arr, _) = wb.to_arrangement();
        // Three members, all present.
        let mut members = arr.referenced_members();
        members.sort();
        assert_eq!(members, vec![m(1), m(2), m(3)]);
        // 2 and 3 are StackedWith; 1 is not stacked with anything.
        let stacked = arr
            .edges()
            .filter(|e| e.kind == forme::ArrangementEdgeKind::StackedWith)
            .count();
        assert_eq!(stacked, 1, "exactly the (2,3) tab pair is stacked");
    }

    #[test]
    fn no_geometry_falls_back_to_flat_projection() {
        // Build an arrangement of three members, two stacked, then rebuild with
        // NO geometry: the flat projection is [1] | (2,3) — two side-by-side cells.
        let mut arr = Arrangement::new();
        let a = arr.add_member_intent(m(1));
        let b = arr.add_member_intent(m(2));
        let c = arr.add_member_intent(m(3));
        let _ = a;
        arr.stack(b, c);
        let wb = Workbench::from_arrangement(&arr, None);
        assert_eq!(wb.slot_count(), 2, "the stacked pair collapses to one cell");
        assert_eq!(wb.tile_count(), 3);
        assert!(wb.is_tiled());
    }

    #[test]
    fn geometry_reconciles_a_dropped_member() {
        // Geometry references {1,2,3}; the arrangement only carries {1,3} (member 2
        // was closed). Rebuild drops 2 and collapses its cell.
        let geom = TreeGeometry::Split {
            axis: crate::projection_geometry::Axis::Row,
            children: vec![
                TreeBranch {
                    fraction: 0.5,
                    node: TreeGeometry::leaf(m(1)),
                },
                TreeBranch {
                    fraction: 0.5,
                    node: TreeGeometry::Stack {
                        members: vec![m(2), m(3)],
                        active: 0,
                    },
                },
            ],
        };
        let mut arr = Arrangement::new();
        arr.add_member_intent(m(1));
        arr.add_member_intent(m(3));
        let wb = Workbench::from_arrangement(&arr, Some(&geom));
        let mut open = wb.open_members();
        open.sort();
        assert_eq!(
            open,
            vec![m(1), m(3)],
            "the dropped member is reconciled away"
        );
    }

    /// A small deterministic PRNG (LCG) so the fuzz test is reproducible in CI.
    fn lcg(state: &mut u64) -> u32 {
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (*state >> 33) as u32
    }

    /// Property: a workbench built by ANY sequence of public mutators round-trips
    /// through `(Arrangement, geometry)` with its split tree bit-identical. Many
    /// seeds exercise the degenerate shapes the hand-written cases miss - single-
    /// child collapses, asymmetric nests, transient one-member stacks, nudged
    /// dividers. This is the safety net the panel asked for before persistence
    /// touches disk: if any shape round-trips imperfectly, a restart would reload a
    /// subtly wrong layout.
    #[test]
    fn random_gesture_sequences_round_trip() {
        let pool: Vec<GraphMemberId> = (1..=6u128).map(m).collect();
        for seed in 0..600u64 {
            let mut st = seed
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(0x0123_4567_89AB_CDEF);
            let mut wb = Workbench::new();
            wb.ensure_tiled();
            let steps = 4 + lcg(&mut st) % 16;
            for _ in 0..steps {
                let a = pool[lcg(&mut st) as usize % pool.len()];
                let b = pool[lcg(&mut st) as usize % pool.len()];
                let axis = if lcg(&mut st) % 2 == 0 {
                    SplitAxis::Row
                } else {
                    SplitAxis::Column
                };
                let after = lcg(&mut st) % 2 == 0;
                match lcg(&mut st) % 8 {
                    0 => {
                        wb.open_tile(a);
                    }
                    1 => {
                        wb.open_stack(&[a, b]);
                    }
                    2 => {
                        wb.split_beside_axis(a, b, axis, after);
                    }
                    3 => {
                        wb.split_out(a, axis, after);
                    }
                    4 => {
                        wb.move_to_slot_of(a, b);
                    }
                    5 => {
                        wb.open_in_slot_of(a, b);
                    }
                    6 => {
                        wb.close_tile(a);
                    }
                    _ => {
                        wb.activate(a);
                    }
                }
                // Occasionally nudge a top-level divider, exercising the fraction path.
                if lcg(&mut st) % 3 == 0 {
                    let f0 = 0.1 + (lcg(&mut st) % 80) as f32 / 100.0;
                    let f1 = 0.1 + (lcg(&mut st) % 80) as f32 / 100.0;
                    wb.set_weights(&[f0, f1]);
                }
            }
            let (arr, geom) = wb.to_arrangement();
            let back = Workbench::from_arrangement(&arr, geom.as_ref());
            assert_eq!(
                back.root, wb.root,
                "seed {seed}: the split tree must survive (Arrangement, geometry) round trip",
            );
        }
    }
}
