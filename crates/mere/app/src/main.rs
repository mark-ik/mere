// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Mere — idiomatic Xilem app (2026-05-21 re-scaffold).
//!
//! Per `design_docs/mere_docs/technical_architecture/2026-05-21_app_architecture_rescaffold.md`:
//! the chrome (frametree splits, workbench, panels) is a Xilem view
//! tree over **one** [`AppState`]; the graph canvas (orrery) will be a
//! custom Masonry widget. No substrate-as-host, no renderer registry,
//! no action bus — Xilem's `View<State>` + state mutation is the whole
//! app-coordination layer (Woodshed's proven shape).
//!
//! The chrome shape: a toolbar (back / forward / omnibar) over a frametree of
//! draggable `split` views — a Workbench pane (forme → tree projection of tiles
//! that render their bound graph member's content), an Orrery pane (the spatial
//! graph view; a custom Masonry widget, see [`graph_canvas`]), and an Apparatus
//! pane (diagnostics). The graph + workbench forme persist across restarts.
//! Navigating the omnibar or clicking an orrery node routes through
//! [`navigation`] → `inker` and renders in the focus tile.

mod camera;
mod engine_tile;
mod graph_canvas;
mod navigation;
mod panes;
mod surface_tile;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use engine_tile::RenderedTile;
use forme::store::{load_all_formes, save_forme};
use forme::{ArrangementNodeKind, FormeDocument};
use camera::GraphScene;
use graph_canvas::scene_from_graph;
use surface_tile::{SurfaceContent, SurfaceRegistry};
use kernel::geometry::PortablePoint;
use kernel::graph::{Graph, NavigationTrigger, Traversal};
#[cfg(test)]
use platen::project_tree;
use xilem::masonry::kurbo::Size;
use xilem::{EventLoop, WindowOptions, Xilem};

/// Lay seed nodes (and new ones) on a world-space ring so the orrery shows a
/// spread graph. The kernel seeds all nodes at the origin; the orrery reads
/// projected positions, so we assign them here.
fn ring_world(i: usize, n: usize) -> PortablePoint {
    let theta = std::f64::consts::TAU * (i as f64) / (n.max(1) as f64) - std::f64::consts::FRAC_PI_2;
    let radius = 140.0;
    PortablePoint::new((radius * theta.cos()) as f32, (radius * theta.sin()) as f32)
}

/// Back/forward navigation history: a list of addresses + a cursor. Pure (no
/// rendering or I/O) so it's unit-testable on its own.
struct History {
    entries: Vec<String>,
    pos: usize,
}

impl History {
    fn new(initial: String) -> Self {
        Self {
            entries: vec![initial],
            pos: 0,
        }
    }

    /// Navigate to `address`: drop any forward entries, then push (unless it
    /// equals the current tip).
    fn push(&mut self, address: &str) {
        self.entries.truncate(self.pos + 1);
        if self.entries.last().map(String::as_str) != Some(address) {
            self.entries.push(address.to_string());
            self.pos = self.entries.len() - 1;
        }
    }

    /// Step back, returning the now-current address (or `None` at the start).
    fn back(&mut self) -> Option<&str> {
        if self.pos > 0 {
            self.pos -= 1;
            Some(&self.entries[self.pos])
        } else {
            None
        }
    }

    /// Step forward, returning the now-current address (or `None` at the end).
    fn forward(&mut self) -> Option<&str> {
        if self.pos + 1 < self.entries.len() {
            self.pos += 1;
            Some(&self.entries[self.pos])
        } else {
            None
        }
    }
}

/// What the single main pane shows. The app opens on `Orrery` (the graph is
/// home); navigating switches it to `Document`; the toolbar switches it to
/// `Workbench` / `Apparatus`. Side-by-side splitting is a separate gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainView {
    Orrery,
    Document,
    Workbench,
    Apparatus,
    /// A single external-surface tile (the verso P3 stub: an engine surface
    /// composited as an external GPU layer, not Masonry paint).
    Surface,
}

/// The single application state the Xilem driver owns. Widgets mutate
/// it in place through their callbacks; the view tree rebuilds on diff.
/// Per-pane UI sub-state gets its own struct here as panes grow
/// (Woodshed's pattern); for the skeleton it's just the graph + a
/// frame label.
struct AppState {
    /// Graph truth — the orrery projects this. Foundational `kernel`
    /// crate, framework-free.
    graph: Graph,
    /// The Workbench pane's forme — a durable, persisted [`FormeDocument`].
    /// Its `arrangement` is rendered via `platen::project_tree`; the document
    /// is loaded at startup and re-saved on every edit, so the workbench
    /// survives a restart.
    workbench: FormeDocument,
    /// The address currently in the omnibar (edited as the user types).
    omnibar: String,
    /// Back/forward navigation history.
    history: History,
    /// The currently-navigated document: resolved by [`navigation`] and
    /// rendered through `inker`. Replaced on every navigation.
    current: RenderedTile,
    /// The orrery's selected node (highlighted; clicking it opened `current`).
    selected_node: Option<uuid::Uuid>,
    /// What the main pane currently shows. Opens on `Orrery`.
    main_view: MainView,
    /// Rendered content for workbench tiles bound to a graph member, keyed by
    /// member id. Built once at startup (tile content is static for v1).
    tile_docs: HashMap<uuid::Uuid, RenderedTile>,
    /// Cached orrery scene (graph projected to world positions + edges).
    /// Rebuilt only on graph mutation, not per frame (memoized derivation).
    scene: GraphScene,
    /// External-surface content registry: maps each `SurfaceTileWidget`'s id to
    /// its content, read by the `with_external_compositor` hook. Cloned into the
    /// compositor closure in `main`.
    surface_registry: SurfaceRegistry,
    /// Shared web-tile channel (frame / bounds / input), cloned to the surf
    /// tile's widget so it can queue pointer events for the on_tick host.
    surface_channel: surface_tile::SurfaceChannel,
    /// Last measured size of the workbench tiling region, reported back by a
    /// `resize_observer`. `platen::layout_plan` needs a viewport to lay tiles
    /// out, but Xilem builds the view before layout runs — so the size round-
    /// trips through state (one-frame lag on resize, then stable).
    content_size: Size,
    /// Where session state lives on disk (the forme store writes under
    /// `<session_dir>/formes/`).
    session_dir: PathBuf,
    /// Human label for the current frame/workspace (placeholder for the
    /// real `FrameLayout` once the canvas + multi-pane interaction land).
    frame_label: String,
}

impl AppState {
    fn new(
        surface_registry: SurfaceRegistry,
        surface_channel: surface_tile::SurfaceChannel,
    ) -> Self {
        let session_dir = session_dir();
        let _ = std::fs::create_dir_all(&session_dir);

        // Restore the graph from disk, or seed + persist a fresh one. Node ids
        // survive the round-trip, so workbench member bindings stay valid.
        let graph = load_or_seed_graph(&session_dir);

        // Restore the workbench forme from disk, or seed + persist a fresh one.
        let mut workbench = load_or_seed_workbench(&session_dir);
        // Bind unbound workbench tiles to graph members (by order) so they
        // render real content, then persist. Durable cross-restart binding
        // arrives with graph persistence; until then we re-bind each run.
        bind_unbound_tiles(&mut workbench, &graph);
        if let Err(e) = save_forme(&session_dir, &mut workbench, now_ms()) {
            eprintln!("mere: failed to persist bound workbench: {e}");
        }
        let tile_docs = render_workbench_tiles(&workbench, &graph);

        // Navigate to the welcome page at startup through the engine seam.
        let omnibar = "mere://welcome".to_string();
        let current = navigation::open(&omnibar);

        let scene = scene_from_graph(&graph);

        Self {
            graph,
            workbench,
            history: History::new(omnibar.clone()),
            omnibar,
            current,
            selected_node: None,
            main_view: MainView::Orrery,
            tile_docs,
            scene,
            surface_registry,
            surface_channel,
            content_size: Size::new(960.0, 640.0),
            session_dir,
            frame_label: "Mere".to_string(),
        }
    }

    fn node_count(&self) -> usize {
        self.graph.nodes().count()
    }

    /// Recompute the cached orrery scene from graph truth. Call after any graph
    /// mutation; the orrery view reads the cache rather than walking the graph
    /// every frame.
    fn rebuild_scene(&mut self) {
        self.scene = scene_from_graph(&self.graph);
    }

    /// Navigate to `address`: push history, resolve + render it, sync omnibar,
    /// and switch the main pane to show the document.
    fn navigate(&mut self, address: &str) {
        self.history.push(address);
        self.current = navigation::open(address);
        self.omnibar = address.to_string();
        self.main_view = MainView::Document;
    }

    /// Go back in history and render that entry (no-op at the start).
    fn back(&mut self) {
        if let Some(address) = self.history.back().map(str::to_string) {
            self.current = navigation::open(&address);
            self.omnibar = address;
            self.main_view = MainView::Document;
        }
    }

    /// Go forward in history and render that entry (no-op at the end).
    fn forward(&mut self) {
        if let Some(address) = self.history.forward().map(str::to_string) {
            self.current = navigation::open(&address);
            self.omnibar = address;
            self.main_view = MainView::Document;
        }
    }

    /// Persist the workbench forme after an edit. Failures are surfaced to
    /// stderr, not fatal — a transient write error shouldn't crash the app.
    fn persist_workbench(&mut self) {
        if let Err(e) = save_forme(&self.session_dir, &mut self.workbench, now_ms()) {
            eprintln!("mere: failed to persist workbench forme: {e}");
        }
    }

    /// Persist the graph after a structural/position change (node added or
    /// dropped). Not called per drag-move tick — only on discrete edits.
    fn persist_graph(&mut self) {
        if let Err(e) = kernel::store::save_graph(&self.session_dir, &self.graph) {
            eprintln!("mere: failed to persist graph: {e}");
        }
    }
}

/// Where this host persists session state: a local `mere-sessions/default`
/// directory (gitignored), relative to the run cwd — matches the prior host
/// convention. A per-user data dir is a later refinement.
fn session_dir() -> PathBuf {
    PathBuf::from("mere-sessions").join("default")
}

/// Unix-epoch milliseconds for the forme store's timestamp stamping.
fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Restore the graph from `<session_dir>/graph.json`, or seed + persist a fresh
/// one. A load error falls back to an unsaved in-memory seed.
fn load_or_seed_graph(session_dir: &Path) -> Graph {
    match kernel::store::load_graph(session_dir) {
        Ok(Some(graph)) => graph,
        Ok(None) => {
            let graph = seed_graph();
            if let Err(e) = kernel::store::save_graph(session_dir, &graph) {
                eprintln!("mere: failed to write seed graph: {e}");
            }
            graph
        }
        Err(e) => {
            eprintln!("mere: failed to load graph ({e}); seeding fresh in-memory");
            seed_graph()
        }
    }
}

/// A fresh seed graph: 6 `mere://node/N` nodes on a world-space ring, with a
/// few traversal relations so the orrery shows a connected graph. Positions are
/// committed (durable) so they survive a restart.
fn seed_graph() -> Graph {
    let mut graph = Graph::new();
    let keys: Vec<_> = (0..6)
        .map(|i| graph.add_node(format!("mere://node/{i}"), PortablePoint::new(0.0, 0.0)))
        .collect();
    for &(a, b) in &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (0, 3), (0, 5)] {
        graph.push_traversal(keys[a], keys[b], Traversal::now(NavigationTrigger::LinkClick));
    }
    for (i, &k) in keys.iter().enumerate() {
        graph.set_node_position(k, ring_world(i, keys.len()));
    }
    graph
}

/// Load the workbench forme (the first stored forme) or seed + persist a fresh
/// one. A load error falls back to an unsaved in-memory seed rather than
/// crashing.
fn load_or_seed_workbench(session_dir: &Path) -> FormeDocument {
    match load_all_formes(session_dir) {
        Ok(mut formes) if !formes.is_empty() => formes.remove(0),
        Ok(_) => {
            let mut doc = seed_workbench();
            if let Err(e) = save_forme(session_dir, &mut doc, now_ms()) {
                eprintln!("mere: failed to write seed workbench forme: {e}");
            }
            doc
        }
        Err(e) => {
            eprintln!("mere: failed to load formes ({e}); using a fresh in-memory workbench");
            seed_workbench()
        }
    }
}

/// A fresh workbench forme: one solo tile + two stacked (tabs) — exercises the
/// projection's split and tab-stack paths. The `graph_id` is freshly minted
/// (the in-memory graph isn't persisted yet; forme persistence is independent
/// for v1, and `TileIntent`s are unbound).
fn seed_workbench() -> FormeDocument {
    let mut doc = FormeDocument::new(uuid::Uuid::new_v4(), Some("Workbench".into()));
    let arr = &mut doc.arrangement;
    let root = arr.root();
    let solo = arr.insert(
        ArrangementNodeKind::TileIntent { member: None },
        Some("a.example".into()),
    );
    arr.attach(solo, root);
    let t2 = arr.insert(
        ArrangementNodeKind::TileIntent { member: None },
        Some("b.example".into()),
    );
    arr.attach(t2, root);
    let t3 = arr.insert(
        ArrangementNodeKind::TileIntent { member: None },
        Some("c.example".into()),
    );
    arr.attach(t3, root);
    arr.stack(t2, t3);
    doc
}

/// Bind unbound root workbench tiles to graph members, in order, so each tile
/// shows real content. Already-bound tiles are left alone. v1 re-binds to the
/// current graph each run (graph identity isn't persistent yet — slice 4).
fn bind_unbound_tiles(workbench: &mut FormeDocument, graph: &Graph) {
    let root = workbench.arrangement.root();
    let tile_ids: Vec<_> = workbench.arrangement.members_of(root).collect();
    let member_ids: Vec<uuid::Uuid> = graph.nodes().map(|(_, n)| n.id).collect();
    let mut next = 0;
    for tid in tile_ids {
        let unbound = matches!(
            workbench.arrangement.node(tid).map(|n| &n.kind),
            Some(ArrangementNodeKind::TileIntent { member: None })
        );
        if unbound {
            if let Some(&member) = member_ids.get(next) {
                workbench.arrangement.set_tile_member(tid, Some(member));
                next += 1;
            }
        }
    }
}

/// Render content for each bound workbench tile, keyed by member id, by
/// resolving member → graph node → address → engine document.
fn render_workbench_tiles(
    workbench: &FormeDocument,
    graph: &Graph,
) -> HashMap<uuid::Uuid, RenderedTile> {
    let mut docs = HashMap::new();
    for member in workbench.arrangement.referenced_members() {
        if let Some((_, node)) = graph.get_node_by_id(member) {
            let url = node.url().to_string();
            docs.insert(member, navigation::open(&url));
        }
    }
    docs
}

fn main() {
    // A scrying WebView frame is a D3D12 shared texture; importing it into our
    // wgpu device requires the DX12 backend (the importer opens the handle via
    // `as_hal::<Dx12>()`). Default to DX12 on Windows — vello renders fine on it
    // — unless the user explicitly chose a backend.
    #[cfg(target_os = "windows")]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        // SAFETY: set once at startup, before any thread reads the environment
        // (wgpu reads `WGPU_BACKEND` when the first window creates its device).
        unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
    }

    // The external-surface registry is shared between the SurfaceTile widgets
    // (which register their content), the compositor hook (which fills each
    // layer), and the on_tick host (which drives web producers). All clones of
    // one `Arc`.
    let surface_registry = SurfaceRegistry::new();
    // Web-tile channel (frame / bounds / input), shared widget ↔ compositor ↔
    // on_tick host.
    let surface_channel: surface_tile::SurfaceChannel =
        std::sync::Arc::new(std::sync::Mutex::new(Default::default()));
    let state = AppState::new(surface_registry.clone(), surface_channel.clone());
    let webview_data_dir = session_dir().join("webview");

    Xilem::new_simple(state, panes::app_logic, WindowOptions::new("Mere"))
        // Realize each external layer (a SurfaceTile's reserved rect): a solid
        // color, or — for a web tile — the imported WebView frame (placeholder
        // until the first frame lands). Also records the tile's bounds so the
        // on_tick host can size the producer to match.
        .with_external_compositor({
            let registry = surface_registry.clone();
            let channel = surface_channel.clone();
            // The blit pipeline is created lazily on the first web frame (needs
            // the device + target format) and cached across frames.
            let mut blit: Option<surface_tile::BlitPipeline> = None;
            move |ctx| {
                for layer in ctx.layers {
                    match registry.content(layer.widget_id) {
                        Some(SurfaceContent::Solid(color)) => {
                            surface_tile::composite_color(
                                ctx.device,
                                ctx.queue,
                                ctx.target_texture,
                                layer.bounds,
                                color,
                            );
                        }
                        Some(SurfaceContent::Web { .. }) => {
                            // Record the tile bounds (host sizes the producer to
                            // them) and read the latest imported frame.
                            let frame = match channel.lock() {
                                Ok(mut c) => {
                                    c.bounds = Some(layer.bounds);
                                    c.frame.clone()
                                }
                                Err(_) => None,
                            };
                            match frame {
                                Some(texture) => {
                                    let pipeline = blit.get_or_insert_with(|| {
                                        surface_tile::BlitPipeline::new(
                                            ctx.device,
                                            ctx.target_texture.format(),
                                        )
                                    });
                                    pipeline.blit(
                                        ctx.device,
                                        ctx.queue,
                                        ctx.target_view,
                                        &texture,
                                        layer.bounds,
                                    );
                                }
                                // No frame imported yet: dark-teal placeholder.
                                None => surface_tile::composite_color(
                                    ctx.device,
                                    ctx.queue,
                                    ctx.target_texture,
                                    layer.bounds,
                                    [28, 56, 48, 255],
                                ),
                            }
                        }
                        None => {}
                    }
                }
            }
        })
        // The outside-render, main-thread tick where the scrying WebView
        // producer is safely driven (it pumps the OS message loop, which would
        // re-enter render() if done in the compositor). Build + navigate a
        // producer for the active web tile (step 2); frames come in step 3.
        .with_on_tick({
            let registry = surface_registry.clone();
            let mut web_host =
                surface_tile::WebSurfaceHost::new(webview_data_dir, surface_channel);
            // Idle-quiet redraw: keep requesting redraws while a web tile is
            // active (input or new frames — so video/scroll/animation stay
            // smooth), but after a grace window with no activity (a static page)
            // stop, letting the event loop sleep. Any interaction or new frame
            // resets it. (~0.5s grace at 60fps.)
            const REDRAW_GRACE_TICKS: u32 = 30;
            let mut idle_ticks: u32 = REDRAW_GRACE_TICKS;
            move |ctx| {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                // Need the shared device + a window. Device exists once the
                // first frame has created it; the producer's frame import needs
                // it (our DX12 device — the same one the importer targets).
                let (Some(device), Some(queue)) = (ctx.device, ctx.queue) else {
                    return;
                };
                let Some(window) = ctx.windows.first() else {
                    return;
                };
                let hwnd = match window.window_handle().map(|h| h.as_raw()) {
                    Ok(RawWindowHandle::Win32(h)) => h.hwnd.get() as *mut std::ffi::c_void,
                    _ => return,
                };
                // The producer is sized to the tile bounds (physical px); the
                // initial size is the window until the first compositor pass
                // records the tile rect. `scale` maps widget logical input → px.
                let inner = window.inner_size();
                let scale = window.scale_factor();
                // First web tile drives the (single, v1) producer.
                let mut web_active = false;
                let mut activity = false;
                for (_id, content) in registry.entries() {
                    if let SurfaceContent::Web { url } = content {
                        activity = web_host.ensure(
                            hwnd,
                            &url,
                            (inner.width, inner.height),
                            device,
                            queue,
                            scale,
                        );
                        web_active = true;
                        break;
                    }
                }
                // Keep redrawing while there's activity (new frames or input) so
                // imported frames reach the screen; once a shown tile goes static
                // (no frames, no input) for the grace window, stop requesting
                // redraws and let the loop idle. A new frame or interaction wakes
                // it and resets the counter.
                if web_active {
                    idle_ticks = if activity { 0 } else { idle_ticks.saturating_add(1) };
                    if idle_ticks < REDRAW_GRACE_TICKS {
                        window.request_redraw();
                    }
                } else {
                    idle_ticks = REDRAW_GRACE_TICKS;
                }
            }
        })
        .run_in(EventLoop::with_user_event())
        .expect("run mere");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The FormeStore wiring's done-line: a first load with nothing on disk
    /// seeds + persists; an edit re-saves; the next load restores the *same*
    /// forme with the edit intact (the workbench survives a restart).
    #[test]
    fn workbench_seeds_then_restores_with_edits() {
        let dir = std::env::temp_dir().join(format!("mere-app-wb-{}", uuid::Uuid::new_v4()));

        // First load: empty store → seed + persist.
        let seeded = load_or_seed_workbench(&dir);
        let id = seeded.id;
        let tiles = project_tree(&seeded.arrangement).tile_count();
        assert!(tiles >= 3, "seed should have at least the 3 demo tiles");

        // Edit (simulating the "+ tile" button) and persist.
        let mut doc = seeded;
        let root = doc.arrangement.root();
        let added = doc.arrangement.insert(
            ArrangementNodeKind::TileIntent { member: None },
            Some("added".into()),
        );
        doc.arrangement.attach(added, root);
        save_forme(&dir, &mut doc, now_ms()).expect("persist edit");

        // Next load: same forme id, edit preserved.
        let restored = load_or_seed_workbench(&dir);
        assert_eq!(restored.id, id, "should restore the same forme, not reseed");
        assert_eq!(
            project_tree(&restored.arrangement).tile_count(),
            tiles + 1,
            "the added tile should survive the reload"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn history_back_forward_and_branch() {
        let mut h = History::new("mere://welcome".into());
        h.push("mere://a");
        h.push("mere://b");
        assert_eq!(h.back(), Some("mere://a"));
        assert_eq!(h.back(), Some("mere://welcome"));
        assert_eq!(h.back(), None); // at the start
        assert_eq!(h.forward(), Some("mere://a"));
        // Navigating from the middle drops the forward entry (b).
        h.push("mere://z");
        assert_eq!(h.forward(), None); // z is the tip
        // Re-pushing the current tip is a no-op (no duplicate entry).
        h.push("mere://z");
        assert_eq!(h.back(), Some("mere://a"));
    }

    /// Slice 4 done-line: a first load seeds + persists the graph; the next load
    /// restores it with the same node ids (so workbench bindings stay valid).
    #[test]
    fn graph_seeds_then_restores_with_stable_ids() {
        let dir = std::env::temp_dir().join(format!("mere-app-graph-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let seeded = load_or_seed_graph(&dir);
        let ids: std::collections::HashSet<_> = seeded.nodes().map(|(_, n)| n.id).collect();
        assert!(!ids.is_empty(), "seed graph should have nodes");

        let restored = load_or_seed_graph(&dir);
        let restored_ids: std::collections::HashSet<_> =
            restored.nodes().map(|(_, n)| n.id).collect();
        assert_eq!(ids, restored_ids, "node ids must survive a restart");

        std::fs::remove_dir_all(&dir).ok();
    }
}
