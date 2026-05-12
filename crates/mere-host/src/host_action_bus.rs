/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-side action-bus adapter.
//!
//! Every keybinding, palette invocation, AccessKit action, drag
//! gesture, and (future) IPC call routes through [`dispatch`] —
//! which checks the permission gate, emits a diagnostic, then
//! calls into the matching `HostRoot` method via [`execute`].
//!
//! Per the [typed action bus refactor plan](../../../../design_docs/mere_docs/implementation_strategy/2026-05-11_typed_action_bus_plan.md):
//! the types + gate trait live in `mere-host-runtime` (portable);
//! the gpui-side execute lives here.
//!
//! v0 cut: uses `PermitEverythingGate` so behaviour is unchanged
//! from pre-bus dispatch. The capability-gate catalogue brief
//! lands a real `SessionPolicyGate` that reads
//! `manifest.policy.overrides`.

use gpui::Context;
use mere_frame::PaneContent;
use mere_host_runtime::{
    ActionKind, ActionTarget, BusAction, PermissionDecision, PermissionGate, TearOutMode,
    check_permission,
};

use crate::HostRoot;

/// Run a kind through target inference + dispatch in one call.
/// Most keybinding handlers route here.
pub(crate) fn dispatch_kind(
    this: &mut HostRoot,
    kind: ActionKind,
    cx: &mut Context<HostRoot>,
) {
    let target = current_target_for_kind(this, &kind);
    dispatch(this, BusAction { target, kind }, cx);
}

/// Run a fully-constructed `BusAction` through the gate +
/// diagnostics + execute. Used by call sites that constructed the
/// target explicitly (e.g. drag-drop carrying a `TileDragPayload`,
/// switcher rows carrying a `SessionId`).
pub(crate) fn dispatch(
    this: &mut HostRoot,
    action: BusAction,
    cx: &mut Context<HostRoot>,
) {
    let gate: &dyn PermissionGate = this.permission_gate.as_ref();
    match check_permission(&action, gate) {
        PermissionDecision::Allow => {
            tracing::debug!(
                target = ?action.target,
                kind = ?action.kind,
                "action.dispatched"
            );
            execute(this, action, cx);
        }
        PermissionDecision::Deny(reason) => {
            tracing::warn!(
                target = ?action.target,
                kind = ?action.kind,
                ?reason,
                "action.denied"
            );
        }
    }
}

/// Map a kind to its natural target given the host's current
/// state. Keybindings don't carry explicit targets — this helper
/// resolves them.
///
/// When changing a kind's natural target, only this function
/// changes. All dispatch sites stay the same.
pub(crate) fn current_target_for_kind(this: &HostRoot, kind: &ActionKind) -> ActionTarget {
    use ActionKind::*;
    match kind {
        // App-scoped lifecycle / chrome
        Quit
        | OpenNewWindow
        | OpenPalette
        | ToggleGraphSwitcher
        | CycleShellbarPosition
        | CycleWorkbenchStripPosition
            => ActionTarget::App,

        // Frame-scoped (affect this window's layout, not a specific pane)
        ToggleWorkbench
        | ToggleGloss
        | ToggleApparatus
        | SummonOrreryForNewGraph
        | SummonOrreryForGraph(_)
            => ActionTarget::Frame(this.frame_layout.id.clone()),

        // Pane-scoped — the active workbench's pane when one is set,
        // else falls back to App so the action no-ops gracefully.
        FocusOmnibar
        | NavigateTo { .. }
        | GoBack
        | GoForward
        | Reload
        | FocusTile { .. }
        | CloseTile { .. }
        | ClosePane
        | TearOutTile { .. }
        | PromoteLeafToBranch
        | PromoteLeafToFork
            => this
                .active_workbench
                .map(ActionTarget::Pane)
                .unwrap_or(ActionTarget::App),

        // Session-scoped — reserved for manifest/fork machinery.
        // v0 falls back to App; real wiring lands with the
        // manifest-store host integration.
        ForkStickyNoteSession
        | KillSession
        | SnapshotSessionToEngram
        | ConsolidateBranchToEngram
        | ConsolidateForkToEngram
            => ActionTarget::App,

        // Broadcast — fans out from a frame.
        BroadcastNavigate { .. } => ActionTarget::Frame(this.frame_layout.id.clone()),
    }
}

/// Execute an allowed action against `HostRoot`. The big match
/// on (target, kind) — calls into existing `HostRoot` methods.
///
/// New actions land here as new arms; existing methods stay
/// untouched. The bus is a wrapper over the host's existing
/// surface, not a replacement.
fn execute(
    this: &mut HostRoot,
    action: BusAction,
    cx: &mut Context<HostRoot>,
) {
    use ActionKind::*;
    let BusAction { target, kind } = action;
    match kind {
        Quit => cx.quit(),

        OpenNewWindow => this.open_new_window(cx),

        OpenPalette => {
            // Palette needs a Window handle; bus dispatch from a
            // listener that already has Window should call
            // HostRoot's palette toggle directly. Surface a debug
            // so we notice if a non-Window path tries this.
            tracing::debug!(
                "OpenPalette via bus has no Window handle; \
                 use the keybinding listener path that owns Window"
            );
        }

        FocusOmnibar => {
            tracing::debug!(
                "FocusOmnibar via bus has no Window handle; \
                 use the keybinding listener path that owns Window"
            );
        }

        GoBack => this.go_back(cx),
        GoForward => this.go_forward(cx),
        Reload => this.reload(cx),

        CycleShellbarPosition => this.cycle_shellbar_position(cx),
        CycleWorkbenchStripPosition => this.cycle_workbench_strip_position(cx),

        ToggleWorkbench => this.toggle_panel(PaneContent::Workbench, cx),
        ToggleGloss => this.toggle_panel(PaneContent::Gloss, cx),
        ToggleApparatus => this.toggle_panel(PaneContent::Apparatus, cx),

        SummonOrreryForNewGraph => this.summon_orrery_for_new_graph(cx),
        SummonOrreryForGraph(graph_id) => this.summon_orrery_for_graph(graph_id, cx),
        ToggleGraphSwitcher => this.toggle_graph_switcher(cx),

        NavigateTo { address, new_tile } => {
            let mode = if new_tile {
                mere_host_runtime::NavigateMode::NewTile
            } else {
                mere_host_runtime::NavigateMode::WithinTile
            };
            this.navigate_to(address, mode, cx);
        }

        FocusTile { index } => {
            if let ActionTarget::Pane(pane_id) = target {
                this.focus_tile_in(pane_id, index, cx);
            }
        }

        CloseTile { index } => {
            if let ActionTarget::Pane(pane_id) = target {
                this.close_tile_in(pane_id, index, cx);
            }
        }

        ClosePane => {
            if let ActionTarget::Pane(pane_id) = target {
                this.close_pane(pane_id, cx);
            }
        }

        TearOutTile { mode } => {
            // Resolve the donor pane + tile index from target +
            // active state. Bus-driven tear-out (drag-drop) sets
            // both explicitly via PromoteLeafTo*; keyboard-driven
            // tear-out uses the active tile.
            let donor = match target {
                ActionTarget::Pane(pid) => Some(pid),
                _ => this.active_workbench,
            };
            let Some(donor_pane) = donor else {
                tracing::debug!(?mode, "TearOutTile: no donor pane resolvable");
                return;
            };
            let active_idx = this
                .panes
                .get(&donor_pane)
                .and_then(|s| s.as_workbench())
                .and_then(|w| w.tiles.active_index());
            let Some(idx) = active_idx else {
                tracing::debug!(?mode, ?donor_pane, "TearOutTile: no active tile");
                return;
            };
            match mode {
                TearOutMode::Leaf => {
                    this.tear_out_tile_sticky_note_for(donor_pane, idx, cx);
                }
                TearOutMode::ForkMinimized => {
                    this.tear_out_tile_to_new_graph(true, cx);
                }
                TearOutMode::ForkVisible | TearOutMode::Fork => {
                    this.tear_out_tile_to_new_graph(false, cx);
                }
                TearOutMode::Branch => {
                    // Phase 3 branch — wired in the same turn as
                    // this bus migration; routes to the new
                    // host-side method.
                    this.tear_out_tile_as_branch_for(donor_pane, idx, cx);
                }
            }
        }

        PromoteLeafToBranch | PromoteLeafToFork => {
            // Toast-button actions. Implemented in Phase 3's toast
            // UI slice. v0: log and bail — the actions exist for
            // the bus to be complete; behaviour lands with the UI.
            tracing::debug!(?kind, "promote-leaf action received; toast UI not yet wired");
        }

        ForkStickyNoteSession
        | KillSession
        | SnapshotSessionToEngram
        | ConsolidateBranchToEngram
        | ConsolidateForkToEngram => {
            tracing::debug!(
                ?kind,
                "session-lifecycle action received; manifest-store host wiring pending"
            );
        }

        BroadcastNavigate { address } => {
            tracing::debug!(
                %address,
                "BroadcastNavigate received; fan-out implementation pending"
            );
        }
    }
}
