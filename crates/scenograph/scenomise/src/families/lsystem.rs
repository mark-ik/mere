//! L-system fractal path: items take positions along a turtle walk.
//!
//! Grammar expansion and the walk are carried over from `arrangements`
//! unchanged; only the frame around them was tied to a stepper.

use sceno::{IterationDepth, LSystem, LSystemGrammar, ScoreItem, Vec2};

/// Walk a path long enough for the items, then hand out positions in order.
pub(super) fn place(config: &LSystem, items: &[&ScoreItem]) -> Vec<Vec2> {
    if items.is_empty() {
        return Vec::new();
    }
    let grammar = resolve_grammar(&config.grammar);
    let depth = match config.iteration_depth {
        IterationDepth::Explicit(depth) => depth,
        IterationDepth::Auto => choose_auto_depth(grammar, items.len()),
    };
    let path = normalize_path(&walk(grammar, depth), config);
    if path.is_empty() {
        return vec![config.origin; items.len()];
    }
    (0..items.len())
        .map(|index| {
            let along = index.min(path.len() - 1);
            path[if config.reverse_order {
                path.len() - 1 - along
            } else {
                along
            }]
        })
        .collect()
}

struct GrammarDef {
    /// Starting symbol string.
    axiom: &'static str,
    /// Production rules: a symbol, and what replaces it.
    rules: &'static [(char, &'static str)],
    /// Turn angle for `+` and `-`, in radians.
    angle: f32,
}

const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;

const HILBERT: GrammarDef = GrammarDef {
    axiom: "A",
    rules: &[('A', "-BF+AFA+FB-"), ('B', "+AF-BFB-FA+")],
    angle: 90.0 * DEG_TO_RAD,
};

const KOCH: GrammarDef = GrammarDef {
    axiom: "F",
    rules: &[('F', "F+F--F+F")],
    angle: 60.0 * DEG_TO_RAD,
};

const DRAGON: GrammarDef = GrammarDef {
    axiom: "FX",
    rules: &[('X', "X+YF+"), ('Y', "-FX-Y")],
    angle: 90.0 * DEG_TO_RAD,
};

/// A named grammar the host has not supplied rules for walks as Hilbert. The
/// alternative — placing nothing — would make a typo in a grammar name look
/// like an empty score.
fn resolve_grammar(grammar: &LSystemGrammar) -> &'static GrammarDef {
    match grammar {
        LSystemGrammar::Hilbert | LSystemGrammar::Named(_) => &HILBERT,
        LSystemGrammar::Koch => &KOCH,
        LSystemGrammar::Dragon => &DRAGON,
    }
}

fn expand(grammar: &GrammarDef, depth: u8) -> String {
    let mut current = grammar.axiom.to_string();
    for _ in 0..depth {
        let mut next = String::with_capacity(current.len() * 4);
        for symbol in current.chars() {
            match grammar.rules.iter().find(|(key, _)| *key == symbol) {
                Some((_, replacement)) => next.push_str(replacement),
                None => next.push(symbol),
            }
        }
        current = next;
    }
    current
}

/// Turtle walk over the expansion. `A`/`B` (Hilbert) and `X`/`Y` (Dragon) are
/// non-drawing by the usual convention; only `F` advances.
fn walk(grammar: &GrammarDef, depth: u8) -> Vec<Vec2> {
    let symbols = expand(grammar, depth);
    let (mut x, mut y, mut heading) = (0.0f32, 0.0f32, 0.0f32);
    let mut stack: Vec<(f32, f32, f32)> = Vec::new();
    let mut positions = vec![Vec2::new(x, y)];

    for symbol in symbols.chars() {
        match symbol {
            'F' => {
                x += heading.cos();
                y += heading.sin();
                positions.push(Vec2::new(x, y));
            }
            '+' => heading -= grammar.angle,
            '-' => heading += grammar.angle,
            '[' => stack.push((x, y, heading)),
            ']' => {
                if let Some((sx, sy, sh)) = stack.pop() {
                    (x, y, heading) = (sx, sy, sh);
                }
            }
            _ => {}
        }
    }
    positions
}

/// Smallest depth whose walk yields at least as many positions as there are
/// items. Capped at 10 to bound memory (Hilbert reaches ~1M steps there).
fn choose_auto_depth(grammar: &GrammarDef, item_count: usize) -> u8 {
    (0u8..=10)
        .find(|depth| {
            expand(grammar, *depth)
                .chars()
                .filter(|c| *c == 'F')
                .count()
                + 1
                >= item_count
        })
        .unwrap_or(10)
}

/// Fit the walked path into the configured extent, centred on the origin and
/// rotated.
fn normalize_path(raw: &[Vec2], config: &LSystem) -> Vec<Vec2> {
    let Some(first) = raw.first() else {
        return Vec::new();
    };
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first.x, first.x, first.y, first.y);
    for point in raw.iter().skip(1) {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    // A single-point path has no extent to divide by.
    let extent = (max_x - min_x).max(max_y - min_y).max(1.0);
    let scale = config.size / extent;
    let (center_x, center_y) = ((min_x + max_x) * 0.5, (min_y + max_y) * 0.5);
    let (sin, cos) = config.rotation.sin_cos();
    raw.iter()
        .map(|point| {
            let (dx, dy) = ((point.x - center_x) * scale, (point.y - center_y) * scale);
            Vec2::new(
                config.origin.x + dx * cos - dy * sin,
                config.origin.y + dx * sin + dy * cos,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::tests::{item_with_axis, items};

    fn plain(count: u32) -> Vec<ScoreItem> {
        (0..count).map(|id| item_with_axis(id, None)).collect()
    }

    #[test]
    fn expansion_is_deterministic() {
        assert_eq!(expand(&HILBERT, 1), "-BF+AFA+FB-");
        assert_eq!(expand(&HILBERT, 2), expand(&HILBERT, 2));
    }

    #[test]
    fn every_grammar_walks_a_non_empty_path() {
        for grammar in [&HILBERT, &KOCH, &DRAGON] {
            assert!(walk(grammar, 3).len() > 1);
        }
    }

    #[test]
    fn hilbert_visits_distinct_points() {
        let path = walk(&HILBERT, 3);
        let mut keys: Vec<(i32, i32)> = path
            .iter()
            .map(|p| (p.x.round() as i32, p.y.round() as i32))
            .collect();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "a Hilbert curve does not revisit");
    }

    #[test]
    fn auto_depth_covers_the_items() {
        for count in [1usize, 5, 40, 300] {
            let depth = choose_auto_depth(&HILBERT, count);
            assert!(walk(&HILBERT, depth).len() >= count);
        }
    }

    #[test]
    fn the_path_stays_inside_the_configured_size() {
        let config = LSystem {
            origin: Vec2::ZERO,
            size: 400.0,
            ..LSystem::default()
        };
        let owned = plain(16);
        for point in place(&config, &items(&owned)) {
            assert!(
                point.x.abs() <= 200.001 && point.y.abs() <= 200.001,
                "{point:?}"
            );
        }
    }

    #[test]
    fn reversing_swaps_the_ends_of_the_walk() {
        let owned = plain(8);
        let forward = place(&LSystem::default(), &items(&owned));
        let reversed = place(
            &LSystem {
                reverse_order: true,
                ..LSystem::default()
            },
            &items(&owned),
        );
        assert_ne!(forward[0], reversed[0]);
    }

    #[test]
    fn an_unsupplied_named_grammar_walks_rather_than_placing_nothing() {
        let config = LSystem {
            grammar: LSystemGrammar::Named("spiral-of-theodorus".into()),
            ..LSystem::default()
        };
        let owned = plain(4);
        let placed = place(&config, &items(&owned));
        assert_eq!(placed.len(), 4);
        let mut distinct = placed.clone();
        distinct.dedup();
        assert!(distinct.len() > 1, "the fallback actually walks");
    }
}
