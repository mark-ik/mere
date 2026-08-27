// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Compatibility names for the document authority now packaged separately.

pub use knot_document::{AuthoredFile, DocumentFormat, SaveOutcome};

#[doc(hidden)]
pub(crate) use knot_document::write_if_distinct;
