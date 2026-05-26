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
//! The chrome shape is a frametree of `split` views: a Workbench pane
//! (forme → tree projection + a live engine-backed tile), an Orrery pane
//! (the spatial graph view — a custom Masonry `canvas` painting graph
//! truth, see [`graph_canvas`]), and an Apparatus pane (diagnostics). The
//! "+ node" / "+ tile" buttons prove the reactive loop end-to-end (mutate
//! `AppState` → view rebuilds → canvas/projection re-render).

mod camera;
mod engine_tile;
mod graph_canvas;
mod navigation;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use engine_tile::RenderedTile;
use forme::store::{load_all_formes, save_forme};
use forme::{ArrangementNodeKind, FormeDocument};
use graph_canvas::{GraphAction, graph_canvas, scene_from_graph};
use kernel::geometry::PortablePoint;
use kernel::graph::{Graph, NavigationTrigger, Traversal};
use platen::{PlanSlot, TilePlan, project_tree};
use xilem::masonry::kurbo::Axis;
use xilem::view::{FlexExt as _, button, flex_col, flex_row, label, prose, split, text_input};
use xilem::{AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem};

/// Lay seed nodes (and new ones) on a world-space ring so the orrery shows a
/// spread graph. The kernel seeds all nodes at the origin; the orrery reads
/// projected positions, so we assign them here.
fn ring_world(i: usize, n: usize) -> PortablePoint {
    let theta = std::f64::consts::TAU * (i as f64) / (n.max(1) as f64) - std::f64::consts::FRAC_PI_2;
    let radius = 140.0;
    PortablePoint::new((radius * theta.cos()) as f32, (radius * theta.sin()) as f32)
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
    /// The currently-navigated document: resolved by [`navigation`] and
    /// rendered through `inker`. Replaced on every navigation.
    current: RenderedTile,
    /// The orrery's selected node (highlighted; clicking it opened `current`).
    selected_node: Option<uuid::Uuid>,
    /// Rendered content for workbench tiles bound to a graph member, keyed by
    /// member id. Built once at startup (tile content is static for v1).
    tile_docs: HashMap<uuid::Uuid, RenderedTile>,
    /// Where session state lives on disk (the forme store writes under
    /// `<session_dir>/formes/`).
    session_dir: PathBuf,
    /// Human label for the current frame/workspace (placeholder for the
    /// real `FrameLayout` once the canvas + multi-pane interaction land).
    frame_label: String,
}

impl AppState {
    fn new() -> Self {
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

        Self {
            graph,
            workbench,
            omnibar,
            current,
            selected_node: None,
            tile_docs,
            session_dir,
            frame_label: "Mere".to_string(),
        }
    }

    fn node_count(&self) -> usize {
        self.graph.nodes().count()
    }

    /// Navigate to `address`: resolve + render it into the current tile, and
    /// sync the omnibar text to the address.
    fn navigate(&mut self, address: &str) {
        self.current = navigation::open(address);
        self.omnibar = address.to_string();
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

/// The whole app view: a frametree of splits. Left = workbench; right
/// column = orrery (top) over apparatus (bottom). This *is* the
/// frametree — splits are split views, panes are view functions.
fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        omnibar_bar(state),
        split(
            workbench_pane(state),
            split(orrery_pane(state), apparatus_pane(state)).split_axis(Axis::Vertical),
        )
        .flex(1.0),
    ))
}

/// The omnibar: type an address and press Enter (or click Go) to navigate.
/// Routes through [`navigation`] → `inker` and updates the live tile.
fn omnibar_bar(state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_row((
        text_input(state.omnibar.clone(), |state: &mut AppState, text| {
            state.omnibar = text;
        })
        .on_enter(|state: &mut AppState, text| state.navigate(&text))
        .flex(1.0),
        button(label("Go"), |state: &mut AppState| {
            let address = state.omnibar.clone();
            state.navigate(&address);
        }),
    ))
}

/// Workbench pane — renders the forme arrangement through platen's tree
/// projection: slots laid side-by-side, tab-stacks shown grouped. The
/// "+ tile" button mutates the arrangement and the projection re-renders,
/// proving the forme → platen → view loop end-to-end. Tiles bound to a graph
/// member render that member's engine content (compact); unbound tiles show a
/// label placeholder.
fn workbench_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let plan = project_tree(&state.workbench.arrangement);
    let slots: Vec<Box<AnyWidgetView<AppState>>> = plan
        .slots
        .iter()
        .map(|s| slot_view(s, &state.tile_docs))
        .collect();
    flex_col((
        label("Workbench").text_size(18.0),
        prose(format!(
            "forme \"{}\" → tree projection: {} slot(s), {} tile(s) · persisted",
            state.workbench.label.as_deref().unwrap_or("workbench"),
            plan.slots.len(),
            plan.tile_count()
        )),
        button(label("+ tile"), |state: &mut AppState| {
            let arr = &mut state.workbench.arrangement;
            let root = arr.root();
            let n = arr.len();
            let id = arr.insert(
                ArrangementNodeKind::TileIntent { member: None },
                Some(format!("tile {n}")),
            );
            arr.attach(id, root);
            state.persist_workbench();
        }),
        flex_row(slots),
        live_tile(state),
    ))
}

/// The focus tile: the omnibar/orrery-navigated document (`state.current`),
/// shown below the workbench's bound tiles. Header names the engine + address.
fn live_tile(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let tile = &state.current;
    let mut children: Vec<Box<AnyWidgetView<AppState>>> = vec![
        label("▣ live tile").text_size(13.0).boxed(),
        prose(format!(
            "{} · engine: {} · {}",
            tile.document.address,
            tile.engine_id,
            tile.document.title.as_deref().unwrap_or("(untitled)"),
        ))
        .boxed(),
    ];
    children.extend(engine_tile::document_views(&tile.document));
    flex_col(children)
}

/// Render one workbench slot: a single tile, or a tab-stack (active tab's
/// content + a strip of the rest).
fn slot_view(
    slot: &PlanSlot,
    docs: &HashMap<uuid::Uuid, RenderedTile>,
) -> Box<AnyWidgetView<AppState>> {
    match slot {
        PlanSlot::Tile(t) => tile_view(t, docs),
        PlanSlot::Tabs(tabs) => {
            let mut children: Vec<Box<AnyWidgetView<AppState>>> =
                vec![label(format!("▭ tabs ({})", tabs.len())).text_size(13.0).boxed()];
            for t in tabs.iter().skip(1) {
                children.push(prose(format!("• {}", tile_label(t))).boxed());
            }
            if let Some(active) = tabs.first() {
                children.push(tile_view(active, docs));
            }
            flex_col(children).boxed()
        }
    }
}

/// A single workbench tile: its bound member's rendered content (compact), or a
/// label placeholder when unbound / unresolved.
fn tile_view(t: &TilePlan, docs: &HashMap<uuid::Uuid, RenderedTile>) -> Box<AnyWidgetView<AppState>> {
    match t.member.and_then(|m| docs.get(&m)) {
        Some(tile) => {
            let mut children: Vec<Box<AnyWidgetView<AppState>>> = vec![
                label(format!(
                    "▭ {}",
                    tile.document.title.as_deref().unwrap_or("(untitled)")
                ))
                .text_size(13.0)
                .boxed(),
                prose(tile.document.address.clone()).boxed(),
            ];
            children.extend(
                engine_tile::document_views(&tile.document)
                    .into_iter()
                    .take(3),
            );
            flex_col(children).boxed()
        }
        None => flex_col((label("▭ tile").text_size(13.0), prose(tile_label(t)))).boxed(),
    }
}

/// Display name for a tile: its label, else its bound member, else unbound.
fn tile_label(t: &TilePlan) -> String {
    t.label.clone().unwrap_or_else(|| match t.member {
        Some(m) => format!("member {}", &m.to_string()[..8]),
        None => "(unbound)".to_string(),
    })
}

/// Orrery pane — the spatial graph view, a bespoke Masonry widget (see
/// [`graph_canvas`]). Drag a node to move it (written back to graph
/// truth), drag empty space to pan, wheel to zoom. The "+ node" button
/// mutates graph truth and the view re-renders. Real cartography layout
/// (force-directed / radial) and LOD are later slices.
fn orrery_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let relations = state.graph.relations().count();
    flex_col((
        label("Orrery").text_size(18.0),
        prose(format!("graph: {} nodes · {relations} relations", state.node_count())),
        button(label("+ node"), |state: &mut AppState| {
            let n = state.node_count();
            let key = state
                .graph
                .add_node(format!("mere://node/{n}"), PortablePoint::new(0.0, 0.0));
            // Place it on the ring (committed position, so it persists).
            state.graph.set_node_position(key, ring_world(n, 8));
            state.persist_graph();
        }),
        // The spatial graph view: a bespoke Masonry widget. Drag a node to move
        // it (writes back to graph truth), drag empty space to pan, wheel to
        // zoom toward the cursor.
        graph_canvas(
            scene_from_graph(&state.graph),
            state.selected_node,
            |state: &mut AppState, action: GraphAction| match action {
                GraphAction::NodeMoved { id, world } => {
                    if let Some(key) = state.graph.get_node_key_by_id(id) {
                        // Per-move: update committed position in memory only
                        // (persist happens on NodeDropped, not every tick).
                        state.graph.set_node_position(
                            key,
                            PortablePoint::new(world.x as f32, world.y as f32),
                        );
                    }
                }
                GraphAction::NodeDropped { .. } => state.persist_graph(),
                GraphAction::NodeActivated { id } => {
                    state.selected_node = Some(id);
                    let url = state
                        .graph
                        .get_node_by_id(id)
                        .map(|(_, node)| node.url().to_string());
                    if let Some(url) = url {
                        state.navigate(&url);
                    }
                }
            },
        )
        .flex(1.0),
    ))
}

/// Apparatus pane — diagnostics / inspector. Static placeholder reading
/// app state.
fn apparatus_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let forme_id = state.workbench.id.as_uuid().to_string();
    flex_col((
        label("Apparatus").text_size(18.0),
        prose(format!("frame: {}", state.frame_label)),
        prose(format!("graph nodes: {}", state.node_count())),
        prose(format!("session: {}", state.session_dir.display())),
        prose(format!("workbench forme: {}", &forme_id[..8])),
    ))
}

fn main() {
    Xilem::new_simple(AppState::new(), app_logic, WindowOptions::new("Mere"))
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
