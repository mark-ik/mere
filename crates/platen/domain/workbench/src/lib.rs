// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Temporary compatibility crate for the former Workbench a11y projection.
//!
//! Platen now owns the structural accessibility projection alongside its tiled layout. This
//! package exists only until it is replaced by Genet's `workbench` component crate.

#![doc(html_root_url = "https://docs.rs/workbench/0.0.1")]

/// Compatibility export for callers that used the former domain crate API.
pub use platen::accessibility::project_tile_layout as project_workbench;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
