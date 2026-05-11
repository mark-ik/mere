/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Focus authorities + command-surface session.
//!
//! **Migrated to `mere-graphshell::authorities` (2026-05-09).** This
//! module re-exports the canonical types from the new location for
//! backward compat. New code should import from
//! `mere_graphshell::authorities`.

pub use mere_graphshell::authorities::*;
