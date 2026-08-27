// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Deprecated compatibility surface for [`esp::infer`].
//!
//! Existing Vates imports continue to compile. New code should depend on `esp`
//! and import the same items through `esp::infer`.

#![deprecated(since = "0.1.2", note = "use esp::infer instead")]

pub use esp::infer::*;

/// Compatibility module for Vates's former stub-provider path.
pub mod canned {
    /// The inference stub moved to [`esp::infer::StubInferenceProvider`].
    #[deprecated(since = "0.1.2", note = "use esp::infer::StubInferenceProvider")]
    pub type CannedProvider = esp::infer::StubInferenceProvider;
}

/// Vates's former name for ESP's deterministic inference test provider.
#[deprecated(since = "0.1.2", note = "use esp::infer::StubInferenceProvider")]
pub type CannedProvider = esp::infer::StubInferenceProvider;
