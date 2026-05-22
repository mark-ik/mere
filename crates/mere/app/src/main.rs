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

mod engine_tile;
mod graph_canvas;

use std::path::{Path, PathBuf};

use engine_tile::RenderedTile;
use forme::store::{load_all_formes, save_forme};
use forme::{ArrangementNodeKind, FormeDocument};
use kernel::geometry::PortablePoint;
use kernel::graph::{Graph, NavigationTrigger, Traversal};
use platen::{PlanSlot, TilePlan, project_tree};
use xilem::masonry::kurbo::Axis;
use xilem::view::{FlexExt as _, button, canvas, flex_col, flex_row, label, prose, split};
use xilem::{AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem};

/// Seeded markdown for the live engine tile. Routed through `inker` and
/// rendered by the `nematic` markdown engine at startup — proof the engine
/// seam produces real content, not a placeholder. Edit here to change the
/// welcome page.
const WELCOME_MD: &str = "\
# Welcome to Mere

A **spatial browser** on the *composition spine*: graph truth → forme →
platen → verso → inker.

This tile is **live**. Its content was routed through `inker`'s engine
policy and parsed by the `nematic` markdown engine — nothing below the rule
is placeholder text.

## What you're looking at

- The **workbench** projects a forme arrangement into tiles
- The **orrery** is the spatial graph view (canvas widget pending)
- This document proves the engine-routing seam end to end

---

    route(address, content_type) -> Engine -> EngineDocument

Edit `WELCOME_MD` in `main.rs` to change this page.
";

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
    /// The live engine-backed tile: a document routed through `inker` and
    /// rendered by a `nematic` engine at startup. The v1 "one live tile."
    welcome: RenderedTile,
    /// Where session state lives on disk (the forme store writes under
    /// `<session_dir>/formes/`).
    session_dir: PathBuf,
    /// Human label for the current frame/workspace (placeholder for the
    /// real `FrameLayout` once the canvas + multi-pane interaction land).
    frame_label: String,
}

impl AppState {
    fn new() -> Self {
        let mut graph = Graph::new();
        let keys: Vec<_> = (0..6)
            .map(|i| graph.add_node(format!("seed://node/{i}"), PortablePoint::new(0.0, 0.0)))
            .collect();
        // A few seed relations so the orrery shows a connected graph, not
        // loose dots. Traversal edges = "navigated A → B".
        for &(a, b) in &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (0, 3), (0, 5)] {
            graph.push_traversal(keys[a], keys[b], Traversal::now(NavigationTrigger::LinkClick));
        }

        // Restore the workbench forme from disk, or seed + persist a fresh one
        // on first run. (The in-memory graph is re-seeded each run — graph
        // persistence is a separate slice; forme persistence stands alone.)
        let session_dir = session_dir();
        let workbench = load_or_seed_workbench(&session_dir);

        // Route + render the welcome document at startup through the engine
        // seam (inker policy → nematic markdown engine).
        let welcome =
            engine_tile::render_address("mere://welcome", WELCOME_MD, Some("text/markdown"));

        Self {
            graph,
            workbench,
            welcome,
            session_dir,
            frame_label: "Mere".to_string(),
        }
    }

    fn node_count(&self) -> usize {
        self.graph.nodes().count()
    }

    /// Persist the workbench forme after an edit. Failures are surfaced to
    /// stderr, not fatal — a transient write error shouldn't crash the app.
    fn persist_workbench(&mut self) {
        if let Err(e) = save_forme(&self.session_dir, &mut self.workbench, now_ms()) {
            eprintln!("mere: failed to persist workbench forme: {e}");
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

/// The whole app view: a frametree of splits. Left = workbench; right
/// column = orrery (top) over apparatus (bottom). This *is* the
/// frametree — splits are split views, panes are view functions.
fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    split(
        workbench_pane(state),
        split(orrery_pane(state), apparatus_pane(state)).split_axis(Axis::Vertical),
    )
}

/// Workbench pane — renders the forme arrangement through platen's tree
/// projection: slots laid side-by-side, tab-stacks shown grouped. The
/// "+ tile" button mutates the arrangement and the projection re-renders,
/// proving the forme → platen → view loop end-to-end. Tile *content*
/// (real engines) is a later slice; tiles show their label for now.
fn workbench_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let plan = project_tree(&state.workbench.arrangement);
    let slots: Vec<Box<AnyWidgetView<AppState>>> = plan.slots.iter().map(slot_view).collect();
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

/// The live engine-backed tile: a header naming the engine that handled the
/// address, then the rendered document. This realizes the v1 "one live
/// engine-backed tile" — the projected slots above are still placeholders
/// (label-only), but *this* tile is real engine output.
fn live_tile(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let tile = &state.welcome;
    let mut children: Vec<Box<AnyWidgetView<AppState>>> = vec![
        label("▣ live tile").text_size(13.0).boxed(),
        prose(format!(
            "engine: {} · {}",
            tile.engine_id,
            tile.document.title.as_deref().unwrap_or("(untitled)"),
        ))
        .boxed(),
    ];
    children.extend(engine_tile::document_views(&tile.document));
    flex_col(children)
}

/// Render one workbench slot: a single tile, or a tab-stack.
fn slot_view(slot: &PlanSlot) -> Box<AnyWidgetView<AppState>> {
    match slot {
        PlanSlot::Tile(t) => {
            flex_col((label("▭ tile").text_size(13.0), prose(tile_label(t)))).boxed()
        }
        PlanSlot::Tabs(tabs) => {
            let rows: Vec<_> = tabs
                .iter()
                .map(|t| prose(format!("• {}", tile_label(t))))
                .collect();
            flex_col((label(format!("▭ tabs ({})", tabs.len())).text_size(13.0), rows)).boxed()
        }
    }
}

/// Display name for a tile: its label, else its bound member, else unbound.
fn tile_label(t: &TilePlan) -> String {
    t.label.clone().unwrap_or_else(|| match t.member {
        Some(m) => format!("member {}", &m.to_string()[..8]),
        None => "(unbound)".to_string(),
    })
}

/// Orrery pane — the spatial graph view. A custom Masonry `canvas`
/// paints the kernel graph (see [`graph_canvas`]); the "+ node" button
/// mutates graph truth and the canvas re-renders. Real cartography
/// layout, camera, and hit-testing are later slices.
fn orrery_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let relations = state.graph.relations().count();
    flex_col((
        label("Orrery").text_size(18.0),
        prose(format!("graph: {} nodes · {relations} relations", state.node_count())),
        button(label("+ node"), |state: &mut AppState| {
            let n = state.node_count();
            state
                .graph
                .add_node(format!("seed://node/{n}"), PortablePoint::new(0.0, 0.0));
        }),
        // The spatial graph view: a custom Masonry canvas painting graph truth
        // (ring layout, relations as lines, parley labels). Fills the pane.
        canvas(|state: &mut AppState, ctx, scene, size| {
            graph_canvas::paint_graph(scene, ctx, size, &state.graph);
        })
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
}
