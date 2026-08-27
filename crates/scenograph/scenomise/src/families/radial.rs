// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Concentric rings at radii the producer's ring index selects.

use std::collections::BTreeMap;

use sceno::{Radial, RadialAngularPolicy, RadialUnreachablePolicy, ScoreItem, Vec2};

use super::{disclosed_position, numeric_axis, stable_hash};

/// Group items by disclosed ring, then distribute each ring around its circle.
pub(super) fn place(config: &Radial, items: &[&ScoreItem]) -> Vec<Vec2> {
    // BTreeMap so rings are visited in ring order: the outer-ring fallback needs
    // to know the deepest ring, and a reader comparing two runs needs the same
    // traversal both times.
    let mut rings: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut unreachable: Vec<usize> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match ring_of(item) {
            Some(ring) => rings.entry(ring).or_default().push(index),
            None => unreachable.push(index),
        }
    }

    let mut placed = vec![Vec2::ZERO; items.len()];
    for (ring, members) in &rings {
        distribute(config, items, members, *ring, &mut placed);
    }

    if !unreachable.is_empty() {
        match config.unreachable_policy {
            RadialUnreachablePolicy::OuterRing => {
                let outer = rings.keys().next_back().map_or(0, |deepest| deepest + 1);
                distribute(config, items, &unreachable, outer, &mut placed);
            }
            RadialUnreachablePolicy::Center => {
                for index in unreachable {
                    placed[index] = config.center;
                }
            }
            RadialUnreachablePolicy::LeaveInPlace => {
                for index in unreachable {
                    placed[index] = disclosed_position(items[index], config.center);
                }
            }
        }
    }
    placed
}

/// Lay `members` around the circle at `ring`.
fn distribute(
    config: &Radial,
    items: &[&ScoreItem],
    members: &[usize],
    ring: u32,
    placed: &mut [Vec2],
) {
    let radius = ring as f32 * config.ring_spacing;
    // Ring zero is a single point; distributing around it would stack every
    // member at the centre anyway, and the angle would be meaningless.
    if radius == 0.0 {
        for index in members {
            placed[*index] = config.center;
        }
        return;
    }

    let mut order: Vec<usize> = members.to_vec();
    if matches!(config.angular_policy, RadialAngularPolicy::HashSorted) {
        order.sort_by_key(|index| stable_hash(items[*index]));
    }

    let at_angle = |angle: f32| {
        Vec2::new(
            config.center.x + radius * angle.cos(),
            config.center.y + radius * angle.sin(),
        )
    };

    match config.angular_policy {
        // An even split, first slot exactly on `rotation_offset`. Not centred
        // in its slot: that would rotate the whole ring by half a step, and
        // `rotation_offset` is documented as putting the first slot on the +x
        // axis.
        RadialAngularPolicy::Uniform | RadialAngularPolicy::HashSorted => {
            let step = std::f32::consts::TAU / order.len() as f32;
            for (slot, index) in order.iter().enumerate() {
                placed[*index] = at_angle(config.rotation_offset + slot as f32 * step);
            }
        }
        // Arc width in proportion to the disclosed weight, so a hub gets room
        // for its satellites instead of the same slice as a leaf. Here the item
        // *is* centred in its arc — a wide arc means room around the item, and
        // anchoring it to the arc's leading edge would push it against its
        // neighbour.
        RadialAngularPolicy::Weighted => {
            let weights: Vec<f32> = order
                .iter()
                .map(|index| items[*index].weight.unwrap_or(1.0).max(0.0))
                .collect();
            let total: f32 = weights.iter().sum();
            let mut cursor = config.rotation_offset;
            for (slot, index) in order.iter().enumerate() {
                // Every weight zero or absent leaves no proportion to honour, so
                // an even split is the only non-degenerate reading.
                let arc = if total > 0.0 {
                    std::f32::consts::TAU * weights[slot] / total
                } else {
                    std::f32::consts::TAU / order.len() as f32
                };
                placed[*index] = at_angle(cursor + arc * 0.5);
                cursor += arc;
            }
        }
    }
}

/// The disclosed ring, or `None` when the producer could not assign one.
///
/// A negative ring is not a ring. Rounding it toward zero would silently move an
/// item to the centre, so it reads as unassigned and takes the configured
/// unreachable path instead.
fn ring_of(item: &ScoreItem) -> Option<u32> {
    let value = numeric_axis(item)?.round();
    (value >= 0.0).then_some(value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::tests::{item_with_axis, item_with_weight, items};
    use sceno::AxisValue;

    fn ring(id: u32, ring: f64) -> ScoreItem {
        item_with_axis(id, Some(AxisValue::Numeric(ring)))
    }

    fn radius_of(config: &Radial, at: Vec2) -> f32 {
        ((at.x - config.center.x).powi(2) + (at.y - config.center.y).powi(2)).sqrt()
    }

    #[test]
    fn ring_index_selects_the_radius() {
        let config = Radial::default();
        let owned = vec![ring(0, 0.0), ring(1, 1.0), ring(2, 2.0)];
        let placed = place(&config, &items(&owned));
        assert!((radius_of(&config, placed[0])).abs() < 1e-4);
        assert!((radius_of(&config, placed[1]) - config.ring_spacing).abs() < 1e-3);
        assert!((radius_of(&config, placed[2]) - 2.0 * config.ring_spacing).abs() < 1e-3);
    }

    #[test]
    fn a_ring_spreads_its_members_around_the_circle() {
        let config = Radial::default();
        let owned = vec![ring(0, 1.0), ring(1, 1.0), ring(2, 1.0)];
        let placed = place(&config, &items(&owned));
        for (index, a) in placed.iter().enumerate() {
            for b in &placed[index + 1..] {
                assert!((a.x - b.x).abs() > 1e-3 || (a.y - b.y).abs() > 1e-3);
            }
        }
    }

    #[test]
    fn the_first_uniform_slot_sits_exactly_on_the_rotation_offset() {
        // `rotation_offset` is documented as putting the first slot on the +x
        // axis. Centring each item in its slot instead would rotate the whole
        // ring by half a step — invisible on a lone ring, and a mismatch against
        // every stored layout the moment two rings are compared.
        let config = Radial {
            rotation_offset: 0.0,
            ..Radial::default()
        };
        let owned = vec![ring(0, 1.0), ring(1, 1.0), ring(2, 1.0), ring(3, 1.0)];
        let placed = place(&config, &items(&owned));
        assert!(
            (placed[0].x - config.ring_spacing).abs() < 1e-3,
            "{:?}",
            placed[0]
        );
        assert!(placed[0].y.abs() < 1e-3, "{:?}", placed[0]);
    }

    #[test]
    fn weighted_slots_give_a_hub_more_room_than_a_leaf() {
        let config = Radial {
            angular_policy: RadialAngularPolicy::Weighted,
            ..Radial::default()
        };
        // One heavy item and three light ones on the same ring.
        let owned = vec![
            item_with_weight(0, 1.0, 9.0),
            item_with_weight(1, 1.0, 1.0),
            item_with_weight(2, 1.0, 1.0),
            item_with_weight(3, 1.0, 1.0),
        ];
        let placed = place(&config, &items(&owned));
        let angle = |at: Vec2| (at.y - config.center.y).atan2(at.x - config.center.x);
        // The hub's slot spans from before its neighbours to well past them: the
        // gap on either side of it exceeds the gap between two leaves.
        let hub_to_first_leaf = (angle(placed[1]) - angle(placed[0])).abs();
        let leaf_to_leaf = (angle(placed[2]) - angle(placed[1])).abs();
        assert!(
            hub_to_first_leaf > leaf_to_leaf,
            "hub {hub_to_first_leaf} vs leaf {leaf_to_leaf}"
        );
    }

    #[test]
    fn absent_weights_fall_back_to_an_even_split() {
        let config = Radial {
            angular_policy: RadialAngularPolicy::Weighted,
            ..Radial::default()
        };
        let owned = vec![ring(0, 1.0), ring(1, 1.0)];
        let placed = place(&config, &items(&owned));
        for point in &placed {
            assert!(point.x.is_finite() && point.y.is_finite(), "{point:?}");
        }
        assert!((radius_of(&config, placed[0]) - config.ring_spacing).abs() < 1e-3);
    }

    #[test]
    fn an_undisclosed_ring_takes_the_outer_ring() {
        let config = Radial::default();
        let owned = vec![ring(0, 0.0), ring(1, 1.0), item_with_axis(2, None)];
        let placed = place(&config, &items(&owned));
        assert!(
            radius_of(&config, placed[2]) > radius_of(&config, placed[1]),
            "the unreachable item sits outside the deepest ring"
        );
    }

    #[test]
    fn a_negative_ring_is_unassigned_not_the_centre() {
        // Rounding a negative ring toward zero would put a node the producer
        // could not classify at the focus, which is the most emphatic spot on
        // the canvas. `OuterRing` distinguishes the two readings: rounding to
        // ring zero would put it at the centre, treating it as unassigned puts
        // it outside everything.
        let config = Radial::default();
        let owned = vec![ring(0, 2.0), ring(1, -1.0)];
        let placed = place(&config, &items(&owned));
        assert!(
            radius_of(&config, placed[1]) > radius_of(&config, placed[0]),
            "a negative ring is unassigned, not the focus"
        );
    }
}
