// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Compatibility re-exports for Servitor's original module path.
//!
//! The algebra itself lives in the dependency-free `mere-capability` leaf
//! crate so Servitor and Gemot consume one definition.

pub use capability::{
    Cap, CapError, Capability, FacetNamespace, Mode, ScopePath, assert_capability_laws,
};
