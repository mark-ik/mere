/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Stable surface ids for the per-surface tile caches
//! (`RenderCore::rasterize_for`; netrender P4, shell paint plan 2026-07-03).
//!
//! Tile invalidation diffs a scene against the SAME surface's previous frame,
//! so every retained surface meerkat rasterizes needs its own id — routing two
//! surfaces through one id makes each render diff against the other's scene
//! and rebuild every tile. Ids are arbitrary but must be stable across frames
//! and distinct across surfaces; group bases keep families apart.

use crate::GraphMemberId;

pub(crate) const CHROME_FULL: u64 = 1;
pub(crate) const CHROME_BASE: u64 = 2;
pub(crate) const CHROME_ORRERY: u64 = 3;
pub(crate) const ORRERY_CANVAS: u64 = 4;
pub(crate) const WORKBENCH: u64 = 5;
pub(crate) const GLOSS_MINIMAP: u64 = 6;
pub(crate) const SNAPSHOT_PEEK: u64 = 7;
pub(crate) const SNAPSHOT_CARD: u64 = 8;
pub(crate) const FIND_OVERLAY_NORMAL: u64 = 9;
pub(crate) const FIND_OVERLAY_ACTIVE: u64 = 10;
pub(crate) const WORKBENCH_GHOST: u64 = 11;
pub(crate) const WINDOW_CONTROLS: u64 = 12;
pub(crate) const DIVIDER: u64 = 13;
pub(crate) const SELECTION_FILL: u64 = 14;
pub(crate) const NODE_THUMBNAIL: u64 = 15;
pub(crate) const UNVISITED_CARD: u64 = 16;
pub(crate) const KEPT_WARM_BADGE: u64 = 17;
pub(crate) const EMPTY_STATE_PANEL: u64 = 18;
/// The chisel status cluster overlay (frame-time meter + recent-trail glyph).
pub(crate) const STATUS_CLUSTER: u64 = 19;

/// Secondary orrery `i` (multi-window / secondary panes).
pub(crate) fn secondary_orrery(i: usize) -> u64 {
    0x100 + i as u64
}

/// One content card's band surface, keyed by the graph member it shows.
pub(crate) fn card(member: GraphMemberId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    member.hash(&mut h);
    // Keep clear of the small fixed ids and the secondary-orrery block.
    h.finish() | 0x8000_0000_0000_0000
}
