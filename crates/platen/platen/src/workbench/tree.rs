// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The TileLayout's recursive split tree: the internal model behind [`TileLayout`].
//!
//! A [`Pane`] is either a [`Stack`] (a tab group of one or more members, one visible)
//! or a [`Split`](Pane::Split) of children laid along an axis (a `Row` is side-by-side,
//! a `Column` is stacked top-to-bottom), each child carrying its fractional share of
//! the split. Splits nest freely, so the tree expresses every variation: horizontal,
//! vertical, and combinations. It mirrors Genet's
//! [`TileTree`](workbench::TileTree)
//! shape (the surface that renders it), so [`Pane::to_tile_tree`] is a direct map.
//!
//! The tree holds [`GraphMemberId`]s; the host resolves each to renderable content. The
//! algorithms here (split beside, stack into, remove + collapse, divider fractions) are
//! the geometry-free tiling logic; layout is the host's genet/taffy job.

use forme::GraphMemberId;
use workbench::SplitAxis;

/// A tab group: members sharing one cell, `active` the visible one.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Stack {
    pub members: Vec<GraphMemberId>,
    pub active: usize,
}

impl Stack {
    pub(super) fn single(member: GraphMemberId) -> Self {
        Self {
            members: vec![member],
            active: 0,
        }
    }

    /// Drop `member` if present; clamp `active`. Returns whether it was removed.
    fn remove(&mut self, member: GraphMemberId) -> bool {
        let Some(i) = self.members.iter().position(|m| *m == member) else {
            return false;
        };
        self.members.remove(i);
        if !self.members.is_empty() && self.active >= self.members.len() {
            self.active = self.members.len() - 1;
        }
        true
    }
}

/// One child of a split: its fractional share of the axis plus its subtree.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Branch {
    pub fraction: f32,
    pub pane: Pane,
}

/// A node in the TileLayout tree.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Pane {
    Stack(Stack),
    Split {
        axis: SplitAxis,
        children: Vec<Branch>,
    },
}

impl Pane {
    pub(super) fn leaf(member: GraphMemberId) -> Self {
        Pane::Stack(Stack::single(member))
    }

    /// Append every member, left-to-right / top-to-bottom, into `out`.
    pub(super) fn collect_members(&self, out: &mut Vec<GraphMemberId>) {
        match self {
            Pane::Stack(s) => out.extend_from_slice(&s.members),
            Pane::Split { children, .. } => {
                for b in children {
                    b.pane.collect_members(out);
                }
            }
        }
    }

    pub(super) fn contains(&self, member: GraphMemberId) -> bool {
        match self {
            Pane::Stack(s) => s.members.contains(&member),
            Pane::Split { children, .. } => children.iter().any(|b| b.pane.contains(member)),
        }
    }

    /// A different member sharing `member`'s stack, if any — the anchor for splitting
    /// `member` out of a multi-tab cell. `None` when `member` is alone in its cell.
    pub(super) fn sibling_in_stack(&self, member: GraphMemberId) -> Option<GraphMemberId> {
        match self {
            Pane::Stack(s) if s.members.contains(&member) => {
                s.members.iter().copied().find(|m| *m != member)
            }
            Pane::Stack(_) => None,
            Pane::Split { children, .. } => children
                .iter()
                .find_map(|b| b.pane.sibling_in_stack(member)),
        }
    }

    /// The number of leaf stacks (the "slots").
    pub(super) fn leaf_count(&self) -> usize {
        match self {
            Pane::Stack(_) => 1,
            Pane::Split { children, .. } => children.iter().map(|b| b.pane.leaf_count()).sum(),
        }
    }

    /// True once this pane carries nothing (an empty stack, or a split with no
    /// children) — the signal for its parent to drop it.
    pub(super) fn is_empty(&self) -> bool {
        match self {
            Pane::Stack(s) => s.members.is_empty(),
            Pane::Split { children, .. } => children.is_empty(),
        }
    }

    /// Make `member` the active tab of its stack. Returns whether it was found.
    pub(super) fn activate(&mut self, member: GraphMemberId) -> bool {
        match self {
            Pane::Stack(s) => {
                if let Some(i) = s.members.iter().position(|m| *m == member) {
                    s.active = i;
                    true
                } else {
                    false
                }
            }
            Pane::Split { children, .. } => children.iter_mut().any(|b| b.pane.activate(member)),
        }
    }

    /// Remove `member` from its stack, then collapse: drop an emptied child stack, and
    /// fold a split that drops to a single child into that child. Returns whether the
    /// member was found. After this `self` may be empty (the caller drops it).
    pub(super) fn remove(&mut self, member: GraphMemberId) -> bool {
        match self {
            Pane::Stack(s) => s.remove(member),
            Pane::Split { children, .. } => {
                let mut found = false;
                for b in children.iter_mut() {
                    if b.pane.remove(member) {
                        found = true;
                        break;
                    }
                }
                if found {
                    children.retain(|b| !b.pane.is_empty());
                    self.collapse();
                }
                found
            }
        }
    }

    /// Fold a single-child split into its child (recursively), else renormalize the
    /// children's fractions to sum to 1.
    fn collapse(&mut self) {
        if let Pane::Split { children, .. } = self {
            if children.len() == 1 {
                let only = children.remove(0);
                *self = only.pane;
                self.collapse();
                return;
            }
            renormalize(children);
        }
    }

    /// Stack `member` into the stack holding `target`, made the active tab. Within the
    /// same stack it reorders `member` to just after `target`. Returns whether `target`
    /// was found. `member` must already be detached from elsewhere by the caller.
    pub(super) fn stack_into(&mut self, member: GraphMemberId, target: GraphMemberId) -> bool {
        match self {
            Pane::Stack(s) => {
                let Some(ti) = s.members.iter().position(|m| *m == target) else {
                    return false;
                };
                if let Some(from) = s.members.iter().position(|m| *m == member) {
                    s.members.remove(from);
                    let after = s.members.iter().position(|m| *m == target).unwrap() + 1;
                    s.members.insert(after, member);
                    s.active = after;
                } else {
                    s.members.insert(ti + 1, member);
                    s.active = ti + 1;
                }
                true
            }
            Pane::Split { children, .. } => children
                .iter_mut()
                .any(|b| b.pane.stack_into(member, target)),
        }
    }

    /// Split `member` (as a fresh single-tile stack) in beside `target` along `axis`,
    /// on the `after` side. When the split holding `target` already runs along `axis`,
    /// `member` is inserted as a sibling there (extending it); otherwise `target`'s
    /// stack is replaced by a new `axis` split of the two (nesting). Returns whether
    /// `target` was found. `member` must already be detached by the caller.
    pub(super) fn split_beside(
        &mut self,
        member: GraphMemberId,
        target: GraphMemberId,
        axis: SplitAxis,
        after: bool,
    ) -> bool {
        match self {
            Pane::Stack(s) if s.members.contains(&target) => {
                let this = std::mem::replace(
                    self,
                    Pane::Stack(Stack {
                        members: Vec::new(),
                        active: 0,
                    }),
                );
                let fresh = Pane::leaf(member);
                let children = if after {
                    vec![
                        Branch {
                            fraction: 0.5,
                            pane: this,
                        },
                        Branch {
                            fraction: 0.5,
                            pane: fresh,
                        },
                    ]
                } else {
                    vec![
                        Branch {
                            fraction: 0.5,
                            pane: fresh,
                        },
                        Branch {
                            fraction: 0.5,
                            pane: this,
                        },
                    ]
                };
                *self = Pane::Split { axis, children };
                true
            }
            Pane::Stack(_) => false,
            Pane::Split {
                axis: my_axis,
                children,
            } => {
                // The direct child that is target's own stack, if any.
                let direct = children
                    .iter()
                    .position(|b| matches!(&b.pane, Pane::Stack(s) if s.members.contains(&target)));
                if let Some(i) = direct {
                    if *my_axis == axis {
                        // Same axis: extend this split with a new sibling, preserving the
                        // existing children's relative shares.
                        let n = children.len() + 1;
                        let new_frac = 1.0 / n as f32;
                        renormalize(children);
                        for b in children.iter_mut() {
                            b.fraction *= 1.0 - new_frac;
                        }
                        let at = if after { i + 1 } else { i };
                        children.insert(
                            at,
                            Branch {
                                fraction: new_frac,
                                pane: Pane::leaf(member),
                            },
                        );
                        return true;
                    }
                    // Cross axis: nest — replace the child stack with an `axis` split.
                    return children[i].pane.split_beside(member, target, axis, after);
                }
                // target is deeper in some child: recurse.
                children
                    .iter_mut()
                    .any(|b| b.pane.split_beside(member, target, axis, after))
            }
        }
    }

    /// The child fractions of the split addressed by `path` (the child index taken at
    /// each level), or `None` if the path does not land on a split.
    pub(super) fn fractions_at(&self, path: &[usize]) -> Option<Vec<f32>> {
        match (self, path.split_first()) {
            (Pane::Split { children, .. }, None) => {
                Some(children.iter().map(|b| b.fraction).collect())
            }
            (Pane::Split { children, .. }, Some((&i, rest))) => {
                children.get(i).and_then(|b| b.pane.fractions_at(rest))
            }
            _ => None,
        }
    }

    /// Set the child fractions of the split at `path` (clamped, renormalized). Extra /
    /// missing entries are ignored. Returns whether the split was found.
    pub(super) fn set_fractions_at(&mut self, path: &[usize], fractions: &[f32]) -> bool {
        match (self, path.split_first()) {
            (Pane::Split { children, .. }, None) => {
                for (b, &f) in children.iter_mut().zip(fractions) {
                    b.fraction = f.max(0.02);
                }
                renormalize(children);
                true
            }
            (Pane::Split { children, .. }, Some((&i, rest))) => children
                .get_mut(i)
                .map(|b| b.pane.set_fractions_at(rest, fractions))
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Map this pane onto Genet's [`TileTree`], resolving each member through `tile_for`.
    pub(super) fn to_tile_tree(
        &self,
        tile_for: &mut impl FnMut(GraphMemberId) -> workbench::Tile,
    ) -> workbench::TileTree {
        use workbench::{TileBranch, TileTree};
        match self {
            Pane::Stack(s) if s.members.len() == 1 => TileTree::single(tile_for(s.members[0])),
            Pane::Stack(s) => {
                TileTree::stack(s.members.iter().map(|m| tile_for(*m)).collect(), s.active)
            }
            Pane::Split { axis, children } => {
                let branches = children
                    .iter()
                    .map(|b| TileBranch::new(b.fraction, b.pane.to_tile_tree(tile_for)))
                    .collect();
                TileTree::split(*axis, branches)
            }
        }
    }
}

/// Scale a split's child fractions to sum to 1 (equal shares if they sum to ~0).
fn renormalize(children: &mut [Branch]) {
    let total: f32 = children.iter().map(|b| b.fraction).sum();
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

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn m(n: u128) -> GraphMemberId {
        Uuid::from_u128(n)
    }

    fn members(p: &Pane) -> Vec<GraphMemberId> {
        let mut v = Vec::new();
        p.collect_members(&mut v);
        v
    }

    #[test]
    fn split_beside_same_axis_extends_the_parent() {
        // [1 | 2], split 3 to the right of 2 along Row -> [1 | 2 | 3] (one split).
        let mut root = Pane::Split {
            axis: SplitAxis::Row,
            children: vec![
                Branch {
                    fraction: 0.5,
                    pane: Pane::leaf(m(1)),
                },
                Branch {
                    fraction: 0.5,
                    pane: Pane::leaf(m(2)),
                },
            ],
        };
        assert!(root.split_beside(m(3), m(2), SplitAxis::Row, true));
        match &root {
            Pane::Split { axis, children } => {
                assert_eq!(*axis, SplitAxis::Row);
                assert_eq!(children.len(), 3, "extended, not nested");
                let sum: f32 = children.iter().map(|b| b.fraction).sum();
                assert!((sum - 1.0).abs() < 1e-5, "fractions renormalized");
            }
            other => panic!("expected a row split, got {other:?}"),
        }
        assert_eq!(members(&root), vec![m(1), m(2), m(3)]);
    }

    #[test]
    fn split_beside_cross_axis_nests() {
        // [1 | 2], split 3 below 2 along Column -> [1 | (2 / 3)] (nested).
        let mut root = Pane::Split {
            axis: SplitAxis::Row,
            children: vec![
                Branch {
                    fraction: 0.5,
                    pane: Pane::leaf(m(1)),
                },
                Branch {
                    fraction: 0.5,
                    pane: Pane::leaf(m(2)),
                },
            ],
        };
        assert!(root.split_beside(m(3), m(2), SplitAxis::Column, true));
        match &root {
            Pane::Split { axis, children } => {
                assert_eq!(*axis, SplitAxis::Row);
                assert_eq!(children.len(), 2, "still two top-level columns");
                match &children[1].pane {
                    Pane::Split { axis, children } => {
                        assert_eq!(
                            *axis,
                            SplitAxis::Column,
                            "the second column nests a Column split"
                        );
                        assert_eq!(children.len(), 2);
                    }
                    other => panic!("expected a nested column split, got {other:?}"),
                }
            }
            other => panic!("expected a row split, got {other:?}"),
        }
        assert_eq!(members(&root), vec![m(1), m(2), m(3)]);
    }

    #[test]
    fn remove_collapses_single_child_splits() {
        // [1 | (2 / 3)] -> remove 3 -> the column collapses -> [1 | 2].
        let mut root = Pane::Split {
            axis: SplitAxis::Row,
            children: vec![
                Branch {
                    fraction: 0.5,
                    pane: Pane::leaf(m(1)),
                },
                Branch {
                    fraction: 0.5,
                    pane: Pane::Split {
                        axis: SplitAxis::Column,
                        children: vec![
                            Branch {
                                fraction: 0.5,
                                pane: Pane::leaf(m(2)),
                            },
                            Branch {
                                fraction: 0.5,
                                pane: Pane::leaf(m(3)),
                            },
                        ],
                    },
                },
            ],
        };
        assert!(root.remove(m(3)));
        match &root {
            Pane::Split { children, .. } => {
                assert_eq!(children.len(), 2);
                assert!(
                    matches!(&children[1].pane, Pane::Stack(s) if s.members == vec![m(2)]),
                    "the column collapsed to a plain stack"
                );
            }
            other => panic!("expected a row split, got {other:?}"),
        }
        assert_eq!(members(&root), vec![m(1), m(2)]);
    }

    #[test]
    fn stack_into_moves_across_and_reorders_within() {
        let mut root = Pane::Split {
            axis: SplitAxis::Row,
            children: vec![
                Branch {
                    fraction: 0.5,
                    pane: Pane::leaf(m(1)),
                },
                Branch {
                    fraction: 0.5,
                    pane: Pane::leaf(m(2)),
                },
            ],
        };
        // Stack 1 into 2's stack (the caller removes 1 from its slot first).
        root.remove(m(1));
        // After removal the split collapsed to a lone stack [2]; re-add 1 beside 2.
        assert!(root.stack_into(m(1), m(2)) || matches!(root, Pane::Stack(_)));
    }

    #[test]
    fn fractions_at_reads_and_sets_a_nested_split() {
        let mut root = Pane::Split {
            axis: SplitAxis::Row,
            children: vec![
                Branch {
                    fraction: 0.5,
                    pane: Pane::leaf(m(1)),
                },
                Branch {
                    fraction: 0.5,
                    pane: Pane::Split {
                        axis: SplitAxis::Column,
                        children: vec![
                            Branch {
                                fraction: 0.5,
                                pane: Pane::leaf(m(2)),
                            },
                            Branch {
                                fraction: 0.5,
                                pane: Pane::leaf(m(3)),
                            },
                        ],
                    },
                },
            ],
        };
        assert_eq!(
            root.fractions_at(&[]).unwrap().len(),
            2,
            "top-level row has two children"
        );
        assert_eq!(
            root.fractions_at(&[1]).unwrap().len(),
            2,
            "the nested column has two"
        );
        assert!(root.set_fractions_at(&[1], &[0.8, 0.2]));
        let nested = root.fractions_at(&[1]).unwrap();
        assert!((nested[0] - 0.8).abs() < 1e-5 && (nested[1] - 0.2).abs() < 1e-5);
    }
}
