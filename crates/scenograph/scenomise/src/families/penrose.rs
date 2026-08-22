//! Penrose aperiodic tiling: items take tiling vertices, nearest the centre
//! first.
//!
//! The Robinson-triangle geometry is carried over from `arrangements` unchanged
//! — it was already a set of pure functions over points, with only the frame
//! around it tied to a stepper.

use sceno::{Penrose, PenroseVariant, ScoreItem, SubdivisionCount, Vec2};

const INV_PHI: f32 = 0.618_034;

/// Build a tiling big enough for the items, then hand out vertices in order.
pub(super) fn place(config: &Penrose, items: &[&ScoreItem]) -> Vec<Vec2> {
    if items.is_empty() {
        return Vec::new();
    }
    let depth = match config.subdivision_count {
        SubdivisionCount::Explicit(depth) => depth,
        SubdivisionCount::Auto => choose_auto_depth(config.variant, items.len()),
    };
    let vertices = sort_center_out(
        build_tiling(config.variant, depth, config.tile_scale),
        config.center,
    );
    if vertices.is_empty() {
        return vec![config.center; items.len()];
    }
    // More items than vertices only happens at an explicit depth too shallow for
    // the score. The last vertex is reused rather than dropping the overflow:
    // an item placed on top of another is visible, an item silently absent is
    // not.
    (0..items.len())
        .map(|index| vertices[index.min(vertices.len() - 1)])
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum TriangleKind {
    /// Golden-gnomon acute triangle (apex angle 36°). "L"/"thick" in P3, "kite
    /// half" in P2.
    Acute,
    /// Golden-gnomon obtuse triangle. "S"/"thin" in P3, "dart half" in P2.
    Obtuse,
}

#[derive(Debug, Clone, Copy)]
struct Triangle {
    kind: TriangleKind,
    a: Vec2,
    b: Vec2,
    c: Vec2,
}

/// Both P2 and P3 seed from a ring of 10 triangles about the origin — five
/// mirrored acute pairs forming a regular decagon. The subdivision rules
/// differ, not the seed.
fn initial_ring(scale: f32) -> Vec<Triangle> {
    (0..10)
        .map(|i| {
            let angle_a = i as f32 * std::f32::consts::TAU / 10.0;
            let angle_b = (i as f32 + 1.0) * std::f32::consts::TAU / 10.0;
            Triangle {
                kind: if i % 2 == 0 {
                    TriangleKind::Acute
                } else {
                    TriangleKind::Obtuse
                },
                a: Vec2::ZERO,
                b: Vec2::new(scale * angle_a.cos(), scale * angle_a.sin()),
                c: Vec2::new(scale * angle_b.cos(), scale * angle_b.sin()),
            }
        })
        .collect()
}

fn lerp(p: Vec2, q: Vec2, t: f32) -> Vec2 {
    Vec2::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t)
}

/// P3 Robinson rules.
fn subdivide_p3(triangle: Triangle) -> Vec<Triangle> {
    match triangle.kind {
        TriangleKind::Acute => {
            let p = lerp(triangle.a, triangle.b, INV_PHI);
            vec![
                Triangle {
                    kind: TriangleKind::Acute,
                    a: triangle.c,
                    b: p,
                    c: triangle.a,
                },
                Triangle {
                    kind: TriangleKind::Obtuse,
                    a: p,
                    b: triangle.c,
                    c: triangle.b,
                },
            ]
        }
        TriangleKind::Obtuse => {
            let q = lerp(triangle.b, triangle.c, INV_PHI);
            vec![
                Triangle {
                    kind: TriangleKind::Obtuse,
                    a: q,
                    b: triangle.c,
                    c: triangle.a,
                },
                Triangle {
                    kind: TriangleKind::Acute,
                    a: q,
                    b: triangle.a,
                    c: triangle.b,
                },
            ]
        }
    }
}

/// P2 kite-dart rules. Same triangle shapes as P3; the pattern differs through
/// the adjacency pairings.
fn subdivide_p2(triangle: Triangle) -> Vec<Triangle> {
    match triangle.kind {
        TriangleKind::Acute => {
            let p = lerp(triangle.a, triangle.b, INV_PHI);
            let q = lerp(triangle.a, triangle.c, INV_PHI);
            vec![
                Triangle {
                    kind: TriangleKind::Obtuse,
                    a: p,
                    b: q,
                    c: triangle.a,
                },
                Triangle {
                    kind: TriangleKind::Acute,
                    a: q,
                    b: p,
                    c: triangle.b,
                },
                Triangle {
                    kind: TriangleKind::Acute,
                    a: triangle.c,
                    b: q,
                    c: triangle.b,
                },
            ]
        }
        TriangleKind::Obtuse => {
            let p = lerp(triangle.b, triangle.a, INV_PHI);
            vec![
                Triangle {
                    kind: TriangleKind::Obtuse,
                    a: p,
                    b: triangle.c,
                    c: triangle.a,
                },
                Triangle {
                    kind: TriangleKind::Acute,
                    a: triangle.c,
                    b: p,
                    c: triangle.b,
                },
            ]
        }
    }
}

fn subdivide(variant: PenroseVariant, triangle: Triangle) -> Vec<Triangle> {
    match variant {
        PenroseVariant::Rhombus => subdivide_p3(triangle),
        PenroseVariant::KiteDart => subdivide_p2(triangle),
    }
}

fn expand_tiling(variant: PenroseVariant, depth: u8, scale: f32) -> Vec<Triangle> {
    let mut tiles = initial_ring(scale);
    for _ in 0..depth {
        let mut next = Vec::with_capacity(tiles.len() * 3);
        for tile in tiles.drain(..) {
            next.extend(subdivide(variant, tile));
        }
        tiles = next;
    }
    tiles
}

/// Tiling vertices, deduplicated by approximate equality at 1/1000 of a world
/// unit — subdivision produces each shared vertex once per incident triangle.
fn build_tiling(variant: PenroseVariant, depth: u8, scale: f32) -> Vec<Vec2> {
    let tiles = expand_tiling(variant, depth, scale);
    let mut seen: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut vertices = Vec::new();
    for tile in &tiles {
        for point in [tile.a, tile.b, tile.c] {
            let key = (
                (point.x * 1000.0).round() as i32,
                (point.y * 1000.0).round() as i32,
            );
            if seen.insert(key) {
                vertices.push(point);
            }
        }
    }
    vertices
}

/// Nearest the centre first, then translated onto it, so the earliest ordinals
/// land in the densest part of the tiling.
///
/// Divergence from the `arrangements` original, which ranked the vertices by
/// their distance from `center` *before* translating them onto it. The tiling
/// is generated about the origin, so that measured from a point the tiling was
/// not centred on and then moved everything there anyway: at the default centre
/// of `(0, 0)` the two agree exactly, and away from it the original's
/// "centre-out" ordering ran outward from somewhere else.
fn sort_center_out(mut vertices: Vec<Vec2>, center: Vec2) -> Vec<Vec2> {
    vertices.sort_by(|a, b| {
        let square = |p: &Vec2| p.x * p.x + p.y * p.y;
        square(a).total_cmp(&square(b))
    });
    for vertex in vertices.iter_mut() {
        vertex.x += center.x;
        vertex.y += center.y;
    }
    vertices
}

/// Smallest depth whose deduplicated vertex count covers the items. Capped at 8
/// to bound memory; a real score rarely needs past 6.
fn choose_auto_depth(variant: PenroseVariant, item_count: usize) -> u8 {
    (0u8..=8)
        .find(|depth| build_tiling(variant, *depth, 1.0).len() >= item_count)
        .unwrap_or(8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::tests::{item_with_axis, items};

    fn plain(count: u32) -> Vec<ScoreItem> {
        (0..count).map(|id| item_with_axis(id, None)).collect()
    }

    #[test]
    fn subdivision_grows_the_tile_count_for_both_variants() {
        for variant in [PenroseVariant::Rhombus, PenroseVariant::KiteDart] {
            let shallow = expand_tiling(variant, 1, 1.0).len();
            let deep = expand_tiling(variant, 2, 1.0).len();
            assert!(deep > shallow, "{variant:?}: {deep} !> {shallow}");
        }
    }

    #[test]
    fn auto_depth_picks_a_tiling_that_fits_the_items() {
        for count in [1usize, 10, 60, 200] {
            let depth = choose_auto_depth(PenroseVariant::Rhombus, count);
            let available = build_tiling(PenroseVariant::Rhombus, depth, 1.0).len();
            assert!(available >= count, "depth {depth} gave {available} for {count}");
        }
    }

    #[test]
    fn the_two_variants_produce_different_tilings() {
        let p3 = build_tiling(PenroseVariant::Rhombus, 3, 100.0);
        let p2 = build_tiling(PenroseVariant::KiteDart, 3, 100.0);
        assert_ne!(p3.len(), p2.len());
    }

    #[test]
    fn early_ordinals_land_nearer_the_centre() {
        let config = Penrose::default();
        let owned = plain(24);
        let placed = place(&config, &items(&owned));
        let reach = |at: Vec2| (at.x - config.center.x).hypot(at.y - config.center.y);
        assert!(reach(placed[0]) <= reach(placed[23]));
    }

    #[test]
    fn every_item_is_placed_exactly_once_per_vertex() {
        let owned = plain(30);
        let placed = place(&Penrose::default(), &items(&owned));
        assert_eq!(placed.len(), 30);
        let mut keys: Vec<(i32, i32)> = placed
            .iter()
            .map(|p| ((p.x * 100.0) as i32, (p.y * 100.0) as i32))
            .collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 30, "auto depth must not double up vertices");
    }

    #[test]
    fn an_empty_score_places_nothing() {
        assert!(place(&Penrose::default(), &[]).is_empty());
    }
}
