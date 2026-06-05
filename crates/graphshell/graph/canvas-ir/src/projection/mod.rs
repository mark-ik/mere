/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! View dimension, projection mode, and z-source types.
//!
//! Describes *how* a graph view projects its 2D layout truth into a rendered
//! scene. Does not own that layout truth.
//!
//! Module layout:
//!
//! - [`types`] — [`ViewDimension`], [`ThreeDMode`], [`ZSource`],
//!   [`ProjectionMode`], and the per-mode tuning configs.
//! - [`math`] — [`ProjectedPosition`] plus the [`project_position`] entry
//!   point used by packet derivation.
//! - [`presets`] — additive preset families ([`TwoDPreset`],
//!   [`TwoPointFiveProjection`]) for the lossless dimension ladder. Wired
//!   into [`ProjectionMode`] in a follow-up cycle; today they ship alongside
//!   the canonical enum so callers can opt in when ready.

pub mod math;
pub mod presets;
pub mod types;

pub use math::{ProjectedPosition, project_position};
pub use presets::{TwoDPreset, TwoPointFiveProjection};
pub use types::{ProjectionConfig, ProjectionMode, ViewDimension};
