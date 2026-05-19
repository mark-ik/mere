/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Return-target classification used by focus-restore and command-
//! surface routing.
//!
//! **Migrated to `mere-graphshell::routing` (2026-05-09).** This module
//! re-exports the canonical types from the new location for backward
//! compat. New code should import from `chrome::routing`.

pub use chrome::routing::*;
