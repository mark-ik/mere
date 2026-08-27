// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Deprecated compatibility surface for [`esp::embed`].
//!
//! Existing Sibylla imports continue to compile. New code should depend on
//! `esp` and import the same items through `esp::embed`.

#![deprecated(since = "0.1.2", note = "use esp::embed instead")]

pub use esp::embed::*;
