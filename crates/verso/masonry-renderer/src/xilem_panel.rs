// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `XilemPanel` — a masonry tile driven by a xilem `View<State, (), ViewCtx>`.
//!
//! This is the reactive half of Mere's panel story. A plain
//! [`crate::MasonryTile`] builds its widget tree once from a
//! `NewWidget` and never reshapes it. `XilemPanel` instead holds a
//! xilem [`MasonryRoot<State>`] view plus the app `State` and a logic
//! function; each [`tick`](PanelTile::tick) re-runs the logic against
//! current state and rebuilds the masonry widget tree in place through
//! `RenderRoot::edit_base_layer` (the same seam xilem's own driver
//! uses).
//!
//! v0 scope: **read-only reactive** — state changes flow in via
//! [`update`](Self::update), the logic re-runs, the tree rebuilds, the
//! tile repaints. Action routing (widget → `View::message` → state
//! mutation) is deliberately deferred; `TileSignal::Action` already
//! carries `(WidgetId, ErasedAction)`, so the message cycle plugs in
//! later without changing this type's shape.
//!
//! Driving xilem manually (Mere owns winit, not xilem) is directly
//! supported: [`xilem_masonry::ViewCtx::new`] is public and documented
//! for "building your own framework". It needs a `RawProxy` (a no-op
//! stub here — no async/task views in v0) and a tokio runtime (shared
//! from the host).

use std::sync::Arc;

use accesskit::TreeUpdate;
use masonry_core::core::{DefaultProperties, PointerEvent, TextEvent};
use xilem_masonry::core::{ProxyError, RawProxy, SendMessage, View, ViewId};
use xilem_masonry::{AnyWidgetView, InitialRootWidget, MasonryRoot, ViewCtx};

use crate::panel_tile::PanelTile;
use crate::signal::TileSignal;
use crate::tile::{MasonryTile, TileSize};

/// Logic function: maps current state to the panel's view tree. Re-run
/// on every [`tick`](PanelTile::tick) / [`update`](XilemPanel::update).
/// Returns a boxed [`AnyWidgetView`] so heterogeneous view trees share
/// one stored type.
pub type PanelLogic<State> = Box<dyn FnMut(&mut State) -> Box<AnyWidgetView<State, ()>>>;

/// No-op message proxy. xilem's `ViewCtx` requires a `RawProxy` for
/// async message routing; v0 panels are synchronous and read-only, so
/// nothing is ever sent through it. If async/task views land, this is
/// replaced by a real proxy wired to the host's redraw signal.
#[derive(Debug)]
struct NoopProxy;

impl RawProxy for NoopProxy {
    fn send_message(&self, _path: Arc<[ViewId]>, _message: SendMessage) -> Result<(), ProxyError> {
        // No async views in v0 — drop silently.
        Ok(())
    }

    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

type RootViewState<State> = <MasonryRoot<State> as View<State, (), ViewCtx>>::ViewState;

/// A reactive masonry panel: a [`MasonryTile`] whose widget tree is
/// (re)built from a xilem view over `State`.
pub struct XilemPanel<State: 'static> {
    tile: MasonryTile,
    state: State,
    logic: PanelLogic<State>,
    view: MasonryRoot<State>,
    view_state: RootViewState<State>,
    view_ctx: ViewCtx,
}

impl<State: 'static> XilemPanel<State> {
    /// Build a reactive panel. Runs `logic(&mut state)` once to build
    /// the initial view tree, hands the resulting widget to a fresh
    /// [`MasonryTile`], and keeps the view machinery for later
    /// rebuilds.
    pub fn new(
        mut state: State,
        mut logic: PanelLogic<State>,
        runtime: Arc<tokio::runtime::Runtime>,
        default_properties: Arc<DefaultProperties>,
        size: TileSize,
        scale_factor: f64,
    ) -> Self {
        let proxy: Arc<dyn RawProxy> = Arc::new(NoopProxy);
        let mut view_ctx = ViewCtx::new(proxy, runtime);
        let view = MasonryRoot::new(logic(&mut state));
        let (InitialRootWidget(pod), view_state) = view.build(&mut view_ctx, &mut state);
        let tile = MasonryTile::new(
            default_properties,
            pod.new_widget.erased(),
            size,
            scale_factor,
        );
        Self {
            tile,
            state,
            logic,
            view,
            view_state,
            view_ctx,
        }
    }

    /// Mutate the panel's state, then rebuild the widget tree to match.
    /// The host calls this to push fresh data into the panel (e.g.
    /// per-frame diagnostics).
    pub fn update(&mut self, f: impl FnOnce(&mut State)) {
        f(&mut self.state);
        self.rebuild_from_logic();
    }

    /// Borrow the panel's current state.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Re-run the logic against current state and rebuild the masonry
    /// widget tree in place.
    fn rebuild_from_logic(&mut self) {
        let Self {
            tile,
            state,
            logic,
            view,
            view_state,
            view_ctx,
        } = self;
        let next = MasonryRoot::new(logic(state));
        tile.edit_render_root(|render_root| {
            next.rebuild(&*view, view_state, view_ctx, render_root, state);
        });
        *view = next;
    }
}

impl<State: 'static> PanelTile for XilemPanel<State> {
    fn tick(&mut self) {
        // Read-only reactive: re-run logic against current state so any
        // host-pushed state change since the last frame is reflected.
        self.rebuild_from_logic();
    }

    fn render_layers(&mut self) {
        self.tile.redraw_layers();
    }

    fn pending_layers(&self) -> Option<&masonry_core::app::VisualLayerPlan> {
        self.tile.pending_layers()
    }

    fn take_accesskit_update(&mut self) -> Option<TreeUpdate> {
        self.tile.take_accesskit_update()
    }

    fn handle_pointer(&mut self, event: PointerEvent) {
        self.tile.handle_pointer(event);
    }

    fn handle_text(&mut self, event: TextEvent) {
        self.tile.handle_text(event);
    }

    fn resize(&mut self, size: TileSize) {
        self.tile.resize(size);
    }

    fn drain_signals(&mut self) -> Vec<TileSignal> {
        self.tile.drain_signals()
    }
}
