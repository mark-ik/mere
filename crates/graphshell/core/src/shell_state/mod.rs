/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable shell-session state.
//!
//! Modules under `shell_state` hold the host-neutral session state
//! types that the M4 runtime extraction surfaces on `GraphshellRuntime`
//! and the `Frame*` view-models. Each module is WASM-clean (no egui,
//! no servo, no tokio, no platform I/O) and testable without
//! building the full graphshell crate — see
//! [`../../../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-04-22_portable_shell_state_in_graphshell_core.md`](../../../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-04-22_portable_shell_state_in_graphshell_core.md)
//! for the scoping authority and slice-order plan.
//!
//! The host crate (`graphshell`) re-exports types from here via
//! `shell/desktop/ui/gui_state.rs` and sibling modules so existing
//! call sites resolve unchanged.

pub mod authorities;
pub mod command_palette;
pub mod command_surface_telemetry;
pub mod frame_model;
pub mod host_intent;
pub mod omnibar;
pub mod toolbar;
