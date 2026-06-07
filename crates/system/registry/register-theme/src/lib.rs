/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Theme registry crate — bundles the edge-style token vocabulary
//! (`edge_style`) with the theme registry (`theme`) into a single crate.
//!
//! Per the proposal §B.2 "bundle, remove egui" decision (2026-05-04),
//! this crate combines two previously-separate root-crate modules:
//!
//! - `model/graph/edge_style_registry.rs` (666 LOC) — the portable edge
//!   visual-style vocabulary (`EdgeStyleFamily`, `EdgeStrokePattern`,
//!   `EdgeEndpointMarker`, `EdgeAccessibilityMode`, `ThemeContract`,
//!   `ThemeEdgeTokens`, `validate_theme_edge_tokens`).
//! - `shell/desktop/runtime/registries/theme.rs` (608 LOC) — the
//!   `ThemeRegistry` itself, plus `ThemeTokenSet`, `GraphNodeChromeTheme`,
//!   theme seed data, and `register_theme` / `unregister_theme` /
//!   `resolve_theme` APIs.
//!
//! The bundling rationale: `theme` was the sole consumer of `edge_style`
//! (verified by exhaustive grep), and `register_theme()` validates submitted
//! tokens via `edge_style::validate_theme_edge_tokens`. The two modules
//! move in lockstep; splitting them across crates would impose a transitive
//! dep on every consumer of the edge vocabulary.
//!
//! The "remove egui" half of the decision: the original shell-side `theme.rs`
//! had a dead `#[cfg(feature = "egui-host")] pub(crate) use egui::Color32;`
//! gate, paired with `pub(crate) use graphshell_core::color::Color32;` for
//! the non-egui case. Per root `Cargo.toml:96`, `egui-host = []` is now an
//! empty no-op feature — egui has been removed from the dependency graph.
//! The cfg branch was therefore dead code and was dropped during the move;
//! this crate always uses `graphshell_core::color::Color32`.

pub mod edge_style;
pub mod theme;
