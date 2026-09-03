// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! One module per analytic family, plus the helpers they share.
//!
//! Every family answers the same question — given the score's items in
//! arrangement order, where does each one go — and answers it in one pass with
//! no state carried between calls. A family may look at all the items at once
//! (rings need to know who else is on the ring; columns need to know the column
//! exists), which is why these take the whole ordered slice rather than one
//! item at a time.

use sceno::{Arrangement, AxisValue, Placement, ScoreItem, Vec2};

mod axial;
mod embedded;
mod lsystem;
mod penrose;
mod radial;
mod stack;

/// Place every item, in the order given.
///
/// Returns `None` for [`Arrangement::Custom`], which is not this crate's to
/// solve: custom arrangements dispatch through the `scenograph` registry, and a
/// built-in solver quietly inventing positions for one would be exactly the
/// silent-wrong-placement failure the contract is built to avoid.
pub(crate) fn arrange(arrangement: &Arrangement, items: &[&ScoreItem]) -> Option<Vec<Vec2>> {
    Some(match arrangement {
        // Spiral, Grid, Geographic and Hulls stay in `solve`: they are pure
        // per-item functions and the spiral additionally needs the measured
        // spacing that `solve` computes across the whole score.
        Arrangement::Spiral(_)
        | Arrangement::Grid(_)
        | Arrangement::Geographic(_)
        | Arrangement::Hulls(_) => return None,
        Arrangement::Stack(config) => stack::place(config, items),
        Arrangement::Penrose(config) => penrose::place(config, items),
        Arrangement::LSystem(config) => lsystem::place(config, items),
        Arrangement::Timeline(config) => axial::timeline(config, items),
        Arrangement::Kanban(config) => axial::kanban(config, items),
        Arrangement::Embedded(config) => embedded::place(config, items),
        Arrangement::Radial(config) => radial::place(config, items),
        Arrangement::Custom { .. } => return None,
    })
}

/// True when this arrangement is one the per-item path in `solve` still owns.
pub(crate) fn is_per_item(arrangement: &Arrangement) -> bool {
    matches!(
        arrangement,
        Arrangement::Spiral(_)
            | Arrangement::Grid(_)
            | Arrangement::Geographic(_)
            | Arrangement::Hulls(_)
    )
}

/// The item's disclosed numeric axis coordinate, if it has one.
pub(crate) fn numeric_axis(item: &ScoreItem) -> Option<f64> {
    match &item.axis {
        Some(AxisValue::Numeric(value)) => Some(*value),
        _ => None,
    }
}

/// The item's disclosed categorical axis tag, if it has one.
pub(crate) fn categorical_axis(item: &ScoreItem) -> Option<&str> {
    match &item.axis {
        Some(AxisValue::Categorical(tag)) => Some(tag.as_str()),
        _ => None,
    }
}

/// Where an item already is, as far as a score can say.
///
/// The `arrangements` originals offered a "leave it in place" fallback, which
/// worked because a stepper could simply emit no delta and let the live
/// position stand. A solved scene has no previous frame to defer to, so the
/// only thing a score can mean by "where it already is" is a coordinate it
/// disclosed. An item that disclosed none has never been anywhere, and the
/// arrangement's own origin is the honest answer.
pub(crate) fn disclosed_position(item: &ScoreItem, origin: Vec2) -> Vec2 {
    match item.placement {
        Placement::Coordinate(at) => at,
        _ => origin,
    }
}

/// FNV-1a over a source reference.
///
/// Hash-ordered placement has to be stable across processes and across runs, so
/// it cannot use `DefaultHasher`: that is `SipHash` seeded per process, and a
/// layout that reshuffles itself on restart is not a layout. Spelling the hash
/// out here also keeps it stable against a future std change.
pub(crate) fn stable_hash(item: &ScoreItem) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in item
        .source
        .adapter
        .as_bytes()
        .iter()
        .chain(b"\x1f")
        .chain(item.source.id.as_bytes())
    {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Rotate `offset` by `radians` and translate by `origin`.
pub(crate) fn place_rotated(origin: Vec2, offset: Vec2, radians: f32) -> Vec2 {
    let (sin, cos) = radians.sin_cos();
    Vec2::new(
        origin.x + offset.x * cos - offset.y * sin,
        origin.y + offset.x * sin + offset.y * cos,
    )
}

/// Centre `count` slots of pitch `gap` on zero, returning slot `index`'s offset.
pub(crate) fn centered(index: usize, count: usize, gap: f32) -> f32 {
    (index as f32 - (count.saturating_sub(1) as f32) / 2.0) * gap
}

#[cfg(test)]
pub(crate) mod tests {
    use sceno::{AxisValue, Footprint, Representation, ScoreItem, Size2, SourceRef, Vec2};

    /// A plain measured card, ordinal following its id.
    pub(crate) fn card(id: u32) -> ScoreItem {
        ScoreItem {
            source: SourceRef::new("fixture", id.to_string()),
            ordinal: id,
            footprint: Footprint::Rect {
                size: Size2::new(36.0, 36.0),
            },
            representation: Representation::Card,
            placement: sceno::Placement::Ordinal,
            layer: 0,
            visible: true,
            axis: None,
            embedding: None,
            weight: None,
        }
    }

    pub(crate) fn item_with_axis(id: u32, axis: Option<AxisValue>) -> ScoreItem {
        ScoreItem { axis, ..card(id) }
    }

    pub(crate) fn item_with_embedding(id: u32, embedding: Option<Vec2>) -> ScoreItem {
        ScoreItem {
            embedding,
            ..card(id)
        }
    }

    pub(crate) fn item_with_weight(id: u32, ring: f64, weight: f32) -> ScoreItem {
        ScoreItem {
            axis: Some(AxisValue::Numeric(ring)),
            weight: Some(weight),
            ..card(id)
        }
    }

    /// Borrow an owned fixture as the ordered slice the families take.
    pub(crate) fn items(owned: &[ScoreItem]) -> Vec<&ScoreItem> {
        owned.iter().collect()
    }
}
