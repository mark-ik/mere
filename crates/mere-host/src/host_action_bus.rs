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
use mere_frame::{GraphId, PaneContent};
use mere_host_runtime::{
    ActionKind, ActionTarget, BusAction, PermissionDecision, PermissionGate, TearOutMode,
    check_permission,
};

use crate::HostRoot;

/// Resolve `graph_id` against the shared registry and run `f` against
/// the live `Graph` entity. Returns `None` when the graph isn't
/// registered (stale id, closed session). `f` owns deciding whether
/// to `notify()`.
fn with_graph<R>(
    this: &HostRoot,
    graph_id: GraphId,
    cx: &mut Context<HostRoot>,
    f: impl FnOnce(&mut mere_kernel::graph::Graph, &mut Context<mere_kernel::graph::Graph>) -> R,
) -> Option<R> {
    let graph_entity = this.registry.read(cx).get(graph_id).cloned()?;
    Some(graph_entity.update(cx, f))
}

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
        | ReparentPane { .. }
        | TearOutTile { .. }
        | PromoteLeafToBranch
        | PromoteLeafToFork
        | PinTileToFrame { .. }
            => this
                .active_workbench
                .map(ActionTarget::Pane)
                .unwrap_or(ActionTarget::App),

        // Graph-truth mutation — the kind carries `graph_id`, so the
        // dispatch target is App (graph truth is shared across
        // windows, not pane-scoped).
        AssertRelation { .. }
        | RetractRelation { .. }
        | AppendTraversal { .. }
        | RemoveNode { .. }
            => ActionTarget::App,

        // View-local relation hide — scoped to the orrery leaf whose
        // view should suppress the relation. Context-menu dispatch
        // carries an explicit `Pane` target; this fallback only
        // applies to a (currently non-existent) keybinding path.
        HideRelation { .. } => this
            .primary_orrery_pane
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

        ReparentPane { source, side } => {
            // Target = the dispatch's `ActionTarget::Pane`; source
            // = where the user grabbed; side = which edge of the
            // target to attach. The drop-zone gutter rule lives in
            // `crate::drop_zone::infer_drop_side`.
            if let ActionTarget::Pane(target_pane) = target {
                this.reparent_pane(source, target_pane, side, cx);
            } else {
                tracing::debug!(?source, ?side, "ReparentPane: non-Pane target");
            }
        }

        TearOutTile { mode, tile_index } => {
            // Resolve donor pane: explicit `Pane(_)` target wins;
            // otherwise fall back to the active workbench.
            let donor = match target {
                ActionTarget::Pane(pid) => Some(pid),
                _ => this.active_workbench,
            };
            let Some(donor_pane) = donor else {
                tracing::debug!(?mode, "TearOutTile: no donor pane resolvable");
                return;
            };
            // Resolve tile index: explicit `tile_index` wins
            // (drag-drop / right-click on a specific tile);
            // otherwise use the donor's active tile (keyboard
            // shortcut path).
            let idx = tile_index.or_else(|| {
                this.panes
                    .get(&donor_pane)
                    .and_then(|s| s.as_workbench())
                    .and_then(|w| w.tiles.active_index())
            });
            let Some(idx) = idx else {
                tracing::debug!(?mode, ?donor_pane, "TearOutTile: no tile resolvable");
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
                    this.tear_out_tile_as_branch_for(donor_pane, idx, cx);
                }
            }
        }

        PinTileToFrame { node, graph_id } => {
            // Target = anchor pane (the one the user invoked from);
            // fall back to active workbench when target is App.
            let anchor = match target {
                ActionTarget::Pane(pid) => Some(pid),
                _ => this.active_workbench,
            };
            let Some(anchor_pane) = anchor else {
                tracing::debug!(?node, "PinTileToFrame: no anchor pane");
                return;
            };
            this.pin_tile_to_frame(anchor_pane, node, graph_id, cx);
        }

        PromoteLeafToBranch | PromoteLeafToFork => {
            // Toast-button actions. The toast records the donor
            // pane + tile index in `TearoutToastState`; target =
            // Pane(donor_pane), and execute pulls those out via
            // the target. Promote-to-branch/fork then dispatches
            // the equivalent TearOutTile action so the existing
            // tear-out path handles everything (mints graphlet /
            // session, opens window, persists).
            let Some(toast) = this.tearout_toast else {
                tracing::debug!("promote-leaf: no toast state to act on");
                return;
            };
            let promote_mode = if matches!(kind, PromoteLeafToBranch) {
                TearOutMode::Branch
            } else {
                TearOutMode::Fork
            };
            tracing::info!(
                ?promote_mode,
                donor = ?toast.donor_pane,
                tile_index = toast.tile_index,
                "promote_leaf via toast"
            );
            // Reuse the existing tear-out infrastructure — same
            // donor pane + tile index, just a different mode.
            // Synthesises a TearOutTile bus action so all gates +
            // diagnostics apply uniformly.
            let next = BusAction::pane(
                toast.donor_pane,
                ActionKind::TearOutTile {
                    mode: promote_mode,
                    tile_index: Some(toast.tile_index),
                },
            );
            // Direct recursive dispatch — gate already passed for
            // PromoteLeafTo*; the synthetic tear-out re-runs the
            // gate against its own kind so policy still applies
            // separately if branch / fork are gated differently.
            crate::host_action_bus::dispatch(this, next, cx);
        }

        AssertRelation {
            graph_id,
            from,
            to,
            assertion,
        } => {
            with_graph(this, graph_id, cx, |g, gcx| {
                if g.assert_relation(from, to, assertion).is_some() {
                    gcx.notify();
                }
            });
        }

        RetractRelation {
            graph_id,
            from,
            to,
            selector,
        } => {
            with_graph(this, graph_id, cx, |g, gcx| {
                if g.retract_relations(from, to, selector) > 0 {
                    gcx.notify();
                }
            });
        }

        AppendTraversal {
            graph_id,
            from,
            to,
            trigger,
        } => {
            with_graph(this, graph_id, cx, |g, gcx| {
                if g.append_traversal(from, to, trigger, None) {
                    gcx.notify();
                }
            });
        }

        RemoveNode { graph_id, node } => {
            let removed = with_graph(this, graph_id, cx, |g, gcx| {
                let removed = g.remove_node(node);
                if removed {
                    gcx.notify();
                }
                removed
            })
            .unwrap_or(false);
            if removed {
                this.rebuild_app_tree(cx);
            }
        }

        HideRelation {
            graph_id,
            from,
            to,
            kind,
        } => {
            let ActionTarget::Pane(pane_id) = target else {
                tracing::debug!(?kind, "HideRelation: non-Pane target");
                return;
            };
            // Resolve the stable UUID key from the graph — orrery
            // hide state is keyed by UUID, not petgraph `NodeKey`.
            let key = this
                .registry
                .read(cx)
                .get(graph_id)
                .and_then(|graph_entity| {
                    let graph = graph_entity.read(cx);
                    let from_id = graph.get_node(from)?.id;
                    let to_id = graph.get_node(to)?.id;
                    Some((from_id, to_id, kind))
                });
            let Some(key) = key else {
                tracing::debug!(?from, ?to, "HideRelation: endpoint node missing");
                return;
            };
            if let Some(orrery) = this
                .panes
                .get_mut(&pane_id)
                .and_then(|s| s.as_orrery_mut())
            {
                // Toggle — a second invocation on the same relation
                // un-hides it.
                if !orrery.hidden_relations.remove(&key) {
                    orrery.hidden_relations.insert(key);
                }
                cx.notify();
            }
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
