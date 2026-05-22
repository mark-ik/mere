// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `forme` — workbench arrangement authority.
//!
//! Named for the printing-press *forme*: the locked-up assembly of typeset
//! pieces ready to print. A forme is whatever shape the typesetter locks up
//! — not forced into a tree. forme owns *what a workbench is*; a tree is one
//! projection of it (platen's job), per the composition spine
//! (`design_docs/mere_docs/technical_architecture/2026-05-21_mere_composition_spine.md`).
//!
//! Boundary: **graph truth** (kernel `Graph`) → **forme** (arrangement
//! authority) → **platen** (projects an arrangement into a presentation plan)
//! → **verso** (realizes tiles/surfaces).
//!
//! ## v1 canonical core (this is the live path)
//!
//! - [`arrangement`] — `Arrangement`: a small, graph-capable, **geometry-free**
//!   arrangement (members / tiles / groups + stacked / split / compare /
//!   focus-path / mirror relations).
//! - [`forme_document`] — `FormeId` / `FormeRef` / `FormeDocument`: a forme as a
//!   durable, session-scoped, graph-bound object.
//!
//! ## Parked (graphshell-era machinery — git-revivable)
//!
//! The modules below are the rich graph-derived-navigation lane (graphlet
//! topology, lenses, reconciliation, pressure, the tree-first `GraphTree` /
//! `TreeTopology`). They are **not** the v1 arrangement core and will be
//! removed from the active crate in a follow-up (kept only as the future
//! `CollapsedGraphlet` / derived-arrangement hook). Do not build new product
//! paths on them.
//!
//! No egui. No iced. No winit. No wgpu. Pure data + pure functions.

// ── v1 canonical core ────────────────────────────────────────────────────────
pub mod arrangement;
pub mod forme_document;

pub use arrangement::{
    Arrangement, ArrangementEdge, ArrangementEdgeKind, ArrangementNode, ArrangementNodeId,
    ArrangementNodeKind, GraphMemberId,
};
pub use forme_document::{FormeDocument, FormeId, FormeRef};

// ── Parked graphshell-era machinery (git-revivable; pending removal) ─────────
mod graphlet;
mod layout;
mod lens;
mod member;
mod nav;
pub mod parity;
pub mod pressure;
mod query;
pub mod reconciliation;
mod topology;
mod tree;
mod ux;

pub use graphlet::*;
pub use layout::*;
pub use lens::*;
pub use member::*;
pub use nav::*;
pub use query::*;
pub use topology::*;
pub use tree::*;
pub use ux::*;

/// Portable rectangle — no framework dependency.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

/// Identity of a tree member. Generic so Graphshell uses `NodeKey`,
/// extensions use `Uuid`, tests use `u64`.
pub trait MemberId:
    Clone + Eq + std::hash::Hash + std::fmt::Debug + serde::Serialize + for<'de> serde::Deserialize<'de>
{
}

impl<T> MemberId for T where
    T: Clone
        + Eq
        + std::hash::Hash
        + std::fmt::Debug
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
{
}
