// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! **Athanor**, the distillation furnace of Distillery, the Mere platform's
//! works.
//!
//! An athanor is the furnace that holds a constant low heat for as long as
//! the work takes. This crate is the authority half of the `alembic`
//! workshop: what a furnace pass is, what it may propose — distillation,
//! cleanup, facet proposals — and the grants, runs, pending petitions,
//! refusals, costs, pause, revoke, retry, and dissolve over the bounded actors
//! the workshop admits, of which the furnace was always the first.
//!
//! Two rules it keeps:
//!
//! - **It proposes; it never owns truth.** A furnace pass emits proposals,
//!   and the authority that owns the affected store decides. Athanor is not
//!   a graph-truth authority and does not become one.
//! - **It is scheduled, not resident.** Djinn contains the scheduler and runs
//!   Athanor as one resident service, composing this crate the way it
//!   composes Distillery's works — putting owners together and inventing
//!   nothing. Lifetime is Djinn's; the domain is Distillery's.
//!
//! Ruled 2026-09-02: the authority lives with the domain, by the precedent
//! Djinn set for the works. Beside `alembic` at `ports/distillery/athanor`.
//! The package is `mere-athanor`; the library keeps the name.
//!
//! No implementation yet. The first furnace pass, when it lands, is one more
//! job kind on the works' board, and the projection walk plan carries it into
//! the board's Chronicle as such.

#![forbid(unsafe_code)]
