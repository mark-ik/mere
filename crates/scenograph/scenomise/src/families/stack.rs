// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Layered stack: layers advance along x, members stack along y.

use sceno::{ScoreItem, Stack, Vec2};

use super::{centered, numeric_axis};

/// Group items by their disclosed layer, then centre both axes.
///
/// An item with no disclosed layer lands in layer zero rather than being
/// dropped. A stack with nothing disclosed is therefore one column, which is a
/// truthful rendering of "nobody said what the layers are" and not a crash.
pub(super) fn place(config: &Stack, items: &[&ScoreItem]) -> Vec<Vec2> {
    // Layers are ordered by their disclosed value, not by first appearance, so
    // rank -3 sits left of rank 0 whatever order the score listed them in.
    let layers: Vec<i64> = items.iter().map(|item| layer_of(item)).collect();
    let mut distinct = layers.clone();
    distinct.sort_unstable();
    distinct.dedup();

    let mut rows_in_layer = vec![0usize; distinct.len()];
    let mut slot = Vec::with_capacity(items.len());
    for layer in &layers {
        let column = distinct.binary_search(layer).expect("layer is in distinct");
        slot.push((column, rows_in_layer[column]));
        rows_in_layer[column] += 1;
    }

    slot.into_iter()
        .map(|(column, row)| {
            Vec2::new(
                config.center.x + centered(column, distinct.len(), config.layer_gap),
                config.center.y + centered(row, rows_in_layer[column], config.row_gap),
            )
        })
        .collect()
}

fn layer_of(item: &ScoreItem) -> i64 {
    numeric_axis(item)
        .map(|value| value.round() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::tests::{item_with_axis, items};
    use sceno::AxisValue;

    #[test]
    fn layers_advance_on_x_and_members_stack_on_y() {
        let owned = vec![
            item_with_axis(0, Some(AxisValue::Numeric(0.0))),
            item_with_axis(1, Some(AxisValue::Numeric(1.0))),
            item_with_axis(2, Some(AxisValue::Numeric(1.0))),
        ];
        let placed = place(&Stack::default(), &items(&owned));
        assert!(placed[0].x < placed[1].x, "layer 1 is right of layer 0");
        assert_eq!(placed[1].x, placed[2].x, "same layer shares a column");
        assert_ne!(placed[1].y, placed[2].y, "members of a layer separate");
    }

    #[test]
    fn layers_order_by_value_not_by_appearance() {
        // The score lists the deeper layer first; the placement must not.
        let owned = vec![
            item_with_axis(0, Some(AxisValue::Numeric(5.0))),
            item_with_axis(1, Some(AxisValue::Numeric(-5.0))),
        ];
        let placed = place(&Stack::default(), &items(&owned));
        assert!(placed[1].x < placed[0].x);
    }

    #[test]
    fn an_undisclosed_layer_is_zero_not_a_dropped_item() {
        let owned = vec![item_with_axis(0, None), item_with_axis(1, None)];
        let placed = place(&Stack::default(), &items(&owned));
        assert_eq!(placed.len(), 2);
        assert_eq!(placed[0].x, placed[1].x, "one column");
    }
}
