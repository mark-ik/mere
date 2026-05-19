// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `forme` — per-graph-view workbench arrangement authority.
//!
//! Named for the printing-press *forme*: the locked-up assembly of typeset
//! pieces ready to print. A forme is whatever shape the typesetter locks up
//! — not forced into a tree. This crate plays the same role for a graph view's
//! workbench arrangement: it projects graph members + edges into the
//! arrangement, which may be tree-shaped, lattice-shaped, or arbitrarily
//! connected depending on the graphlet topology.
//!
//! Boundary in the printing-press stack:
//!
//! - **graph truth** (mere-kernel `Graph`) → **forme** (workbench arrangement
//!   authority) → **platen** (graph-aware print/composition stage that turns
//!   the forme into surface/pane output) → **verso-core** (receptor of
//!   surface/tile lifecycle, communicating rearrangement up the stack from
//!   tile → forme → graph truth).
//!
//! One `GraphTree<N>` per graph view (the type name still says "tree" — that
//! is the *default output shape*, not the input contract; the input is any
//! graphlet topology). Contains all members — active, warm, and cold —
//! organized by graph topology with multiple projection lenses.
//!
//! No egui. No iced. No winit. No wgpu. Pure data + pure functions.

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
