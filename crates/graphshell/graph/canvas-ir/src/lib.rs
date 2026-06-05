/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `canvas-ir` — the portable canvas scene IR for Mere's graph view.
//!
//! The dependency-light data model the graph view is expressed in: a
//! [`scene::CanvasSceneInput`] of [`scene::CanvasNode`]s / [`scene::CanvasEdge`]s,
//! a [`camera::CanvasViewport`], [`projection::ProjectionMode`] rules, render
//! [`packet`]s (draw items, colors), and scene-object ids / hit shapes
//! ([`scripting`]). No interaction, derivation, physics, or backend logic —
//! those lived in the retired `graph-canvas` monolith; only the IR survives here,
//! shared one-way by `graph-layout` (layout algorithms produce a scene) and
//! `platen` (composition consumes one).
//!
//! Sibling crates: `kernel` (graph truth), `aether` (field algebra),
//! `graph-layout` (layout impls over this IR), `cartography` (projection
//! strategy contract).

pub mod camera;
pub mod packet;
pub mod projection;
pub mod scene;
pub mod scripting;
