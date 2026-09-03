// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The physics catalog's overlays: extra forces composable onto any law.
//!
//! A law decides the dynamics; an overlay adds one more pull or push over
//! it, and a named *profile* is a law plus the overlays it toggles. These are
//! the donor Graphshell's Level-2 "post-physics forces" (its ten presets were
//! all one law with different overlays), rebuilt as seiche [`Force`](crate::Force)s
//! so they run in the same reset window as the law, after it, in the order a
//! host lists them.
//!
//! - [`DegreeRepulsion`] — hubs push their surroundings apart, by log degree.
//! - [`DomainCluster`] — nodes drift toward the centroid of their group
//!   (site, facet, family — the host's grouping).
//! - [`HubGravity`] — everything is drawn toward hubs, by log degree.
//! - [`DepthGravity`] — depth from a root drives one axis: roots up, leaves
//!   down (the donor's sediment).
//! - [`GridSnap`] — a spring to the nearest grid point (the donor's crystal).
//! - [`GravityLocus`] — a pull toward a point, optionally one that moves on
//!   a slow sine (the donor's tide: never fully settled).
//!
//! Fields ([`CouplingForce`](crate::CouplingForce)), semantic affinity
//! ([`AffinitySpring`](crate::AffinitySpring)) and the arrangement's anchor
//! ([`AnchorSpring`](crate::AnchorSpring)) are overlays too, already landed
//! in their own slots.

mod degree_repulsion;
mod depth_gravity;
mod domain_cluster;
mod gravity_locus;
mod grid_snap;
mod hub_gravity;

pub use degree_repulsion::DegreeRepulsion;
pub use depth_gravity::DepthGravity;
pub use domain_cluster::DomainCluster;
pub use gravity_locus::GravityLocus;
pub use grid_snap::GridSnap;
pub use hub_gravity::HubGravity;
