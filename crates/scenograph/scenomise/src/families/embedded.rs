// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Placement at coordinates a producer already computed.
//!
//! One solver for every producer of 2-D coordinates. A dimensionality reduction
//! and a spectral decomposition of a graph Laplacian were separate layouts in
//! `arrangements`, in separate files, differing in their placement half by
//! nothing at all — a producer's coordinate space is the producer's business,
//! and normalizing it before disclosure is too.

use sceno::{Embedded, EmbeddingFallback, ScoreItem, Vec2};

use super::{disclosed_position, place_rotated, stable_hash};

/// Scale, rotate, and translate each disclosed coordinate onto the scene.
pub(super) fn place(config: &Embedded, items: &[&ScoreItem]) -> Vec<Vec2> {
    // The ring only has to be built when something is actually missing, and its
    // radius depends on how far the embedded cluster reaches.
    let ring_radius = matches!(config.fallback, EmbeddingFallback::RingOutside)
        .then(|| ring_radius(config, items))
        .unwrap_or_default();

    items
        .iter()
        .map(|item| match item.embedding {
            Some(at) => place_rotated(
                config.origin,
                Vec2::new(at.x * config.scale, at.y * config.scale),
                config.rotation,
            ),
            None => match config.fallback {
                EmbeddingFallback::LeaveInPlace => disclosed_position(item, config.origin),
                EmbeddingFallback::CollapseToOrigin => config.origin,
                EmbeddingFallback::RingOutside => {
                    // Angle from a stable hash rather than from position in the
                    // score: an unembedded item keeps its spot on the ring when
                    // a different item gains an embedding and leaves.
                    let angle =
                        (stable_hash(item) % 3_600) as f32 / 3_600.0 * std::f32::consts::TAU;
                    Vec2::new(
                        config.origin.x + ring_radius * angle.cos(),
                        config.origin.y + ring_radius * angle.sin(),
                    )
                }
            },
        })
        .collect()
}

/// One step outside the furthest embedded item, so the ring reads as "outside
/// the cluster" rather than sitting in it.
fn ring_radius(config: &Embedded, items: &[&ScoreItem]) -> f32 {
    let furthest = items
        .iter()
        .filter_map(|item| item.embedding)
        .map(|at| (at.x * at.x + at.y * at.y).sqrt() * config.scale)
        .fold(0.0f32, f32::max);
    // An all-unembedded score has no cluster to sit outside of; the configured
    // scale is the only extent anyone has named.
    if furthest > 0.0 {
        furthest * 1.15
    } else {
        config.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::tests::{item_with_embedding, items};

    #[test]
    fn a_disclosed_coordinate_scales_about_the_origin() {
        let config = Embedded {
            origin: Vec2::new(10.0, 20.0),
            scale: 100.0,
            rotation: 0.0,
            ..Embedded::default()
        };
        let owned = vec![item_with_embedding(0, Some(Vec2::new(0.5, -0.25)))];
        let placed = place(&config, &items(&owned));
        assert_eq!(placed[0], Vec2::new(60.0, -5.0));
    }

    #[test]
    fn rotation_turns_the_whole_cloud_about_the_origin() {
        let config = Embedded {
            origin: Vec2::ZERO,
            scale: 1.0,
            rotation: std::f32::consts::FRAC_PI_2,
            ..Embedded::default()
        };
        let owned = vec![item_with_embedding(0, Some(Vec2::new(1.0, 0.0)))];
        let placed = place(&config, &items(&owned));
        assert!(placed[0].x.abs() < 1e-6, "{:?}", placed[0]);
        assert!((placed[0].y - 1.0).abs() < 1e-6, "{:?}", placed[0]);
    }

    #[test]
    fn collapse_puts_the_unembedded_at_the_origin() {
        let config = Embedded {
            origin: Vec2::new(3.0, 4.0),
            fallback: EmbeddingFallback::CollapseToOrigin,
            ..Embedded::default()
        };
        let owned = vec![item_with_embedding(0, None)];
        assert_eq!(place(&config, &items(&owned))[0], Vec2::new(3.0, 4.0));
    }

    #[test]
    fn the_outside_ring_clears_the_embedded_cluster() {
        let config = Embedded {
            origin: Vec2::ZERO,
            scale: 100.0,
            fallback: EmbeddingFallback::RingOutside,
            ..Embedded::default()
        };
        let owned = vec![
            item_with_embedding(0, Some(Vec2::new(1.0, 0.0))),
            item_with_embedding(1, None),
        ];
        let placed = place(&config, &items(&owned));
        let embedded_reach = (placed[0].x.powi(2) + placed[0].y.powi(2)).sqrt();
        let ring_reach = (placed[1].x.powi(2) + placed[1].y.powi(2)).sqrt();
        assert!(
            ring_reach > embedded_reach,
            "{ring_reach} vs {embedded_reach}"
        );
    }

    #[test]
    fn a_ring_position_does_not_move_when_a_neighbour_leaves() {
        // Derived from the source reference, not from position in the score, so
        // an unembedded item keeps its place as the cluster around it changes.
        let config = Embedded {
            scale: 100.0,
            fallback: EmbeddingFallback::RingOutside,
            ..Embedded::default()
        };
        let both = vec![item_with_embedding(7, None), item_with_embedding(9, None)];
        let alone = vec![item_with_embedding(7, None)];
        assert_eq!(
            place(&config, &items(&both))[0],
            place(&config, &items(&alone))[0]
        );
    }
}
