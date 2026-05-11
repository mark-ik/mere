/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Application + window bootstrap.
//!
//! `run` initialises the gpui app, builds the app-scope graph
//! registry, binds the keymap, and opens the first window.
//! `open_host_window` is the per-window entry — it allocates fresh
//! per-window state (frame layout, omnibar, palette, history) and
//! resolves the primary graph from the registry. Multi-window =
//! repeated `open_host_window` calls with different `graph_id`s.

use std::collections::HashMap;

use gpui::{App, AppContext, Bounds, Entity, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use inker::EngineRoutePolicy;
use mere_frame::{FrameLayout, GraphId, PaneNode};
use mere_host_apparatus::EventBuffer;
use mere_host_graphshell::{OmnibarInput, OmnibarSubmitted, Palette, PaletteInvoke, omnibar_bindings, palette_bindings};
use mere_kernel::graph::Graph;

use crate::actions;
use crate::demo::{
    build_demo_application_tree, render_demo_document, render_demo_frame_layout_for,
    render_demo_graph_state, render_demo_toolbar,
};
use crate::graph_registry::GraphRegistry;
use crate::host_helpers::ensure_node_for_address;
use crate::layout_config::load_chrome_layout;
use crate::loader;
use crate::persistence::load_frame_layout;
use crate::ticks::{install_a11y_push, install_apparatus_auto_refresh, install_orrery_inertia_tick};
use crate::tiles::TileManager;
use crate::HostRoot;

/// Bootstrap a gpui application and open the demo window.
pub fn run() {
    let event_buffer = mere_host_apparatus::init_diagnostics();

    application().run(move |cx: &mut App| {
        // Register the app-wide keymap layers + Quit handler exactly
        // once for the process.
        cx.bind_keys(actions::default_bindings());
        cx.bind_keys(omnibar_bindings());
        cx.bind_keys(palette_bindings());
        match actions::load_user_keymap(cx) {
            Ok(user) if !user.is_empty() => cx.bind_keys(user),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "failed to load user keymap"),
        }
        cx.on_action(|_: &actions::Quit, cx| {
            tracing::info!("quit requested");
            cx.quit();
        });

        // The app-scope graph registry. Every window holds an
        // `Entity<GraphRegistry>` clone; `Cmd-N` (and future
        // multi-graph operations) extend the registry with new
        // `Entity<Graph>` instances keyed by fresh `GraphId`s.
        let registry = cx.new(|_| GraphRegistry::new());

        // Seed the first graph. Initial node = intro page so the
        // first window's workbench has a tile waiting.
        let initial_graph_id = GraphId::new();
        let initial_graph = cx.new(|_| {
            let mut g = render_demo_graph_state();
            ensure_node_for_address(&mut g, "mere://intro");
            g
        });
        registry.update(cx, |reg, _| {
            reg.insert(initial_graph_id, initial_graph);
        });

        open_host_window(cx, registry, initial_graph_id, event_buffer.clone());
    });
}

/// Open one host window referencing `graph_id` as its **primary**
/// graph. Fresh per-window state (frame layout, tile list, omnibar,
/// palette, history) is allocated; the graph itself lives in the
/// shared `GraphRegistry`.
pub(crate) fn open_host_window(
    cx: &mut App,
    registry: Entity<GraphRegistry>,
    graph_id: GraphId,
    event_buffer: EventBuffer,
) {
    let engine_registry = loader::default_registry();
    let engine_policy = EngineRoutePolicy::default();
    let frame_layout = load_frame_layout()
        .map(|mut layout| {
            stamp_graph_id(&mut layout, graph_id);
            layout
        })
        .unwrap_or_else(|| {
            tracing::debug!("no saved frame layout; using demo fixture");
            render_demo_frame_layout_for(graph_id)
        });
    let chrome_layout = load_chrome_layout();

    let graph = registry
        .read(cx)
        .get(graph_id)
        .cloned()
        .expect("primary graph_id must be registered before opening window");

    // Per-window startup: load the intro doc, find its node in the
    // primary graph (it was seeded at app start), open the tile.
    let intro_address = "mere://intro";
    let intro_doc = loader::load(&engine_registry, &engine_policy, intro_address)
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to load intro; falling back to demo doc");
            render_demo_document()
        });
    let intro_node = graph.update(cx, |g, _| ensure_node_for_address(g, intro_address));
    let mut tiles = TileManager::new();
    tiles.open_or_focus(intro_node, intro_doc);

    let app_tree = build_demo_application_tree(
        tiles.active_document(),
        graph.read(cx),
        &frame_layout,
    );

    open_window(
        tiles,
        graph,
        graph_id,
        registry,
        frame_layout,
        chrome_layout,
        app_tree,
        event_buffer,
        engine_registry,
        engine_policy,
        intro_address.to_string(),
        cx,
    );
}

/// Open a host window pre-seeded by a tear-out gesture. Skips the
/// usual intro-loading path: the new window opens directly on the
/// caller-supplied `tiles` and `frame_layout`, with the omnibar /
/// history seeded to `initial_address`.
///
/// Used by `tear_out_tile_to_new_graph` and
/// `tear_out_tile_sticky_note` — the graph entity has already been
/// resolved (either freshly minted or cloned from the donor) and the
/// active tile's document is being moved/copied across the window
/// boundary, so the new window doesn't need to re-fetch anything.
#[allow(clippy::too_many_arguments)]
pub(crate) fn open_tearout_window(
    cx: &mut App,
    registry: Entity<GraphRegistry>,
    graph_id: GraphId,
    graph: Entity<Graph>,
    frame_layout: FrameLayout,
    tiles: TileManager,
    initial_address: String,
    event_buffer: EventBuffer,
) {
    let engine_registry = loader::default_registry();
    let engine_policy = EngineRoutePolicy::default();
    let chrome_layout = load_chrome_layout();
    let app_tree = build_demo_application_tree(
        tiles.active_document(),
        graph.read(cx),
        &frame_layout,
    );
    open_window(
        tiles,
        graph,
        graph_id,
        registry,
        frame_layout,
        chrome_layout,
        app_tree,
        event_buffer,
        engine_registry,
        engine_policy,
        initial_address,
        cx,
    );
}

/// Walk a `FrameLayout` and overwrite every leaf's `graph_id` with
/// the supplied value. Used when loading a saved layout into a
/// freshly-allocated graph (Phase 1: every leaf is the window's
/// primary graph; saved pre-graph_id layouts deserialize as nil
/// and get stamped here).
pub(crate) fn stamp_graph_id(layout: &mut FrameLayout, graph_id: GraphId) {
    fn walk(node: &mut PaneNode, graph_id: GraphId) {
        match node {
            PaneNode::Leaf { graph_id: g, .. } => *g = graph_id,
            PaneNode::Split { first, second, .. } => {
                walk(first, graph_id);
                walk(second, graph_id);
            }
        }
    }
    walk(&mut layout.root, graph_id);
}

#[allow(clippy::too_many_arguments)]
fn open_window(
    tiles: TileManager,
    graph: Entity<Graph>,
    graph_id: GraphId,
    registry: Entity<GraphRegistry>,
    frame_layout: FrameLayout,
    chrome_layout: crate::layout_config::ChromeLayoutConfig,
    app_tree: uxtree::UxTree,
    event_buffer: EventBuffer,
    engine_registry: inker::EngineRegistry,
    engine_policy: EngineRoutePolicy,
    initial_address: String,
    cx: &mut App,
) {
    let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        |_, cx| {
            cx.new(|cx| {
                install_apparatus_auto_refresh(cx);
                install_orrery_inertia_tick(cx);
                install_a11y_push(cx);

                let mut toolbar = render_demo_toolbar();
                toolbar.location = initial_address.clone();
                let omnibar_seed = initial_address.clone();
                let omnibar_input = cx.new(|cx| {
                    let mut input = OmnibarInput::new("Search or enter address…", cx);
                    input.set_content(omnibar_seed, cx);
                    input
                });
                let palette = cx.new(Palette::new);

                // PaletteInvoke → materialize action by name +
                // dispatch via App::dispatch_action.
                cx.subscribe(
                    &palette,
                    |_host: &mut HostRoot, _palette, ev: &PaletteInvoke, cx| {
                        match cx.build_action(&ev.action_name, None) {
                            Ok(action) => {
                                tracing::debug!(
                                    action = %ev.action_name,
                                    "palette dispatching action"
                                );
                                cx.dispatch_action(action.as_ref());
                            }
                            Err(e) => tracing::warn!(
                                action = %ev.action_name,
                                error = %e,
                                "palette could not build action"
                            ),
                        }
                    },
                )
                .detach();
                cx.subscribe(
                    &omnibar_input,
                    |host: &mut HostRoot, _input, ev: &OmnibarSubmitted, cx| {
                        host.navigate_to(ev.text.clone(), true, cx);
                    },
                )
                .detach();
                cx.subscribe(
                    &omnibar_input,
                    |host: &mut HostRoot,
                     input,
                     _ev: &mere_host_graphshell::OmnibarCancelled,
                     cx| {
                        let current = host.toolbar.location.clone();
                        input.update(cx, |input, cx| input.set_content(current, cx));
                        tracing::debug!("omnibar cancelled");
                    },
                )
                .detach();
                cx.subscribe(
                    &palette,
                    |host: &mut HostRoot,
                     _palette,
                     _ev: &mere_host_graphshell::PaletteClosed,
                     cx| {
                        host.refocus_root_on_next_render = true;
                        cx.notify();
                    },
                )
                .detach();
                // Re-render this window whenever the shared graph
                // mutates — a node added or moved in window A
                // bubbles back here so window B's orrery sees the
                // change live.
                cx.observe(&graph, |_host: &mut HostRoot, _graph, cx| cx.notify())
                    .detach();

                let mut host = HostRoot {
                    panes: HashMap::new(),
                    active_workbench: None,
                    primary_orrery_pane: None,
                    next_pane_id: 1000,
                    graph,
                    graph_id,
                    registry,
                    frame_layout,
                    chrome_layout,
                    app_tree,
                    event_buffer,
                    toolbar,
                    omnibar_input,
                    palette,
                    engine_registry,
                    engine_policy,
                    history: vec![initial_address.clone()],
                    history_cursor: 0,
                    focus_handle: cx.focus_handle(),
                    refocus_root_on_next_render: false,
                    a11y_handle: None,
                    dragging_splitter: None,
                    graph_switcher_open: false,
                };
                host.reconcile_panes();
                if let Some(active_tiles) = host.active_tiles_mut() {
                    *active_tiles = tiles;
                }
                host
            })
        },
    )
    .expect("open_window failed");
    cx.activate(true);
}
