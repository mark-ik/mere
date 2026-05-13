/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-host
//!
//! gpui host orchestrator for the Mere browser. Owns:
//! - Application bootstrap + window lifecycle
//! - Frame layout traversal
//! - Application uxtree composition (frame + per-domain subtrees)
//! - Demo fixtures (markdown, graph, frame layout) for the v0 demo
//!
//! Per-domain rendering lives in companion crates:
//! [`mere_host_workbench`], [`mere_host_orrery`], [`mere_host_gloss`],
//! [`mere_host_apparatus`]. Each takes its portable input and returns
//! a gpui element. Diagnostics (tracing capture) lives in
//! `mere-host-apparatus` since the apparatus pane is its consumer.
//!
//! ## Internal module layout
//!
//! - [`actions`] — gpui action types + default keymap + JSON loader
//! - [`loader`] — address → fetch → sniff → route → engine pipeline
//! - [`tiles`] — workbench-tile state (per-node document cache)
//! - [`demo`] — throwaway fixtures the v0 shell boots against
//! - [`persistence`] — frame-layout JSON load/save
//! - [`ticks`] — background tasks (apparatus refresh, orrery inertia)
//! - [`orrery_input`] — orrery pointer-event wiring → CanvasActions
//! - [`panes`] — pane-tree recursion + splitter + per-pane rendering
//! - [`host_helpers`] — graph find-or-create, tile labels, error doc

pub mod actions;
pub mod loader;

/// Re-export of [`mere_host_runtime::tiles`] so existing call sites
/// using `mere_host::tiles::TileManager` continue to resolve. The
/// type itself moved to `mere-host-runtime` as part of the
/// 2026-05-11 portability extraction.
pub use mere_host_runtime::tiles;

mod a11y;
mod bootstrap;
mod context_menu;
mod demo;
mod graph_registry;
mod graph_switcher;
mod host_action_bus;
mod host_helpers;
mod host_navigation;
mod layout_config;
mod orrery_input;
mod pane_state;
mod panes;
mod persistence;
mod session_persist;
mod tearout;
mod tearout_toast;
mod ticks;
mod tile_ops;

pub use bootstrap::run;

use std::collections::HashMap;

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, MouseButton, MouseMoveEvent, MouseUpEvent,
    Render, Window, div, prelude::*, rgb,
};
use inker::{EngineRegistry, EngineRoutePolicy};
use mere_frame::{FrameLayout, GraphId, PaneContent, PaneId, SplitAxis};
use mere_graphshell::frame_model::ToolbarViewModel;
use mere_host_apparatus::{CapturedEvent, EventBuffer};
use mere_host_graphshell::{OmnibarInput, Palette, ShellbarOrientation};
use mere_kernel::graph::Graph;
use uxtree::UxTree;

use crate::graph_registry::GraphRegistry;
use crate::layout_config::{ChromeLayoutConfig, EdgePosition};
use crate::panes::{PaneRenderCtx, SplitterDrag, compute_container_size, render_pane_node};
use crate::persistence::save_frame_layout;
use crate::tiles::TileManager;

pub(crate) struct HostRoot {
    /// Per-pane state, keyed by `PaneId`. Each leaf in `frame_layout`
    /// has an entry created on first paint. Closing a leaf drops the
    /// corresponding `PaneState`.
    pub(crate) panes: HashMap<PaneId, pane_state::PaneState>,
    /// The most recently focused workbench pane. omnibar submit /
    /// reload / back / forward target this workbench's tile list and
    /// graph. `None` when no workbench is open in this window.
    pub(crate) active_workbench: Option<PaneId>,
    /// First orrery pane in layout order. v0 single-orrery cut: all
    /// mouse handling routes here. Phase 1B Part 2 will key
    /// handlers by `pane_id` so multiple orreries in one window
    /// can each be interactive.
    pub(crate) primary_orrery_pane: Option<PaneId>,
    /// Monotonic counter for minting fresh `PaneId`s when summoning
    /// panels at runtime. Seeded high (1000) so it doesn't collide
    /// with the demo fixture's small PaneIds.
    pub(crate) next_pane_id: u64,
    /// This window's **primary graph** — the one new panels summoned
    /// from the shellbar default to. Held as a direct
    /// `Entity<Graph>` clone for fast-path reads of the active
    /// workbench's graph; `registry` is the source of truth across
    /// every window.
    pub(crate) graph: Entity<Graph>,
    /// Identity of the primary graph (key into `registry`).
    pub(crate) graph_id: GraphId,
    /// App-scope graph registry, shared by every window. Used to
    /// resolve `graph_id`s on leaves (Phase 1B Part 2) and to
    /// register new graphs created via `Cmd-N` or "summon orrery
    /// for new graph" gestures.
    pub(crate) registry: Entity<GraphRegistry>,
    /// App-scope durable session manifests. Shared by every window
    /// alongside `registry`. Session creation paths (Cmd-N, summon-
    /// orrery-for-new-graph, fork tear-out) write here; the store
    /// flushes manifests on mutation (auto-marked dirty) and at
    /// shutdown.
    pub(crate) manifests: Entity<mere_host_runtime::ManifestStore>,
    pub(crate) frame_layout: FrameLayout,
    /// Where the shellbar + workbench tile strip live. Persisted
    /// independently of `frame_layout` so the user can rotate the
    /// chrome without touching their split arrangement.
    pub(crate) chrome_layout: ChromeLayoutConfig,
    pub(crate) app_tree: UxTree,
    pub(crate) event_buffer: EventBuffer,
    pub(crate) toolbar: ToolbarViewModel,
    pub(crate) omnibar_input: Entity<OmnibarInput>,
    pub(crate) palette: Entity<Palette>,
    pub(crate) engine_registry: EngineRegistry,
    pub(crate) engine_policy: EngineRoutePolicy,
    // Note: app-wide history was removed 2026-05-11 with the
    // node-per-tile semantics shift. Back/forward/reload operate on
    // the *active tile's* within-tile history (`TileState.history`
    // in `mere-host-runtime`). Cross-tile navigation is the
    // tile-strip switcher.
    /// Top-level focus point so app-global keybindings dispatch even
    /// when no pane has focus.
    pub(crate) focus_handle: FocusHandle,
    /// Set by the palette-close subscription so the next `render`
    /// (which has a `Window` handle) can move focus back here.
    pub(crate) refocus_root_on_next_render: bool,
    /// Raw window handle captured on the first paint. Cheap +
    /// side-effect-free to read; the real platform a11y adapter
    /// lives inside the `install_a11y_push` spawn task and reads
    /// this field once on its first iteration. Keeping the adapter
    /// itself out of the entity is deliberate — accesskit calls
    /// pump platform messages and would re-enter gpui's wndproc.
    pub(crate) a11y_handle: Option<a11y::RawHandle>,
    pub(crate) dragging_splitter: Option<SplitterDrag>,
    /// Whether the graph-switcher dropdown is currently visible.
    /// Toggled by the "Graphs" button in the shellbar.
    pub(crate) graph_switcher_open: bool,
    /// Permission gate the bus checks every action against. v0:
    /// `PermitEverythingGate` — behaviour identical to pre-bus
    /// dispatch. The capability-gate catalogue plan plugs in a
    /// real `SessionPolicyGate` driven by `manifest.policy.overrides`.
    pub(crate) permission_gate: Box<dyn mere_host_runtime::PermissionGate>,
    /// Currently-open right-click context menu, if any. `Some` =
    /// visible at the recorded position with the recorded entries;
    /// `None` = hidden. Only one menu can be open per window — see
    /// [`crate::context_menu`].
    pub(crate) context_menu: Option<context_menu::ContextMenuState>,
    /// Tear-out promotion toast — shown on no-modifier tile drops
    /// so the user can promote the resulting leaf to a branch or
    /// fork. `Some` = visible; `None` = hidden. See
    /// [`crate::tearout_toast`].
    pub(crate) tearout_toast: Option<tearout_toast::TearoutToastState>,
}

impl HostRoot {
    /// Ensure every leaf in `frame_layout` has a corresponding entry
    /// in `panes` (lazy init). Drops entries whose `PaneId` no
    /// longer appears in the layout (a leaf was closed).
    ///
    /// Also nominates an `active_workbench` if none is set — the
    /// first workbench in layout order becomes default.
    pub(crate) fn reconcile_panes(&mut self) {
        let mut live_ids: std::collections::HashSet<PaneId> = std::collections::HashSet::new();
        let mut first_workbench: Option<PaneId> = None;
        let mut first_orrery: Option<PaneId> = None;
        for (pane_id, content, _graph_id) in self.frame_layout.iter_leaves() {
            live_ids.insert(pane_id);
            self.panes
                .entry(pane_id)
                .or_insert_with(|| pane_state::PaneState::for_content(content));
            if matches!(content, PaneContent::Workbench) && first_workbench.is_none() {
                first_workbench = Some(pane_id);
            }
            if matches!(content, PaneContent::Orrery) && first_orrery.is_none() {
                first_orrery = Some(pane_id);
            }
        }
        self.panes.retain(|id, _| live_ids.contains(id));
        if self
            .active_workbench
            .map(|id| !live_ids.contains(&id))
            .unwrap_or(false)
        {
            self.active_workbench = None;
        }
        if self.active_workbench.is_none() {
            self.active_workbench = first_workbench;
        }
        if self
            .primary_orrery_pane
            .map(|id| !live_ids.contains(&id))
            .unwrap_or(false)
        {
            self.primary_orrery_pane = None;
        }
        if self.primary_orrery_pane.is_none() {
            self.primary_orrery_pane = first_orrery;
        }
    }

    /// Borrow the active workbench's tile manager, if any.
    pub(crate) fn active_tiles(&self) -> Option<&TileManager> {
        let pane_id = self.active_workbench?;
        self.panes.get(&pane_id)?.as_workbench().map(|w| &w.tiles)
    }

    pub(crate) fn active_tiles_mut(&mut self) -> Option<&mut TileManager> {
        let pane_id = self.active_workbench?;
        self.panes
            .get_mut(&pane_id)?
            .as_workbench_mut()
            .map(|w| &mut w.tiles)
    }

    /// Borrow the primary orrery pane's state (camera/engine/bounds),
    /// if any. v0: this is the orrery handling all mouse events.
    pub(crate) fn primary_orrery(&self) -> Option<&pane_state::OrreryPaneState> {
        let pane_id = self.primary_orrery_pane?;
        self.panes.get(&pane_id)?.as_orrery()
    }

    pub(crate) fn primary_orrery_mut(
        &mut self,
    ) -> Option<&mut pane_state::OrreryPaneState> {
        let pane_id = self.primary_orrery_pane?;
        self.panes.get_mut(&pane_id)?.as_orrery_mut()
    }
}


impl Focusable for HostRoot {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for HostRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // First paint: claim focus so action handlers on the root
        // div are in the dispatch chain immediately. Subsequent
        // renders only re-focus if a subsystem (e.g. the palette
        // closing) explicitly asked for it.
        if !self.focus_handle.contains_focused(window, cx)
            && (self.refocus_root_on_next_render || window.focused(cx).is_none())
        {
            window.focus(&self.focus_handle, cx);
            self.refocus_root_on_next_render = false;
        }

        // Capture the raw window handle on first paint so the
        // a11y push tick (see `install_a11y_push`) can construct
        // its `Adapter` from a spawn-task body where no `App`
        // borrow is live. The capture itself is just reading
        // `raw_window_handle` — no side effects, safe in render.
        if self.a11y_handle.is_none() {
            self.a11y_handle = a11y::A11yAdapter::capture_handle(window);
        }

        let events: Vec<CapturedEvent> = self
            .event_buffer
            .lock()
            .map(|buf| buf.iter().cloned().collect())
            .unwrap_or_default();

        // Make sure every leaf in the (possibly just-mutated) frame
        // has a PaneState entry; drop entries for removed leaves.
        self.reconcile_panes();

        // Build live-graph snapshots indexed by GraphId. Each leaf
        // looks up its graph by id when rendering.
        let registry_ref = self.registry.read(cx);
        let graphs: HashMap<GraphId, &Graph> = registry_ref
            .iter()
            .map(|(id, entity)| (id, entity.read(cx)))
            .collect();

        let pctx = PaneRenderCtx {
            workbench_strip_position: self.chrome_layout.workbench_strip.0,
            app_tree: &self.app_tree,
            events: &events,
            frame_layout: &self.frame_layout,
            panes: &self.panes,
            graphs,
            active_workbench: self.active_workbench,
        };

        let pane_tree = render_pane_node(&self.frame_layout.root, Vec::new(), &pctx, cx);

        let on_mouse_move = cx.listener(
            |this, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>| {
                let Some(drag) = this.dragging_splitter.clone() else {
                    return;
                };
                let window_size = window.viewport_size();
                let container =
                    compute_container_size(&this.frame_layout, &drag.path, window_size);
                let (extent, delta_px) = match drag.axis {
                    SplitAxis::Horizontal => (container.width, ev.position.x - drag.start_cursor.x),
                    SplitAxis::Vertical => (container.height, ev.position.y - drag.start_cursor.y),
                };
                let extent_f = extent.as_f32();
                if extent_f <= 0.0 {
                    return;
                }
                let new_ratio =
                    (drag.start_ratio + (delta_px.as_f32() / extent_f)).clamp(0.05, 0.95);
                if this.frame_layout.set_split_ratio(&drag.path, new_ratio) {
                    // Rebuild app_tree from the mutated layout so the
                    // apparatus pane and any uxtree consumers see the
                    // new split ratio.
                    this.rebuild_app_tree(cx);
                    cx.notify();
                }
            },
        );
        let on_mouse_up = cx.listener(
            |this, _ev: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>| {
                if this.dragging_splitter.take().is_some() {
                    // Persist the new ratios so the layout survives
                    // across sessions. Save on mouse-up rather than
                    // on every move so we don't write per-pixel.
                    save_frame_layout(&this.frame_layout);
                    cx.notify();
                }
            },
        );

        // Keybinding listeners — every action routes through the
        // host-side bus (`host_action_bus::dispatch_kind`) which
        // checks the permission gate + emits a diagnostic. Two
        // actions (FocusOmnibar, OpenPalette) keep their direct
        // listener bodies because they need a `&mut Window` handle
        // that the bus's `Context`-only execute path doesn't have.
        let focus_omnibar = cx.listener(
            |this, _: &actions::FocusOmnibar, window: &mut Window, cx: &mut Context<Self>| {
                this.omnibar_input
                    .update(cx, |input, cx| input.focus_and_select_all(window, cx));
            },
        );
        let open_palette = cx.listener(
            |this, _: &actions::OpenPalette, window: &mut Window, cx: &mut Context<Self>| {
                this.palette
                    .update(cx, |palette, cx| palette.toggle(window, cx));
            },
        );
        let go_back = cx.listener(
            |this, _: &actions::GoBack, _window: &mut Window, cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(this, mere_host_runtime::ActionKind::GoBack, cx);
            },
        );
        let go_forward = cx.listener(
            |this, _: &actions::GoForward, _window: &mut Window, cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(this, mere_host_runtime::ActionKind::GoForward, cx);
            },
        );
        let reload = cx.listener(
            |this, _: &actions::Reload, _window: &mut Window, cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(this, mere_host_runtime::ActionKind::Reload, cx);
            },
        );
        let cycle_shellbar = cx.listener(
            |this,
             _: &actions::CycleShellbarPosition,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::CycleShellbarPosition,
                    cx,
                );
            },
        );
        let cycle_workbench_strip = cx.listener(
            |this,
             _: &actions::CycleWorkbenchStripPosition,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::CycleWorkbenchStripPosition,
                    cx,
                );
            },
        );
        let open_new_window = cx.listener(
            |this, _: &actions::OpenNewWindow, _window: &mut Window, cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::OpenNewWindow,
                    cx,
                );
            },
        );
        let toggle_workbench_action = cx.listener(
            |this,
             _: &actions::ToggleWorkbench,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::ToggleWorkbench,
                    cx,
                );
            },
        );
        let toggle_gloss_action = cx.listener(
            |this,
             _: &actions::ToggleGloss,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::ToggleGloss,
                    cx,
                );
            },
        );
        let toggle_apparatus_action = cx.listener(
            |this,
             _: &actions::ToggleApparatus,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::ToggleApparatus,
                    cx,
                );
            },
        );
        let summon_orrery_new_graph_action = cx.listener(
            |this,
             _: &actions::SummonOrreryForNewGraph,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::SummonOrreryForNewGraph,
                    cx,
                );
            },
        );
        let toggle_graph_switcher_action = cx.listener(
            |this,
             _: &actions::ToggleGraphSwitcher,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::ToggleGraphSwitcher,
                    cx,
                );
            },
        );
        let tearout_new_graph_min_action = cx.listener(
            |this,
             _: &actions::TearOutTileToNewGraphMinimized,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::TearOutTile {
                        mode: mere_host_runtime::TearOutMode::ForkMinimized,
                        tile_index: None,
                    },
                    cx,
                );
            },
        );
        let tearout_new_graph_vis_action = cx.listener(
            |this,
             _: &actions::TearOutTileToNewGraphVisible,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::TearOutTile {
                        mode: mere_host_runtime::TearOutMode::ForkVisible,
                        tile_index: None,
                    },
                    cx,
                );
            },
        );
        let tearout_sticky_note_action = cx.listener(
            |this,
             _: &actions::TearOutTileAsStickyNote,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::TearOutTile {
                        mode: mere_host_runtime::TearOutMode::Leaf,
                        tile_index: None,
                    },
                    cx,
                );
            },
        );

        let shellbar_pos = self.chrome_layout.shellbar;
        let orientation = if shellbar_pos.is_vertical() {
            ShellbarOrientation::Vertical
        } else {
            ShellbarOrientation::Horizontal
        };
        // Compute which panel kinds are currently in the layout so
        // the toggle buttons can render with an "active" highlight.
        let mut panel_state = mere_host_graphshell::panel_buttons::PanelButtonState::default();
        for (_, content, _) in self.frame_layout.iter_leaves() {
            match content {
                PaneContent::Workbench => panel_state.workbench_summoned = true,
                PaneContent::Gloss => panel_state.gloss_summoned = true,
                PaneContent::Apparatus => panel_state.apparatus_summoned = true,
                _ => {}
            }
        }
        let shellbar = mere_host_graphshell::render(
            &self.toolbar,
            None,
            self.omnibar_input.clone(),
            orientation,
            panel_state,
            self.graph_switcher_open,
            // Shellbar mouse handlers — bus-routed so the same
            // permission gate + diagnostics + (future) capability
            // catalogue apply uniformly with keybinding paths.
            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                host_action_bus::dispatch_kind(this, mere_host_runtime::ActionKind::GoBack, cx);
            }),
            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                host_action_bus::dispatch_kind(this, mere_host_runtime::ActionKind::GoForward, cx);
            }),
            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                host_action_bus::dispatch_kind(this, mere_host_runtime::ActionKind::Reload, cx);
            }),
            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::ToggleWorkbench,
                    cx,
                );
            }),
            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::ToggleGloss,
                    cx,
                );
            }),
            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::ToggleApparatus,
                    cx,
                );
            }),
            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::SummonOrreryForNewGraph,
                    cx,
                );
            }),
            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                host_action_bus::dispatch_kind(
                    this,
                    mere_host_runtime::ActionKind::ToggleGraphSwitcher,
                    cx,
                );
            }),
        );
        let body = div().flex().flex_1().overflow_hidden().child(pane_tree);

        // Outer flex direction + child order varies with the
        // shellbar's edge. Top/Bottom use a column; Left/Right use a
        // row. Top/Left put the shellbar first; Bottom/Right put it
        // last.
        let container_row = shellbar_pos.is_vertical();
        let shellbar_first = matches!(shellbar_pos, EdgePosition::Top | EdgePosition::Left);
        // Drop handler for tear-out drags. Phase 2 Part 2 v0
        // scaffold: drops fire a leaf tear-out for the payload's
        // exact pane + tile index. Modifier-keyed variants (Shift =
        // branch, Ctrl+Shift = fork) wire up once toast UI lands
        // (see 2026-05-11_tearout_operations_brief.md). Routed
        // through the bus so the active payload's pane_id is the
        // dispatch target (not whatever's "active" at drop time).
        let on_tile_drop = cx.listener(
            |this,
             payload: &crate::tearout::TileDragPayload,
             _window: &mut Window,
             cx: &mut Context<Self>| {
                tracing::info!(
                    pane_id = ?payload.pane_id,
                    tile_index = payload.tile_index,
                    "tile drag dropped \u{2014} firing leaf tear-out + promotion toast"
                );
                host_action_bus::dispatch(
                    this,
                    mere_host_runtime::BusAction::pane(
                        payload.pane_id,
                        mere_host_runtime::ActionKind::TearOutTile {
                            mode: mere_host_runtime::TearOutMode::Leaf,
                            // Tear out the *grabbed* tile (drag
                            // payload), not whatever's active at
                            // drop time.
                            tile_index: Some(payload.tile_index),
                        },
                    ),
                    cx,
                );
                // Show the promotion toast so the user can escalate
                // to branch / fork. Toast appears on the *donor*
                // window (this one) since the new leaf window
                // shares its session — promoting routes back
                // through the donor's HostRoot context anyway.
                this.show_tearout_toast(
                    crate::tearout_toast::TearoutToastState {
                        donor_pane: payload.pane_id,
                        tile_index: payload.tile_index,
                    },
                    cx,
                );
            },
        );

        let mut frame = div()
            .id("host-root")
            .relative()
            .flex()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xe0e0e0))
            .text_sm()
            .track_focus(&self.focus_handle)
            .on_drop::<crate::tearout::TileDragPayload>(on_tile_drop)
            .on_action(focus_omnibar)
            .on_action(open_palette)
            .on_action(go_back)
            .on_action(go_forward)
            .on_action(reload)
            .on_action(cycle_shellbar)
            .on_action(cycle_workbench_strip)
            .on_action(open_new_window)
            .on_action(toggle_workbench_action)
            .on_action(toggle_gloss_action)
            .on_action(toggle_apparatus_action)
            .on_action(summon_orrery_new_graph_action)
            .on_action(toggle_graph_switcher_action)
            .on_action(tearout_new_graph_min_action)
            .on_action(tearout_new_graph_vis_action)
            .on_action(tearout_sticky_note_action)
            .on_mouse_move(on_mouse_move)
            .on_mouse_up(MouseButton::Left, on_mouse_up);
        frame = if container_row {
            frame.flex_row()
        } else {
            frame.flex_col()
        };
        let with_chrome = if shellbar_first {
            frame.child(shellbar).child(body)
        } else {
            frame.child(body).child(shellbar)
        };
        // Palette renders as an absolute overlay on top of the
        // chrome + pane tree. It paints nothing when closed, so
        // unconditionally including it in the tree is fine.
        let with_palette = with_chrome.child(self.palette.clone());

        // Graph switcher overlay — rendered conditionally on top
        // of palette. Clicking outside closes it; clicking an
        // entry summons that graph's orrery.
        let with_switcher = if self.graph_switcher_open {
            with_palette.child(self.render_graph_switcher(cx))
        } else {
            with_palette
        };

        // Tear-out promotion toast — appears on no-modifier tile
        // drops so the user can promote leaf → branch / fork.
        // Rendered above switcher, below context menu.
        let with_toast = if let Some(toast) = self.render_tearout_toast_overlay(cx) {
            with_switcher.child(toast)
        } else {
            with_switcher
        };

        // Context menu — rendered last so it's the topmost
        // interactive overlay (right-click should win over anything
        // else open). `render_context_menu_overlay` returns `None`
        // when no menu is active.
        if let Some(menu) = self.render_context_menu_overlay(cx) {
            with_toast.child(menu)
        } else {
            with_toast
        }
    }
}

