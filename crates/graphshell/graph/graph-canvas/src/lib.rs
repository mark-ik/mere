/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `graph-canvas` — framework-agnostic graph-view canvas for Graphshell.
//!
//! Owns scene derivation, camera and projection rules, interaction and hit
//! testing, LOD and culling policy, render-packet derivation, and backend
//! selection. Emits typed packets and actions rather than rendering directly
//! or mutating application state.
//!
//! Sibling crates:
//! - `kernel` — portable graph data model (graph truth)
//! - `graph-layout` — graph layout algorithms (`Layout<N>` trait,
//!   force-directed / Barnes-Hut / Phyllotaxis / Penrose / etc. + the
//!   cartography adapters). Extracted from `graph-canvas::layout` on
//!   2026-05-18 per the cartography layer brief §9 step 4.
//! - `forme` — per-graph-view workbench arrangement authority (formerly
//!   `graph-tree`)
//! - `node-lineage` — owner-scoped navigation-lineage model (formerly
//!   `graph-memory`)

pub mod backend;
pub mod camera;
pub mod derive;
pub mod engine;
pub mod fields;
pub mod hit_test;
pub mod input;
pub mod interaction;
pub mod lod;
pub mod navigation;
pub mod node_style;
pub mod packet;
pub mod projection;
pub mod scene;
pub mod scene_composition;
pub mod scene_physics;
pub mod scene_region;
pub mod scripting;
