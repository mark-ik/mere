// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Lens registry — projection presets that combine
//! physics + layout + theme + filters into named bundles
//! (`lens:default`, `lens:semantic_overlay`, etc.). Lenses are the
//! user-facing knob for switching between graph presentations
//! without touching the underlying graph state.
//!
//! Extracted from `registries/atomic/lens/` per Slice 66 of the
//! workspace architecture proposal. The keystone unblocker was
//! Slice 65's promotion of physics tuning + scene collision policy
//! types from the root crate's `graph/` directory to
//! `graph_canvas::physics_config`.

mod layout;
mod physics;
pub mod physics_config;
mod registry;
mod theme;

pub use layout::LayoutMode;
#[allow(unused_imports)]
pub use physics::{
    PHYSICS_ID_DEFAULT, PHYSICS_ID_DRIFT, PHYSICS_ID_SCATTER, PHYSICS_ID_SETTLE, PhysicsProfile,
    PhysicsProfileResolution, canonical_physics_profile_id_hint, physics_profile_descriptors,
    resolve_physics_profile,
};
#[allow(unused_imports)]
pub use registry::{
    GlyphAnchor, GlyphOverlay, LENS_ID_DEFAULT, LENS_ID_SEMANTIC_OVERLAY, LensOverlayDescriptor,
    LensRegistry,
};
pub use theme::{
    THEME_ID_DARK, THEME_ID_DEFAULT, THEME_ID_LIGHT, ThemeData, ThemeResolution,
    deserialize_optional_theme_data, resolve_theme_data, theme_data_id,
};
