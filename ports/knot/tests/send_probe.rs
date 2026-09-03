// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! `KnotEndpoint` must stay `Send`, and this says so where the reason is
//! visible.
//!
//! A resident Graphshell host schedules endpoints across threads, so
//! `ResidentEndpointCatalog` will not register one that is not `Send`. The
//! chain that makes this hold runs through two repos and is easy to break by
//! accident: rhai's `sync` feature makes `rhai::Engine` `Send`, which lets
//! `RhaiEvaluator` satisfy inker's `BlockEvaluator: Send` bound, which is what
//! keeps `KnotEffectAuthority`'s evaluator map `Send`.
//!
//! Without this test, breaking any link in that chain surfaces as a confusing
//! trait error at a catalog registration in another crate. With it, the
//! failure names the type that actually changed.

fn assert_send<T: Send>() {}

#[test]
fn a_knot_endpoint_can_be_scheduled_by_a_resident_host() {
    assert_send::<knot_editor::KnotEndpoint>();
}
