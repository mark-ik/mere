// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The two axial boards: a continuous axis ([`Timeline`]) and a categorical one
//! ([`Kanban`]).

use sceno::{Kanban, ScoreItem, Timeline, TimelineFallback, Vec2};

use super::{categorical_axis, disclosed_position, numeric_axis};

/// Items with x from their disclosed coordinate, normalized across the axis;
/// items landing on the same x stack downward.
pub(super) fn timeline(config: &Timeline, items: &[&ScoreItem]) -> Vec<Vec2> {
    let disclosed: Vec<Option<f64>> = items.iter().map(|item| numeric_axis(item)).collect();

    let (min, max) = disclosed
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), value| {
            (lo.min(*value), hi.max(*value))
        });
    // A single disclosed coordinate — or many identical ones — has no span to
    // normalize against. Everything sits at the axis origin rather than being
    // divided by zero into NaN, which would place items nowhere at all.
    let span = (max - min) as f32;
    let normalize = |value: f64| -> f32 {
        if span > 0.0 {
            config.origin.x + ((value - min) as f32 / span) * config.axis_length
        } else {
            config.origin.x
        }
    };

    // Rows are assigned per distinct x, in the order the axis reaches them, so
    // two items at the same instant stack instead of overprinting.
    let mut rows_at: Vec<(i64, usize)> = Vec::new();
    let mut row_for = |x: f32| -> usize {
        // Keyed at whole world units: "nearby" in the original, made exact so
        // the same score always produces the same rows.
        let key = x.round() as i64;
        match rows_at.iter_mut().find(|(at, _)| *at == key) {
            Some((_, used)) => {
                *used += 1;
                *used - 1
            }
            None => {
                rows_at.push((key, 1));
                0
            }
        }
    };

    let mut placed = Vec::with_capacity(items.len());
    for (item, value) in items.iter().zip(&disclosed) {
        placed.push(match value {
            Some(value) => {
                let x = normalize(*value);
                Vec2::new(x, config.origin.y + row_for(x) as f32 * config.row_gap)
            }
            None => match config.fallback {
                TimelineFallback::LeaveInPlace => disclosed_position(item, config.origin),
                TimelineFallback::StackBelowOrigin => {
                    let x = config.origin.x;
                    Vec2::new(
                        x,
                        config.origin.y - (row_for(x) + 1) as f32 * config.row_gap,
                    )
                }
                TimelineFallback::StackPastEnd => {
                    let x = config.origin.x + config.axis_length;
                    Vec2::new(x, config.origin.y + row_for(x) as f32 * config.row_gap)
                }
            },
        });
    }
    placed
}

/// Items bucketed into named columns by their disclosed tag.
pub(super) fn kanban(config: &Kanban, items: &[&ScoreItem]) -> Vec<Vec2> {
    // The configured order is canonical and holds even for columns nothing
    // landed in: a kanban board whose "blocked" column vanishes when nothing is
    // blocked has told the reader the wrong thing.
    let mut columns: Vec<&str> = config.column_order.iter().map(String::as_str).collect();

    // A tag nobody configured still earns its own column. Merging every
    // unforeseen tag into one pile would collapse the whole board whenever
    // `column_order` is empty, which is the common case — the caller often
    // knows the axis and not the values. Sorted rather than first-seen, so the
    // columns come out the same whatever order the score listed its items in.
    let mut unlisted: Vec<&str> = items
        .iter()
        .filter_map(|item| categorical_axis(item))
        .filter(|tag| !columns.contains(tag))
        .collect();
    unlisted.sort_unstable();
    unlisted.dedup();
    columns.extend(&unlisted);

    // An item that disclosed no tag at all is a different case from one whose
    // tag is merely unlisted: there is nothing to name a column after.
    let untagged = items.iter().any(|item| categorical_axis(item).is_none());
    let listed = columns.len();
    let other = match (untagged, config.include_other_column, listed) {
        (false, _, _) => None,
        // Its own slot, past the last named column.
        (true, true, _) => Some(listed),
        // The board was told not to grow. With columns to fold into, the last
        // one takes them; with none, they are the only column there is.
        (true, false, 0) => Some(0),
        (true, false, _) => Some(listed - 1),
    };
    let column_count = listed.max(other.map_or(0, |index| index + 1)).max(1);

    let mut rows_in_column = vec![0usize; column_count];
    let mut placed = Vec::with_capacity(items.len());
    for item in items {
        let column = categorical_axis(item)
            .and_then(|tag| columns.iter().position(|known| *known == tag))
            .or(other)
            .unwrap_or(0);
        let row = rows_in_column[column];
        rows_in_column[column] += 1;
        placed.push(Vec2::new(
            config.origin.x + column as f32 * config.column_gap,
            config.origin.y + row as f32 * config.row_gap,
        ));
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::tests::{item_with_axis, items};
    use sceno::AxisValue;

    fn numeric(id: u32, value: f64) -> ScoreItem {
        item_with_axis(id, Some(AxisValue::Numeric(value)))
    }

    fn tagged(id: u32, tag: &str) -> ScoreItem {
        item_with_axis(id, Some(AxisValue::Categorical(tag.to_string())))
    }

    #[test]
    fn timeline_normalizes_disclosed_values_across_the_axis() {
        let owned = vec![numeric(0, 1900.0), numeric(1, 1950.0), numeric(2, 2000.0)];
        let config = Timeline::default();
        let placed = timeline(&config, &items(&owned));
        assert_eq!(placed[0].x, config.origin.x);
        assert_eq!(placed[2].x, config.origin.x + config.axis_length);
        // Midpoint lands at the midpoint; the axis is linear in the disclosure.
        assert!((placed[1].x - (config.origin.x + config.axis_length / 2.0)).abs() < 0.01);
    }

    #[test]
    fn a_zero_span_does_not_produce_nan() {
        // Every item at the same instant: the original would divide by zero.
        let owned = vec![numeric(0, 7.0), numeric(1, 7.0)];
        let placed = timeline(&Timeline::default(), &items(&owned));
        for point in &placed {
            assert!(point.x.is_finite() && point.y.is_finite(), "{point:?}");
        }
        assert_ne!(placed[0].y, placed[1].y, "coincident items stack");
    }

    #[test]
    fn timeline_stacks_coincident_items_instead_of_overprinting() {
        let owned = vec![numeric(0, 0.0), numeric(1, 0.0), numeric(2, 100.0)];
        let config = Timeline::default();
        let placed = timeline(&config, &items(&owned));
        assert_eq!(placed[0].x, placed[1].x);
        assert_eq!(placed[1].y - placed[0].y, config.row_gap);
    }

    #[test]
    fn kanban_keeps_configured_columns_even_when_empty() {
        // Nothing is "blocked", but the blocked column still occupies its slot,
        // so "done" does not slide left into it.
        let config = Kanban {
            column_order: vec!["todo".into(), "blocked".into(), "done".into()],
            ..Kanban::default()
        };
        let owned = vec![tagged(0, "todo"), tagged(1, "done")];
        let placed = kanban(&config, &items(&owned));
        assert_eq!(placed[0].x, config.origin.x);
        assert_eq!(placed[1].x, config.origin.x + 2.0 * config.column_gap);
    }

    #[test]
    fn an_unlisted_tag_earns_its_own_column_and_an_untagged_item_the_trailing_one() {
        let config = Kanban {
            column_order: vec!["todo".into()],
            ..Kanban::default()
        };
        let owned = vec![
            tagged(0, "todo"),
            tagged(1, "surprise"),
            item_with_axis(2, None),
        ];
        let placed = kanban(&config, &items(&owned));
        assert_eq!(placed[0].x, config.origin.x);
        // "surprise" is a tag, so it gets a column rather than joining the pile.
        assert_eq!(placed[1].x, config.origin.x + config.column_gap);
        // The untagged item has no tag to name a column after, so it lands past
        // every named one.
        assert_eq!(placed[2].x, config.origin.x + 2.0 * config.column_gap);
    }

    #[test]
    fn unlisted_tags_order_alphabetically_not_by_appearance() {
        // A board whose columns reorder because the score listed its items
        // differently is a board nobody can navigate twice.
        let owned = vec![tagged(0, "zulu"), tagged(1, "alpha"), tagged(2, "mike")];
        let config = Kanban::default();
        let placed = kanban(&config, &items(&owned));
        assert_eq!(placed[1].x, config.origin.x, "alpha is leftmost");
        assert_eq!(placed[2].x, config.origin.x + config.column_gap);
        assert_eq!(placed[0].x, config.origin.x + 2.0 * config.column_gap);
    }

    #[test]
    fn without_a_trailing_column_strays_fold_into_the_last_one() {
        // The board was told not to grow. Folding keeps every item on it; the
        // alternative is an item placed past a column that does not exist.
        let config = Kanban {
            column_order: vec!["todo".into(), "done".into()],
            include_other_column: false,
            ..Kanban::default()
        };
        let owned = vec![tagged(0, "done"), item_with_axis(1, None)];
        let placed = kanban(&config, &items(&owned));
        let done_x = config.origin.x + config.column_gap;
        assert_eq!(placed[0].x, done_x);
        assert_eq!(
            placed[1].x, done_x,
            "the untagged item joined the final column"
        );
        assert_ne!(
            placed[0].y, placed[1].y,
            "and stacked rather than overprinted"
        );
    }
}
