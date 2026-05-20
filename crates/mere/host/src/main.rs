// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `host` — the Mere browser host (xilem stack: winit + wgpu +
//! vello, with masonry panel rendering through the substrate's
//! renderer registry).
//!
//! Replaces the prior gpui host (deleted 2026-05-18 on the
//! "xilem is the host" pivot). Modular by design: each module here
//! is a sibling concern composed at the top level, not absorbed
//! into a god-struct.
//!
//! Module layout:
//!
//! - [`cartography_projection`] — orchestration that runs
//!   [`cartography::LayoutStrategy`] against each orrery pane and
//!   writes results into the orrery renderer's snapshot map.
//! - [`graph_registry`] — app-scope `GraphId → Graph` map. Each
//!   leaf in a [`frame::FrameLayout`] carries a `graph_id`
//!   resolved here at projection time.
//! - [`orrery_renderer`] — substrate `InScenePaintRenderer` for
//!   `NodeContentKind::GraphView` panes (Path A); reads a host-
//!   installed `Projection` per-identity from a shared snapshot map.
//! - [`graph_node_renderer`] — substrate `InScenePaintRenderer` for
//!   `NodeContentKind::GraphNode` (Path B); paints one circle per
//!   exploded graph node.
//! - [`graph_node_explode`] — turns a projection into per-node
//!   `GraphNode` substrate entities + relation edges, with stable
//!   identity across resyncs.
//! - [`diagnostics_panel`] — the first reactive xilem-driven panel:
//!   a `View<DiagnosticsState, (), ViewCtx>` showing live host metrics,
//!   rebuilt each frame from a shared snapshot the host publishes.
//! - [`splitter_renderer`] — substrate renderer for the 4px
//!   draggable chrome between sibling panes.
//! - [`render`] — per-frame paint path (substrate dispatch → vello
//!   rasterise → blit).
//! - [`runtime`] — `App` (winit `ApplicationHandler`) +
//!   `RuntimeState` + event handlers.
//! - [`seed`] — initial frametree / graph / masonry factory.
//! - [`setup`] — boots the window + wgpu + vello stack and assembles
//!   the initial `RuntimeState`.
//! - [`view_preset`] — named view-intent presets (Orrery / Drift /
//!   Minimap); maps a pane's role to a [`cartography::ViewIntent`]
//!   shape so the projection pass picks the right form factor per
//!   pane.
//! - [`strategy_registry`] — `projection_id` → boxed
//!   [`cartography::LayoutStrategy`]; each preset resolves to a
//!   registered strategy here. Third-party / WASM strategies
//!   eventually plug in through this surface.

mod cartography_projection;
mod diagnostics_panel;
mod graph_node_explode;
mod graph_node_renderer;
mod graph_registry;
mod orrery_renderer;
mod render;
mod runtime;
mod seed;
mod setup;
mod splitter_renderer;
mod strategy_registry;
mod view_preset;

use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    // Event-driven render: park until an event arrives. Each input
    // handler that changes visible state requests a redraw, so an idle
    // host issues no frames and doesn't peg a core. When an animation
    // lane (e.g. streaming layout) lands, it re-arms the redraw each
    // active frame.
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = runtime::App::default();
    event_loop.run_app(&mut app).expect("run_app");
}
