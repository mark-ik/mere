/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Projection geometry — the geometry sidecar for a forme's **Tree** projection.
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

use std::collections::HashSet;

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

/// The geometry of the **Cartography** projection (the orrery): each member's world
/// position, member-keyed. The cartography counterpart of [`TreeGeometry`] for the
/// Tree projection — "canonical world positions (cartography)" per the composition
/// spine (§9), keyed `(FormeRef::Identity(GraphId), ProjectionKind::Cartography)` and
/// persisted beside the session graph. Positions are *world* coordinates (the
/// semantic geometry), not pixels, so they render responsively at any zoom.
///
/// This is the authoritative store of the orrery's *settled* layout: the live
/// force-directed positions live in the gyre read model and were never written back
/// to the kernel graph, so without this sidecar a session's settled layout is lost on
/// reload (graph.json carries only the load-time seed).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CartographyGeometry {
    positions: Vec<(GraphMemberId, (f32, f32))>,
    /// Per-member face-size overrides (px) — the sizing counterpart of the positions, for
    /// members the user sized explicitly. A member absent here falls back to size-by-degree
    /// or the default at render time, so only deliberate overrides persist. Serde-defaulted,
    /// so a pre-sizing sidecar still loads. (Node-rep — size persistence.)
    #[serde(default)]
    sizes: Vec<(GraphMemberId, f32)>,
    /// Whether the scene's size-by-degree mode (faces grow with undirected degree) was on at
    /// save time. Serde-defaulted to `false`. (Node-rep — size persistence.)
    #[serde(default)]
    size_by_degree: bool,
    /// Whether the scene's **size-by-importance** mode (faces grow with the graph-signals
    /// importance) was on at save time. Serde-defaulted to `false`. (Graph signals.)
    #[serde(default)]
    size_by_importance: bool,
    /// The importance **metric** code (`degree` / `betweenness`) size-by-importance read at save
    /// time, so a betweenness-sized scene re-opens betweenness-sized. Stored as a string to keep
    /// this crate signals-free; the orrery maps it to/from `ImportanceMetric`. Serde-defaulted to
    /// the empty string, which the orrery's `from_code` reads as `degree` (a pre-metric sidecar
    /// loads as degree). (Graph signals — metric persistence.)
    #[serde(default)]
    importance_metric: String,
    /// Per-member sprite faces: the imported image as a PNG data-URI, for members the user
    /// gave a custom face. A member absent here has no sprite (its content-type default /
    /// favicon applies). Serde-defaulted, so a pre-sprite sidecar still loads. The data-URIs
    /// add bulk to the sidecar — acceptable for a handful of faces (a future optimization
    /// could externalize the blobs). (Node-rep — sprite persistence.)
    #[serde(default)]
    sprites: Vec<(GraphMemberId, String)>,
    /// Per-member sprite collider hulls: the opaque region as a face-normalized convex
    /// polygon ([-0.5, 0.5]), persisted beside the sprite so the traced-to-image collider
    /// survives a reload without re-decoding the image. Serde-defaulted. (Node-rep — sprite hull.)
    #[serde(default)]
    sprite_hulls: Vec<(GraphMemberId, Vec<(f32, f32)>)>,
    /// Per-member physical material overrides `(restitution, friction, density)` on the Body
    /// axis, so a node tuned heavier / bouncier / grippier re-opens that way. Stored as a plain
    /// tuple to keep this crate gyre-free; the orrery maps it to/from `gyre::NodeMaterial`.
    /// Serde-defaulted. (Node body & face — material.)
    #[serde(default)]
    materials: Vec<(GraphMemberId, (f32, f32, f32))>,
    /// Per-member **face** overrides on the Face axis, as a string code (`favicon` / `sprite` /
    /// `bare`), so a node's chosen texture re-opens that way. Stored as a string to keep this
    /// crate orrery-free; the orrery maps it to/from `orrery::Face`. Serde-defaulted. (Node body
    /// & face — face persistence.)
    #[serde(default)]
    faces: Vec<(GraphMemberId, String)>,
}

impl CartographyGeometry {
    /// Collect a member-keyed position set (e.g. the orrery's live node positions).
    pub fn from_positions(
        positions: impl IntoIterator<Item = (GraphMemberId, (f32, f32))>,
    ) -> Self {
        Self {
            positions: positions.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Attach per-member size overrides (chainable after [`from_positions`]).
    /// (Node-rep — size persistence.)
    pub fn with_sizes(mut self, sizes: impl IntoIterator<Item = (GraphMemberId, f32)>) -> Self {
        self.sizes = sizes.into_iter().collect();
        self
    }

    /// Set the scene's size-by-importance flag (chainable). (Graph signals.)
    pub fn with_size_by_importance(mut self, on: bool) -> Self {
        self.size_by_importance = on;
        self
    }

    /// Set the scene's importance-metric code (`degree` / `betweenness`) (chainable).
    /// (Graph signals — metric persistence.)
    pub fn with_importance_metric(mut self, code: impl Into<String>) -> Self {
        self.importance_metric = code.into();
        self
    }

    /// Set the scene's size-by-degree flag (chainable). (Node-rep — size persistence.)
    pub fn with_size_by_degree(mut self, on: bool) -> Self {
        self.size_by_degree = on;
        self
    }

    /// Attach per-member sprite faces (chainable). (Node-rep — sprite persistence.)
    pub fn with_sprites(
        mut self,
        sprites: impl IntoIterator<Item = (GraphMemberId, String)>,
    ) -> Self {
        self.sprites = sprites.into_iter().collect();
        self
    }

    /// Attach per-member sprite collider hulls (chainable). (Node-rep — sprite hull.)
    pub fn with_sprite_hulls(
        mut self,
        hulls: impl IntoIterator<Item = (GraphMemberId, Vec<(f32, f32)>)>,
    ) -> Self {
        self.sprite_hulls = hulls.into_iter().collect();
        self
    }

    /// Attach per-member physical material overrides `(restitution, friction, density)`
    /// (chainable). (Node body & face — material.)
    pub fn with_materials(
        mut self,
        materials: impl IntoIterator<Item = (GraphMemberId, (f32, f32, f32))>,
    ) -> Self {
        self.materials = materials.into_iter().collect();
        self
    }

    /// Attach per-member face overrides (string codes `favicon` / `sprite` / `bare`)
    /// (chainable). (Node body & face — face persistence.)
    pub fn with_faces(mut self, faces: impl IntoIterator<Item = (GraphMemberId, String)>) -> Self {
        self.faces = faces.into_iter().collect();
        self
    }

    /// The `(member, (x, y))` pairs, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (GraphMemberId, (f32, f32))> + '_ {
        self.positions.iter().copied()
    }

    /// The `(member, size)` overrides, in insertion order. (Node-rep — size persistence.)
    pub fn size_iter(&self) -> impl Iterator<Item = (GraphMemberId, f32)> + '_ {
        self.sizes.iter().copied()
    }

    /// Whether size-by-degree was on at save time. (Node-rep — size persistence.)
    pub fn size_by_degree(&self) -> bool {
        self.size_by_degree
    }

    /// Whether size-by-importance was on at save time. (Graph signals.)
    pub fn size_by_importance(&self) -> bool {
        self.size_by_importance
    }

    /// The importance-metric code (`degree` / `betweenness`, or empty for a pre-metric sidecar)
    /// at save time. (Graph signals — metric persistence.)
    pub fn importance_metric(&self) -> &str {
        &self.importance_metric
    }

    /// The `(member, data-URI)` sprite faces, in insertion order. (Node-rep — sprite persistence.)
    pub fn sprite_iter(&self) -> impl Iterator<Item = (GraphMemberId, &str)> + '_ {
        self.sprites.iter().map(|(m, uri)| (*m, uri.as_str()))
    }

    /// The `(member, hull)` sprite collider hulls, in insertion order (owned clones, for the
    /// host to apply via `apply_cartography_sprite_hulls`). (Node-rep — sprite hull.)
    pub fn sprite_hull_iter(&self) -> impl Iterator<Item = (GraphMemberId, Vec<(f32, f32)>)> + '_ {
        self.sprite_hulls.iter().map(|(m, h)| (*m, h.clone()))
    }

    /// The `(member, (restitution, friction, density))` material overrides, in insertion order
    /// (for the host to apply via `apply_cartography_materials`). (Node body & face — material.)
    pub fn material_iter(&self) -> impl Iterator<Item = (GraphMemberId, (f32, f32, f32))> + '_ {
        self.materials.iter().map(|(m, mat)| (*m, *mat))
    }

    /// The `(member, code)` face overrides, in insertion order (for the host to apply via
    /// `apply_cartography_faces`). (Node body & face — face persistence.)
    pub fn face_iter(&self) -> impl Iterator<Item = (GraphMemberId, &str)> + '_ {
        self.faces.iter().map(|(m, code)| (*m, code.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// The members carrying a position.
    pub fn members(&self) -> Vec<GraphMemberId> {
        self.positions.iter().map(|(m, _)| *m).collect()
    }

    /// Serialize to JSON for session persistence (the host writes it beside the graph).
    pub fn to_persisted_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Rebuild from persisted JSON, dropping any member the live graph no longer has
    /// (`present`) so a deleted node's stale position (or size) is reconciled away. `None`
    /// on a parse error — the host falls back to the graph's own seed.
    pub fn from_persisted_json(json: &str, present: &HashSet<GraphMemberId>) -> Option<Self> {
        let mut geom: Self = serde_json::from_str(json).ok()?;
        geom.positions
            .retain(|(member, _)| present.contains(member));
        geom.sizes.retain(|(member, _)| present.contains(member));
        geom.sprites.retain(|(member, _)| present.contains(member));
        geom.sprite_hulls
            .retain(|(member, _)| present.contains(member));
        geom.materials
            .retain(|(member, _)| present.contains(member));
        geom.faces.retain(|(member, _)| present.contains(member));
        Some(geom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use uuid::Uuid;

    fn m(n: u128) -> GraphMemberId {
        Uuid::from_u128(n)
    }

    #[test]
    fn cartography_geometry_round_trips_and_prunes_absent_members() {
        let geom = CartographyGeometry::from_positions([
            (m(1), (10.0, 20.0)),
            (m(2), (30.0, 40.0)),
            (m(3), (50.0, 60.0)),
        ]);
        let json = geom.to_persisted_json().unwrap();
        // Member 2 was deleted from the graph since the save.
        let present: HashSet<GraphMemberId> = [m(1), m(3)].into_iter().collect();
        let back = CartographyGeometry::from_persisted_json(&json, &present).unwrap();
        assert_eq!(
            back.members(),
            vec![m(1), m(3)],
            "the absent member is pruned"
        );
        let by_member: std::collections::HashMap<_, _> = back.iter().collect();
        assert_eq!(by_member[&m(1)], (10.0, 20.0));
        assert_eq!(by_member[&m(3)], (50.0, 60.0));
    }

    #[test]
    fn cartography_sizes_and_flag_round_trip_and_prune() {
        let geom =
            CartographyGeometry::from_positions([(m(1), (10.0, 20.0)), (m(2), (30.0, 40.0))])
                .with_sizes([(m(1), 72.0), (m(2), 48.0)])
                .with_size_by_degree(true);
        let json = geom.to_persisted_json().unwrap();
        // Member 2 deleted since the save — its size prunes alongside its position.
        let present: HashSet<GraphMemberId> = [m(1)].into_iter().collect();
        let back = CartographyGeometry::from_persisted_json(&json, &present).unwrap();
        assert!(back.size_by_degree(), "the scene flag survives");
        let sizes: std::collections::HashMap<_, _> = back.size_iter().collect();
        assert_eq!(
            sizes.get(&m(1)),
            Some(&72.0),
            "the kept member's size survives"
        );
        assert_eq!(sizes.get(&m(2)), None, "the absent member's size is pruned");
    }

    #[test]
    fn cartography_sprites_round_trip_and_prune() {
        let geom =
            CartographyGeometry::from_positions([(m(1), (10.0, 20.0)), (m(2), (30.0, 40.0))])
                .with_sprites([
                    (m(1), "data:image/png;base64,AAAA".to_string()),
                    (m(2), "data:image/png;base64,BBBB".to_string()),
                ]);
        let json = geom.to_persisted_json().unwrap();
        // Member 2 deleted since the save — its sprite prunes alongside its position.
        let present: HashSet<GraphMemberId> = [m(1)].into_iter().collect();
        let back = CartographyGeometry::from_persisted_json(&json, &present).unwrap();
        let sprites: std::collections::HashMap<_, _> = back.sprite_iter().collect();
        assert_eq!(
            sprites.get(&m(1)),
            Some(&"data:image/png;base64,AAAA"),
            "the kept member's sprite survives",
        );
        assert_eq!(
            sprites.get(&m(2)),
            None,
            "the absent member's sprite is pruned"
        );
    }

    #[test]
    fn cartography_sprite_hulls_round_trip_and_prune() {
        let geom = CartographyGeometry::from_positions([(m(1), (0.0, 0.0)), (m(2), (1.0, 1.0))])
            .with_sprite_hulls([
                (
                    m(1),
                    vec![(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)],
                ),
                (m(2), vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]),
            ]);
        let json = geom.to_persisted_json().unwrap();
        // Member 2 deleted since the save — its hull prunes alongside its position.
        let present: HashSet<GraphMemberId> = [m(1)].into_iter().collect();
        let back = CartographyGeometry::from_persisted_json(&json, &present).unwrap();
        let hulls: std::collections::HashMap<_, _> = back.sprite_hull_iter().collect();
        assert_eq!(
            hulls.get(&m(1)).map(|h| h.len()),
            Some(4),
            "the kept member's hull survives"
        );
        assert_eq!(hulls.get(&m(2)), None, "the absent member's hull is pruned");
    }

    #[test]
    fn cartography_materials_round_trip_and_prune() {
        let geom = CartographyGeometry::from_positions([(m(1), (0.0, 0.0)), (m(2), (1.0, 1.0))])
            .with_materials([(m(1), (0.6, 0.3, 0.002)), (m(2), (0.0, 0.0, 0.001))]);
        let json = geom.to_persisted_json().unwrap();
        // Member 2 deleted since the save — its material prunes alongside its position.
        let present: HashSet<GraphMemberId> = [m(1)].into_iter().collect();
        let back = CartographyGeometry::from_persisted_json(&json, &present).unwrap();
        let mats: std::collections::HashMap<_, _> = back.material_iter().collect();
        assert_eq!(
            mats.get(&m(1)),
            Some(&(0.6, 0.3, 0.002)),
            "the kept member's material survives"
        );
        assert_eq!(
            mats.get(&m(2)),
            None,
            "the absent member's material is pruned"
        );
    }

    #[test]
    fn cartography_faces_round_trip_and_prune() {
        let geom = CartographyGeometry::from_positions([(m(1), (0.0, 0.0)), (m(2), (1.0, 1.0))])
            .with_faces([(m(1), "bare".to_string()), (m(2), "sprite".to_string())]);
        let json = geom.to_persisted_json().unwrap();
        // Member 2 deleted since the save — its face prunes alongside its position.
        let present: HashSet<GraphMemberId> = [m(1)].into_iter().collect();
        let back = CartographyGeometry::from_persisted_json(&json, &present).unwrap();
        let faces: std::collections::HashMap<_, _> = back.face_iter().collect();
        assert_eq!(
            faces.get(&m(1)),
            Some(&"bare"),
            "the kept member's face survives"
        );
        assert_eq!(faces.get(&m(2)), None, "the absent member's face is pruned");
    }

    #[test]
    fn cartography_loads_a_pre_sizing_sidecar() {
        // A sidecar written before sizes existed (positions only) still deserializes.
        let json = r#"{"positions":[["00000000-0000-0000-0000-000000000001",[1.0,2.0]]]}"#;
        let present: HashSet<GraphMemberId> = [m(1)].into_iter().collect();
        let back = CartographyGeometry::from_persisted_json(json, &present).unwrap();
        assert_eq!(back.members(), vec![m(1)]);
        assert_eq!(back.size_iter().count(), 0, "no sizes default to empty");
        assert!(!back.size_by_degree(), "the flag defaults to off");
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
