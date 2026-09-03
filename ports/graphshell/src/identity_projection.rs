// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Castellan's identity cards and intents, at their pre-founding path.
//!
//! Moved home to [`castellan::projection`] with the keeper founding
//! (2026-08-14), and the intents took the port's own namespace the same day:
//! `graphshell.identity.*` became `castellan.*`, matching how `knot.*` already
//! names knot's. Receipts written before that carry the old strings, which is
//! correct — they record what ran.

pub use castellan::projection::*;
