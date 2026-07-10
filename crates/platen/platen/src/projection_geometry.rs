/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Projection geometry — the geometry sidecar for a forme's **Tree** projection.
//! (Cartography geometry moved to the canvas crate in the 2026-07-09 platen
//! decomposition; this file keeps the pane-lane Tree geometry.)
//!
//! Per the composition spine (§9, §15): `forme` owns the geometry-free *semantic*
//! arrangement (which members are present, which are tabbed together); this owns
//! the *semantic geometry* of one projection of it, keyed `(FormeRef,
//! ProjectionKind)` in the projection-geometry store. For the Tree projection
//! that geometry is the split **skeleton** (axis, nesting, sibling order), each
//! split's **fractions**, and each leaf stack's **active** tab.
//!
//! "Semantic geometry, not pixels": fractions are *ratios*, never rectangles, so
//! one saved geometry renders responsively at any pane size, and is shared across
//! panes projecting the same forme the same way (two independent layouts are
//! *forked formes*, not one bench in two views). Leaves are **member-keyed** so a
//! saved geometry survives re-projection and reconciles against the arrangement.
//!
//! A [`TreeGeometry`] is a structural mirror of the workbench's live split tree
//! (platen's `Workbench`): the bridge ([`Workbench::to_arrangement`] /
//! [`Workbench::from_arrangement`]) derives an `(Arrangement, TreeGeometry)` pair
//! from a tree and rebuilds the tree from the pair, losslessly. The arrangement
//! is the semantic + re-projection truth; this is the layout refinement over the
//! arrangement's default flat projection ([`crate::project_tree`]).

use forme::GraphMemberId;
use pelt_core::tile::SplitAxis;
use serde::{Deserialize, Serialize};

/// Split orientation. Mirrors [`pelt_core::tile::SplitAxis`] with a serde impl —
/// pelt's axis is a render contract and is deliberately serde-free, while
/// projection geometry persists (the `(FormeRef, ProjectionKind)` store).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    /// Children laid side-by-side (a horizontal split).
    Row,
    /// Children stacked top-to-bottom (a vertical split).
    Column,
}

impl From<SplitAxis> for Axis {
    fn from(a: SplitAxis) -> Self {
        match a {
            SplitAxis::Row => Axis::Row,
            SplitAxis::Column => Axis::Column,
        }
    }
}

impl From<Axis> for SplitAxis {
    fn from(a: Axis) -> Self {
        match a {
            Axis::Row => SplitAxis::Row,
            Axis::Column => SplitAxis::Column,
        }
    }
}

/// The Tree-projection geometry of a forme: the split skeleton, each split's
/// fractions, and each leaf stack's active tab. Member-keyed at the leaves. See
/// the module docs for the persistence/ownership contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TreeGeometry {
    /// A leaf cell — a tab-stack of members, `active` the visible one. `active`
    /// is clamped into range by [`Self::sanitize`].
    Stack {
        members: Vec<GraphMemberId>,
        active: usize,
    },
    /// A split of `children` along `axis`, each carrying its fractional share of
    /// the axis. Fractions are ratios (sum normalized to 1 by [`Self::sanitize`]).
    Split {
        axis: Axis,
        children: Vec<TreeBranch>,
    },
}

/// One child of a [`TreeGeometry::Split`]: its fractional share plus its subtree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreeBranch {
    pub fraction: f32,
    pub node: TreeGeometry,
}

impl TreeGeometry {
    /// A lone leaf stack of one member.
    pub fn leaf(member: GraphMemberId) -> Self {
        TreeGeometry::Stack {
            members: vec![member],
            active: 0,
        }
    }

    /// Every member referenced at the leaves, left-to-right / top-to-bottom.
    pub fn members(&self) -> Vec<GraphMemberId> {
        let mut out = Vec::new();
        self.collect_members(&mut out);
        out
    }

    fn collect_members(&self, out: &mut Vec<GraphMemberId>) {
        match self {
            TreeGeometry::Stack { members, .. } => out.extend_from_slice(members),
            TreeGeometry::Split { children, .. } => {
                for b in children {
                    b.node.collect_members(out);
                }
            }
        }
    }

    /// The number of leaf stacks (cells).
    pub fn leaf_count(&self) -> usize {
        match self {
            TreeGeometry::Stack { .. } => 1,
            TreeGeometry::Split { children, .. } => {
                children.iter().map(|b| b.node.leaf_count()).sum()
            }
        }
    }

    /// Clamp every stack's `active` into range and renormalize every split's
    /// fractions to sum to 1 (equal shares if they sum to ~0). Applied after
    /// loading a persisted geometry, where members may have been reconciled away.
    pub fn sanitize(&mut self) {
        match self {
            TreeGeometry::Stack { members, active } => {
                if members.is_empty() {
                    *active = 0;
                } else if *active >= members.len() {
                    *active = members.len() - 1;
                }
            }
            TreeGeometry::Split { children, .. } => {
                // Renormalize only when the stored fractions are actually invalid (a
                // non-positive share, or a sum drifted past a hair from 1). Valid
                // fractions are left bit-exact, so a persist/reload round-trips the
                // layout identically rather than nudging every divider each load.
                let total: f32 = children.iter().map(|b| b.fraction).sum();
                let invalid =
                    children.iter().any(|b| !(b.fraction > 0.0)) || (total - 1.0).abs() > 1e-4;
                if invalid {
                    if total > f32::EPSILON {
                        for b in children.iter_mut() {
                            b.fraction /= total;
                        }
                    } else if !children.is_empty() {
                        let f = 1.0 / children.len() as f32;
                        for b in children.iter_mut() {
                            b.fraction = f;
                        }
                    }
                }
                for b in children.iter_mut() {
                    b.node.sanitize();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn m(n: u128) -> GraphMemberId {
        Uuid::from_u128(n)
    }

    #[test]
    fn axis_round_trips_through_split_axis() {
        for a in [Axis::Row, Axis::Column] {
            assert_eq!(a, Axis::from(SplitAxis::from(a)));
        }
    }

    #[test]
    fn members_flattens_leaves_in_order() {
        let g = TreeGeometry::Split {
            axis: Axis::Row,
            children: vec![
                TreeBranch {
                    fraction: 0.5,
                    node: TreeGeometry::leaf(m(1)),
                },
                TreeBranch {
                    fraction: 0.5,
                    node: TreeGeometry::Stack {
                        members: vec![m(2), m(3)],
                        active: 1,
                    },
                },
            ],
        };
        assert_eq!(g.members(), vec![m(1), m(2), m(3)]);
        assert_eq!(g.leaf_count(), 2);
    }

    #[test]
    fn sanitize_clamps_active_and_renormalizes_fractions() {
        let mut g = TreeGeometry::Split {
            axis: Axis::Row,
            children: vec![
                TreeBranch {
                    fraction: 3.0,
                    node: TreeGeometry::leaf(m(1)),
                },
                TreeBranch {
                    fraction: 1.0,
                    node: TreeGeometry::Stack {
                        members: vec![m(2)],
                        active: 9,
                    },
                },
            ],
        };
        g.sanitize();
        match &g {
            TreeGeometry::Split { children, .. } => {
                let sum: f32 = children.iter().map(|b| b.fraction).sum();
                assert!((sum - 1.0).abs() < 1e-5, "fractions renormalized to 1");
                assert!(
                    (children[0].fraction - 0.75).abs() < 1e-5,
                    "shares preserved"
                );
                match &children[1].node {
                    TreeGeometry::Stack { active, .. } => assert_eq!(*active, 0, "active clamped"),
                    other => panic!("expected a stack, got {other:?}"),
                }
            }
            other => panic!("expected a split, got {other:?}"),
        }
    }

    #[test]
    fn round_trips_through_json() {
        let g = TreeGeometry::Split {
            axis: Axis::Column,
            children: vec![
                TreeBranch {
                    fraction: 0.3,
                    node: TreeGeometry::leaf(m(1)),
                },
                TreeBranch {
                    fraction: 0.7,
                    node: TreeGeometry::Stack {
                        members: vec![m(2), m(3)],
                        active: 0,
                    },
                },
            ],
        };
        let json = serde_json::to_string(&g).unwrap();
        let back: TreeGeometry = serde_json::from_str(&json).unwrap();
        assert_eq!(g, back);
    }
}
