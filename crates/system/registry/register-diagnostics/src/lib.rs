// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Diagnostics registry — channel name catalog + descriptor literals
//! + `DiagnosticsRegistry` in one crate.
//!
//! This crate is the keystone extraction for the registrar sweep
//! (Slice 53 of the workspace architecture proposal). The 253
//! channel-name `&'static str` constants and the descriptor
//! literals that key on them are the load-bearing tangle that
//! blocks 8+ other registry extractions (`mod-loader`, `action`,
//! `agent`, `identity`, `input`, `theme`, `workflow`,
//! `workbench-surface`). With both halves in this crate, those
//! registries can each `use register_diagnostics::channels::*`
//! and the relevant descriptor types directly without going
//! through the shell-side runtime mod.

pub mod channels;
pub mod descriptor;
pub mod emit;

pub use descriptor::*;
pub use emit::{
    DiagnosticEvent, SpanPhase, StructuredPayloadField, emit_event, emit_span_duration,
    install_global_sender,
};
