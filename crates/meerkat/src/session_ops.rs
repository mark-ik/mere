/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Session lifecycle for the shell: persisting a session (graph + camera +
//! frame), renaming sessions in the switcher, rebuilding switcher thumbnails and
//! labels, and the `Shell`-level create / switch / cycle / close ops that re-key
//! the focused orrery in the pool (the step a per-window `WindowCtx` cannot do).
//! The per-window pieces hang off `WindowCtx`; the pool re-keying off `Shell`.
//! Factored out of `frame_ops.rs` to keep files under the 600-LOC ceiling.

use frame::{GraphId, SessionId};
use kernel::geometry::PortablePoint;
use kernel::graph::Graph;
use session_runtime::{
    SwitcherThumbnailOptions, ViewIntent, build_switcher_thumbnail_with, frame_layout_store,
    manifest::GraphSessionManifest, session_graph_store, view_intent_store,
};

use super::switcher::{SWITCHER_THUMB_H, SWITCHER_THUMB_W};
use super::{DEFAULT_FRAME, DEFAULT_PANE, WindowCtx};

/// Filename for the workbench tiling sidecar (beside `graph.json`): the platen
/// bridge's canonical `(arrangement, geometry)` pair, so a workbench's split shape,
/// tab stacks, and active tab survive a restart. The live `Pane` tree carries no
/// serde, so it persists through the bridge rather than directly. (A3 persistence.)
const WORKBENCH_FILE: &str = "workbench.json";

/// Kill-switch for restoring persisted workbench tiling on load. The read already
/// falls back to an empty workbench on any IO/parse error; flipping this to `false`
/// disables the whole path, should the round-trip ever prove wanting in the field.
const RESTORE_WORKBENCH_TILING: bool = true;

/// Load a persisted workbench from `session_dir`, pruned to `present` (the loaded
/// graph's member ids, so a tile whose node was deleted is reconciled away). Empty
/// workbench when the feature is off, the file is absent, or the JSON fails to
/// parse. Shared by the session-switch path ([`WindowCtx::restore_workbench`]) and
/// the boot restore in `main.rs`, so a restart reloads the tiling too. (A3.)
pub(crate) fn load_workbench(
    session_dir: &std::path::Path,
    present: &std::collections::HashSet<forme::GraphMemberId>,
) -> platen::Workbench {
    if !RESTORE_WORKBENCH_TILING {
        return platen::Workbench::new();
    }
    std::fs::read_to_string(session_dir.join(WORKBENCH_FILE))
        .ok()
        .and_then(|json| platen::Workbench::from_persisted_json(json.as_str(), present))
        .unwrap_or_else(platen::Workbench::new)
}

/// Filename for the cartography position sidecar (beside `graph.json`): the orrery's
/// settled node positions, member-keyed (the Cartography projection geometry, the
/// counterpart of `workbench.json`'s TreeGeometry). The live force-directed layout is
/// never committed to the kernel graph, so this is what makes a session's settled
/// layout durable across a restart. (Position sidecar.)
const CARTOGRAPHY_FILE: &str = "cartography.json";

/// Kill-switch for restoring persisted orrery positions on load. On any miss the host
/// falls back to the graph's own load-time seed (the prior behavior). (Position sidecar.)
const RESTORE_CARTOGRAPHY: bool = true;

/// Load the persisted cartography positions from `session_dir`, pruned to `present`
/// (the loaded graph's members). `None` when the feature is off, the file is absent,
/// or the JSON fails to parse — the orrery then keeps its graph-seeded layout. Shared
/// by the session-switch and boot restore paths. (Position sidecar.)
pub(crate) fn load_cartography(
    session_dir: &std::path::Path,
    present: &std::collections::HashSet<forme::GraphMemberId>,
) -> Option<platen::CartographyGeometry> {
    if !RESTORE_CARTOGRAPHY {
        return None;
    }
    std::fs::read_to_string(session_dir.join(CARTOGRAPHY_FILE))
        .ok()
        .and_then(|json| platen::CartographyGeometry::from_persisted_json(json.as_str(), present))
}

impl WindowCtx<'_> {
    /// The session a `graph_id` belongs to: its [`SessionId`] and storage dir,
    /// resolved by the graph (manifests are keyed by session but each carries its
    /// `root_graph_id`). This is the pane-as-unit resolution — a pane pinned to a
    /// graph saves / renames / navigates through *this* session, not a global
    /// active one. Falls back to the conventional `sessions/<id>` dir when a
    /// manifest has no explicit `storage_path`. (Window composition — pane-as-unit.)
    pub(super) fn session_for_graph(
        &self,
        graph_id: GraphId,
    ) -> Option<(SessionId, std::path::PathBuf)> {
        self.shared.session.manifests.iter().find_map(|(id, m)| {
            (m.root_graph_id == graph_id).then(|| {
                let dir = m.storage_path.clone().unwrap_or_else(|| {
                    self.shared
                        .session
                        .mere_root
                        .join("sessions")
                        .join(id.as_uuid().to_string())
                });
                (id, dir)
            })
        })
    }

    /// Move input focus to the graph-pane resolving to `graph_id`: point the
    /// window's `focused_graph` at it and re-key the active session (id + dir) to its
    /// session, so the context menu, selection, nav, the omnibar, and save all act on
    /// *this* pane. Focus is just a pointer — this does **not** reload the graph or
    /// clear the window's content caches (both graphs coexist), unlike a session
    /// switch. No-op if already focused or the graph has no session. (Window
    /// composition — pane-as-unit; focus-follows-click.)
    pub(super) fn focus_pane_graph(&mut self, graph_id: GraphId) {
        if self.view.focused_graph == graph_id {
            return;
        }
        let Some((session_id, session_dir)) = self.session_for_graph(graph_id) else {
            return;
        };
        self.view.focused_graph = graph_id;
        self.shared.session.active_session_id = session_id;
        self.shared.session.session_dir = session_dir;
        self.view.request_redraw();
    }

    /// Persist the **focused pane's** session: its graph + camera view-intent under
    /// that session's dir, resolved from `focused_graph` (not a global active
    /// session), so two live graphs each persist to their own storage. The frame
    /// layout is window-scoped and stays at the shared root. Best-effort: a write
    /// failure is logged, not fatal. Called after each navigation and on close.
    /// (Window composition — pane-as-unit; per-pane save.)
    pub(super) fn save_session(&mut self) {
        let Some((session_id, session_dir)) = self.session_for_graph(self.view.focused_graph)
        else {
            return; // the focused graph has no session manifest (should not happen)
        };
        let graph_file = session_dir.join(session_graph_store::GRAPH_FILE);
        if let Err(err) = session_graph_store::save(&graph_file, self.orrery().graph()) {
            tracing::warn!(%err, path = ?graph_file, "failed to persist the session graph");
        }
        let intent = ViewIntent {
            camera: Some(super::camera_to_snapshot(
                self.orrery().camera(),
                self.orrery().yaw(),
                self.orrery().tilt(),
            )),
            focus: self.orrery().focused_url().map(str::to_string),
            strategy: self.orrery().layout_strategy().map(str::to_string),
            mirror_tiles: self.view.mirror_tiles,
            ..Default::default()
        };
        if let Err(err) =
            view_intent_store::save_view_intent(&session_dir, DEFAULT_FRAME, DEFAULT_PANE, &intent)
        {
            tracing::warn!(%err, dir = ?session_dir, "failed to persist the view intent");
        }
        // The content frame's pane layout (which panes are open + split ratios) is
        // **window-scoped** (Model B, MG5): it persists at the shared root and stays
        // put across session switches, so a graph swap re-sources the panes without
        // rearranging them.
        if let Err(err) = frame_layout_store::save_frame_layout(&self.shared.session.mere_root, &self.view.frame_layout)
        {
            tracing::warn!(%err, dir = ?self.shared.session.mere_root, "failed to persist the frame layout");
        }
        // The workbench tiling is the focused graph's, so it persists per-session
        // beside graph.json. Saved as the bridge's (arrangement, geometry) pair (the
        // Pane tree is the derived live model, not serde). Best-effort. (A3.)
        match self.view.workbench.to_persisted_json() {
            Ok(json) => {
                let path = session_dir.join(WORKBENCH_FILE);
                if let Err(err) = std::fs::write(&path, json) {
                    tracing::warn!(%err, path = ?path, "failed to persist the workbench tiling");
                }
            }
            Err(err) => tracing::warn!(%err, "failed to serialize the workbench tiling"),
        }
        // The orrery's settled layout: persist the live node positions as the
        // cartography sidecar (the live force-directed layout is never committed to
        // the kernel graph, so this is what survives a restart). Best-effort. (Position sidecar.)
        match self.orrery().cartography_geometry().to_persisted_json() {
            Ok(json) => {
                let path = session_dir.join(CARTOGRAPHY_FILE);
                if let Err(err) = std::fs::write(&path, json) {
                    tracing::warn!(%err, path = ?path, "failed to persist the cartography positions");
                }
            }
            Err(err) => tracing::warn!(%err, "failed to serialize the cartography positions"),
        }
        // Auto-reconcile this graph's Linked graphlets after the save, so their persisted
        // roster tracks graph drift (the scoped window already tracks it live via re-derive;
        // this keeps the on-disk set + other readers current). Deferred to Shell (needs
        // `&mut graphlets`); cheap + idempotent. (Graphlet wiring Phase 3 slice 2+.)
        let focused = self.view.focused_graph;
        if self
            .graphlets
            .get(&focused)
            .is_some_and(|idx| idx.has_linked())
        {
            self.commands
                .push(super::ShellCommand::ReconcileGraphlets { graph: focused });
        }
        // Record the save in this session's manifest (advances `updated_at`, the
        // switcher's recency key) and flush the registry. (Multi-graph MG1.)
        self.shared.session.manifests.update(session_id, |_| {});
        if let Err(err) = self.shared.session.manifests.flush_dirty() {
            tracing::warn!(%err, "failed to flush the session registry");
        }
        // Keep this session's switcher thumbnail live as its graph grows (cheap; no
        // disk read, unlike the full refresh on a session change).
        let opts = SwitcherThumbnailOptions {
            width: SWITCHER_THUMB_W,
            height: SWITCHER_THUMB_H,
            ..SwitcherThumbnailOptions::default()
        };
        let orrery = self.orrery();
        let thumb =
            build_switcher_thumbnail_with(orrery.graph(), |k| orrery.node_position(k), opts);
        self.shared.session.session_thumbnails.insert(session_id, thumb);
    }

    /// Restore the focused graph's persisted workbench tiling from its sidecar,
    /// pruned to the live graph's members. Thin wrapper over [`load_workbench`] for
    /// the session-switch path (which has a live `ctx`). (A3 persistence.)
    pub(super) fn restore_workbench(&self, session_dir: &std::path::Path) -> platen::Workbench {
        let present = self.orrery().graph().nodes().map(|(_, node)| node.id).collect();
        load_workbench(session_dir, &present)
    }

    /// Begin renaming `id`: seed the switcher edit buffer from its current label, so
    /// editing starts from the shown name (display or derived). (Host text path.)
    pub(super) fn start_rename(&mut self, id: SessionId) {
        if self.shared.session.manifests.get(id).is_none() {
            return;
        }
        let seed = self.shared.session.session_labels.get(&id).cloned().unwrap_or_default();
        self.view.renaming = Some((id, seed));
        self.view.request_redraw();
    }

    /// Commit the in-progress rename: a non-empty name sets the session's display
    /// name; an empty one clears it (the label reverts to the derived one). Persists
    /// the manifest and refreshes the labels. (Host text path.)
    pub(super) fn commit_rename(&mut self) {
        let Some((id, name)) = self.view.renaming.take() else {
            return;
        };
        let trimmed = name.trim().to_string();
        let display = (!trimmed.is_empty()).then_some(trimmed);
        self.shared.session.manifests.update(id, |m| m.display_name = display);
        if let Err(err) = self.shared.session.manifests.flush_dirty() {
            tracing::warn!(%err, "failed to flush the renamed session manifest");
        }
        self.refresh_session_thumbnails();
        self.view.request_redraw();
    }

    /// Drop the in-progress rename without saving (Escape, or an interaction that
    /// moves on). A no-op when not renaming. (Host text path.)
    pub(super) fn cancel_rename(&mut self) {
        if self.view.renaming.take().is_some() {
            self.view.request_redraw();
        }
    }

    /// Append typed `ch` to the rename buffer. No-op when not renaming. (Host text.)
    pub(super) fn rename_push(&mut self, ch: &str) {
        if let Some((_, buf)) = self.view.renaming.as_mut() {
            buf.push_str(ch);
            self.view.request_redraw();
        }
    }

    /// Delete the last char of the rename buffer (Backspace). (Host text path.)
    pub(super) fn rename_backspace(&mut self) {
        if let Some((_, buf)) = self.view.renaming.as_mut() {
            buf.pop();
            self.view.request_redraw();
        }
    }

    /// Rebuild the per-session switcher thumbnails **and labels**: the active
    /// session from the **live** orrery graph, each inactive session from its cold
    /// `graph.json`. The label is the user's display name, else one derived from the
    /// graph. Drops entries for closed sessions. Called on every session or graph
    /// change (cheap small-graph walks; no per-frame disk reads). (Multi-graph MG4 /
    /// host text path.)
    pub(super) fn refresh_session_thumbnails(&mut self) {
        let opts = SwitcherThumbnailOptions {
            width: SWITCHER_THUMB_W,
            height: SWITCHER_THUMB_H,
            ..SwitcherThumbnailOptions::default()
        };
        let ids: Vec<SessionId> = self.shared.session.manifests.iter().map(|(id, _)| id).collect();
        let live: std::collections::HashSet<SessionId> = ids.iter().copied().collect();
        self.shared.session.session_thumbnails.retain(|id, _| live.contains(id));
        self.shared.session.session_labels.retain(|id, _| live.contains(id));
        for id in ids {
            // A user-set display name wins; otherwise derive a short label.
            let display_name = self.shared.session.manifests
                .get(id)
                .and_then(|m| m.display_name.clone())
                .filter(|n| !n.trim().is_empty());
            // A session whose graph is *pooled* (live in any pane, not just the
            // focused one) thumbnails off its live orrery; the rest cold-load from
            // disk. (Pane-as-unit — was "the active session only".)
            let pooled = self
                .shared
                .session
                .manifests
                .get(id)
                .map(|m| m.root_graph_id)
                .and_then(|gid| self.orreries.get(&gid));
            let (thumb, label) = if let Some(orrery) = pooled {
                let g = orrery.graph();
                let label = display_name.unwrap_or_else(|| derive_session_label(g));
                (build_switcher_thumbnail_with(g, |k| orrery.node_position(k), opts), label)
            } else {
                let dir = self.shared.session.mere_root
                    .join("sessions")
                    .join(id.as_uuid().to_string());
                let graph = session_graph_store::load(&dir.join(session_graph_store::GRAPH_FILE))
                    .ok()
                    .flatten()
                    .unwrap_or_else(Graph::new);
                let label = display_name.unwrap_or_else(|| derive_session_label(&graph));
                // Positions are no longer in graph.json; draw the cold thumbnail from
                // the session's cartography sidecar (origin without one). (Position gut.)
                let present: std::collections::HashSet<forme::GraphMemberId> =
                    graph.nodes().map(|(_, n)| n.id).collect();
                let positions: std::collections::HashMap<forme::GraphMemberId, (f32, f32)> =
                    load_cartography(&dir, &present).map(|g| g.iter().collect()).unwrap_or_default();
                let thumb = build_switcher_thumbnail_with(
                    &graph,
                    |k| {
                        graph
                            .get_node(k)
                            .and_then(|n| positions.get(&n.id))
                            .map(|&(x, y)| PortablePoint::new(x, y))
                    },
                    opts,
                );
                (thumb, label)
            };
            self.shared.session.session_thumbnails.insert(id, thumb);
            self.shared.session.session_labels.insert(id, label);
        }
    }
}

// Session ops live on `Shell`, not `WindowCtx`: switching a session re-keys the
// orrery pool, and a `WindowCtx` holds exactly one orrery borrowed *out* of the
// pool, so it cannot insert or re-key entries. Per-window input handlers request
// these by pushing a [`ShellCommand`]; `Shell::apply` runs them after the ctx
// borrow ends (the same seam as spawn/close). WindowCtx-shaped sub-steps
// (save_session, the cache reset, thumbnails) re-enter through `self.ctx()`, which
// resolves the focused view primary-or-pending and bundles its pooled orrery.
// (Window composition P1, multi-graph.)
impl super::Shell {
    /// Mint a fresh, empty session and make it active (persisting the current one
    /// first). The new session opens on the welcome node. Returns its id. (MG3.)
    pub(super) fn create_session(&mut self) -> SessionId {
        self.ctx().save_session();
        let session_id = SessionId::new();
        let session_dir = self
            .shared
            .session
            .mere_root
            .join("sessions")
            .join(session_id.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        let mut manifest = GraphSessionManifest::new(session_id, GraphId::new());
        manifest.storage_path = Some(session_dir.clone());
        self.shared.session.manifests.insert(manifest);
        let _ = self.shared.session.manifests.flush_dirty();
        self.load_active_session(session_id, session_dir, true);
        session_id
    }

    /// Mint an independent **fork** for a tear-out (G4): a new session + graph holding a
    /// copy of `node`'s connected component (cloned out of graph `from` via
    /// [`Graph::copy_component_from`], each copy carrying `CopiedFrom` provenance), with
    /// a weak `parent_session` ref back to the donor. The fork graph is persisted and its
    /// orrery pooled under the new id, so the caller can open a window onto it. Unlike
    /// [`create_session`](Self::create_session) this does **not** switch the active
    /// session — the donor window is untouched; the fork is a new window. Returns the new
    /// graph id, or `None` if the donor graph or seed is already gone.
    pub(super) fn fork_session_from(
        &mut self,
        node: uuid::Uuid,
        from: GraphId,
    ) -> Option<GraphId> {
        // Commit the donor's live physics layout into its graph first, so the fork
        // preserves it (the graph's own node positions are only the spawn seed; the live
        // layout lives in the orrery's physics view — without this the fork opens with
        // every node piled at the seed).
        self.orreries.get_mut(&from)?.commit_positions_to_graph();
        // Clone the donor as the copy source (a pool borrow-split is a refinement; demo
        // graphs are small), then snapshot the seed's connected component into a fresh
        // graph. An empty result means the seed is no longer in the donor — nothing to
        // fork.
        let source = self.orreries.get(&from).map(|o| o.graph().clone())?;
        let mut fork_graph = Graph::new();
        if fork_graph
            .copy_component_from(&source, node, Some(from.as_uuid().to_string()))
            .is_empty()
        {
            return None;
        }
        // Mint the fork session + graph, with a weak parent ref to the current (donor)
        // session — the lineage edge a later "back to parent" affordance reads.
        let parent = self.shared.session.active_session_id;
        let session_id = SessionId::new();
        let graph_id = GraphId::new();
        let session_dir = self
            .shared
            .session
            .mere_root
            .join("sessions")
            .join(session_id.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        let mut manifest = GraphSessionManifest::new(session_id, graph_id);
        manifest.parent_session = Some(parent);
        manifest.storage_path = Some(session_dir.clone());
        self.shared.session.manifests.insert(manifest);
        let _ = self.shared.session.manifests.flush_dirty();
        // Persist the fork graph beside its manifest, then pool its orrery so a window
        // bound to `graph_id` renders it. No `load_active_session` — the active session
        // stays the donor (a fork is a new window, not a switch).
        let _ = session_graph_store::save(
            &session_dir.join(session_graph_store::GRAPH_FILE),
            &fork_graph,
        );
        self.orreries
            .insert(graph_id, orrery::Orrery::with_graph(fork_graph));
        self.shared.observability.record_probe(
            "tear_out",
            "fork",
            format!("node={node} graph={}", graph_id.as_uuid()),
        );
        Some(graph_id)
    }

    /// Mint + persist a tear-out **branch** graphlet (G3) anchored on `node`, in donor
    /// graph `from`'s session graphlet index. No new graph or session — the branch
    /// shares the donor's `GraphId` + kernel nodes (brief §4.2); it diverges only in this
    /// graphlet's lineage (Phase 2). Loads the donor's index from its sidecar on first
    /// touch, records the branch, and persists immediately (graphlets change only on a
    /// structural op like this, so the per-session save path need not carry them yet).
    /// Returns the new `GraphletId` the torn window carries, or `None` if the donor has
    /// no session manifest. (Graphlet wiring Phase 1.)
    pub(super) fn branch_graphlet_from(
        &mut self,
        node: uuid::Uuid,
        from: GraphId,
    ) -> Option<forme::GraphletId> {
        let session_dir = self.graph_session_dir(from)?;
        // Load-or-default the donor's graphlet index into the pool, mint the branch,
        // persist. The window carries the returned id.
        let graphlets = self
            .graphlets
            .entry(from)
            .or_insert_with(|| crate::graphlets::SessionGraphlets::load(&session_dir));
        let id = graphlets.record_branch(node, crate::graphlets::default_spec_for(node));
        if let Err(err) = graphlets.save(&session_dir) {
            tracing::warn!(%err, dir = ?session_dir, "failed to persist the branch graphlet");
        }
        self.shared.observability.record_probe(
            "tear_out",
            "branch",
            format!("node={node} graphlet={id}"),
        );
        Some(id)
    }

    /// Mint a **Linked** `Component` graphlet (Phase 3 slice 2) seeded on `node`, derived
    /// from graph `from`, persisted, returning its id for a scoped window to carry. Unlike
    /// a branch (a hand-built roster), its members are *derived* from the graph and
    /// reconcile on drift. The caller opens a window scoped to it. `None` if the donor
    /// graph or its session is gone. (Graphlet wiring Phase 3 — the manual Linked
    /// consumer.)
    pub(super) fn linked_component_graphlet(
        &mut self,
        node: uuid::Uuid,
        from: GraphId,
    ) -> Option<forme::GraphletId> {
        let session_dir = self.graph_session_dir(from)?;
        // Clone the graph as the derivation source (a borrow-split is a refinement; the
        // component BFS is cheap).
        let graph = self.orreries.get(&from)?.graph().clone();
        let spec = forme::GraphletSpec {
            kind: forme::GraphletKind::Component,
            anchors: vec![node.to_string()],
            primary_anchor: Some(node.to_string()),
            selectors: Vec::new(),
        };
        let graphlets = self
            .graphlets
            .entry(from)
            .or_insert_with(|| crate::graphlets::SessionGraphlets::load(&session_dir));
        let id = graphlets.record_linked(&graph, spec);
        if let Err(err) = graphlets.save(&session_dir) {
            tracing::warn!(%err, dir = ?session_dir, "failed to persist the linked graphlet");
        }
        self.shared.observability.record_probe(
            "graphlet",
            "linked-component",
            format!("node={node} graphlet={id}"),
        );
        Some(id)
    }

    /// Reconcile graph `graph`'s Linked graphlets against the current graph and persist any
    /// that drifted (Phase 3 slice 2+ — data-level drift). Queued by `save_session` after a
    /// mutation. A no-op when nothing drifted (the diff returns empty). (Graphlet wiring.)
    pub(super) fn reconcile_linked_graphlets(&mut self, graph: GraphId) {
        let Some(session_dir) = self.graph_session_dir(graph) else {
            return;
        };
        let src = match self.orreries.get(&graph) {
            Some(o) => o.graph().clone(),
            None => return,
        };
        if let Some(idx) = self.graphlets.get_mut(&graph) {
            if idx.reconcile_all(&src) {
                if let Err(err) = idx.save(&session_dir) {
                    tracing::warn!(%err, dir = ?session_dir, "failed to persist reconciled graphlets");
                }
            }
        }
    }

    /// Grow a branch graphlet's roster (Phase 2 slice 2): the branch window navigated to
    /// `node`, so it joins graphlet `graphlet`'s lineage in graph `graph`'s index.
    /// Persists on a real change (dedup'd, so revisits are cheap no-ops). No-op if the
    /// index or graphlet is gone. (Tear-out gestures G3.)
    pub(super) fn record_branch_member(
        &mut self,
        graph: GraphId,
        graphlet: forme::GraphletId,
        node: uuid::Uuid,
    ) {
        let Some(session_dir) = self.graph_session_dir(graph) else {
            return;
        };
        if let Some(graphlets) = self.graphlets.get_mut(&graph) {
            if graphlets.add_member(graphlet, node) {
                if let Err(err) = graphlets.save(&session_dir) {
                    tracing::warn!(%err, dir = ?session_dir, "failed to persist the branch member");
                }
                self.shared.observability.record_probe(
                    "tear_out",
                    "branch-grow",
                    format!("graphlet={graphlet} node={node}"),
                );
            }
        }
    }

    /// The storage dir of the session graph `graph` belongs to (manifests are keyed by
    /// session; each carries its `root_graph_id`). The Shell-side counterpart of
    /// [`WindowCtx::session_for_graph`], for the graphlet pool ops that run on `Shell`.
    fn graph_session_dir(&self, graph: GraphId) -> Option<std::path::PathBuf> {
        self.shared.session.manifests.iter().find_map(|(id, m)| {
            (m.root_graph_id == graph).then(|| {
                m.storage_path.clone().unwrap_or_else(|| {
                    self.shared
                        .session
                        .mere_root
                        .join("sessions")
                        .join(id.as_uuid().to_string())
                })
            })
        })
    }

    /// Switch the active session to `target`: persist the current one, then load the
    /// target's graph + camera + frame. No-op if already active or unknown. (MG2.)
    pub(super) fn switch_session(&mut self, target: SessionId) {
        if target == self.shared.session.active_session_id
            || self.shared.session.manifests.get(target).is_none()
        {
            return;
        }
        self.ctx().save_session();
        let session_dir = self
            .shared
            .session
            .mere_root
            .join("sessions")
            .join(target.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        self.load_active_session(target, session_dir, false);
    }

    /// Switch to the next (`forward`) or previous session in id order, wrapping.
    /// No-op with fewer than two sessions. (MG2.)
    pub(super) fn cycle_session(&mut self, forward: bool) {
        let mut ids: Vec<SessionId> =
            self.shared.session.manifests.iter().map(|(id, _)| id).collect();
        if ids.len() < 2 {
            return;
        }
        ids.sort_by_key(|id| *id.as_uuid());
        let Some(pos) = ids.iter().position(|id| *id == self.shared.session.active_session_id)
        else {
            return;
        };
        let next = if forward {
            (pos + 1) % ids.len()
        } else {
            (pos + ids.len() - 1) % ids.len()
        };
        self.switch_session(ids[next]);
    }

    /// Close (trash) the `target` session. Refuses to close the last one. If it was
    /// active, switches to the most-recently-updated survivor first. (MG3.)
    pub(super) fn close_session(&mut self, target: SessionId) {
        if self.shared.session.manifests.len() <= 1
            || self.shared.session.manifests.get(target).is_none()
        {
            return;
        }
        // The deleted session's graph, captured before `move_to_trash` removes the
        // manifest. Its pooled state + windows are torn down below: a donor's branch /
        // leaf windows die with it (brief §4.2), while forks (their own `GraphId`)
        // survive. (Graphlet wiring #3 / tear-out G6.)
        let dead_graph = self
            .shared
            .session
            .manifests
            .get(target)
            .map(|m| m.root_graph_id);
        if target == self.shared.session.active_session_id {
            let survivor = self
                .shared
                .session
                .manifests
                .iter()
                .filter(|(id, _)| *id != target)
                .max_by_key(|(_, m)| m.updated_at)
                .map(|(id, _)| id);
            if let Some(next) = survivor {
                self.switch_session(next);
            }
        }
        if let Err(err) = self.shared.session.manifests.move_to_trash(target) {
            tracing::warn!(%err, "failed to trash the closed session");
        }
        // Tear down the dead graph: close its secondary windows first (so none outlives
        // its orrery), then drop its pooled orrery + graphlet index. The on-disk
        // `graphlets.json` already went to `.trash` with the session dir above.
        if let Some(graph) = dead_graph {
            self.close_windows_on_graph(graph);
            self.graphlets.remove(&graph);
            self.orreries.remove(&graph);
            self.orrery_lru.retain(|g| *g != graph);
        }
        self.focused_view_mut().renaming = None;
        self.ctx().refresh_session_thumbnails();
        self.focused_view_mut().request_redraw();
    }

    /// Make `id` (whose dir is `session_dir`) the active session: load its graph +
    /// camera + frame and reset the prior session's runtime caches. `fresh` marks a
    /// just-minted empty session. Re-keys the focused orrery to the target graph in
    /// the pool (the Shell-only step a WindowCtx cannot do); the rest runs on the
    /// re-entered ctx, which now resolves that re-keyed orrery. (MG2.)
    fn load_active_session(&mut self, id: SessionId, session_dir: std::path::PathBuf, fresh: bool) {
        // The target graph (empty for a fresh session, or a missing/corrupt file).
        let graph = if fresh {
            Graph::new()
        } else {
            session_graph_store::load(&session_dir.join(session_graph_store::GRAPH_FILE))
                .ok()
                .flatten()
                .unwrap_or_else(Graph::new)
        };
        let empty = graph.nodes().count() == 0;
        let target_graph = self
            .shared
            .session
            .manifests
            .get(id)
            .map(|m| m.root_graph_id)
            .unwrap_or_default();
        // Pool the target graph's orrery (Shell-level — a WindowCtx cannot insert
        // pool entries), keeping the *outgoing* graph's orrery live so two graphs
        // coexist. A graph shown for the first time mints a fresh orrery from its
        // loaded graph and offloads its own physics actor (like the seed did at
        // boot); switching back to a still-pooled graph reuses its live orrery —
        // its state was kept warm, so disk is not reloaded over it. The outgoing
        // graph is parked (its content actors reaped per-graph below); its orrery
        // stays in the pool.
        let old_gid = self.focused_view().focused_graph;
        self.pool_orrery(target_graph, graph, empty);
        // Pool this session's graphlet index alongside its orrery (load-or-default from
        // the sidecar; a fresh session gets the default whole-session graphlet). Kept
        // warm like the orrery — a switch-back reuses the live index. Eviction parity
        // with the orrery LRU is a refinement (the index is tiny). (Graphlet wiring P1.)
        self.graphlets
            .entry(target_graph)
            .or_insert_with(|| crate::graphlets::SessionGraphlets::load(&session_dir));
        self.focused_view_mut().focused_graph = target_graph;

        // Park the outgoing graph's physics (OQ2 park): a switched-away graph stays
        // warm in the pool, but stop it settling so it does not keep ticking and
        // waking the loop in the background — its actor then idles on its channel.
        // Switching back shows its settled positions; interaction resumes the settle.
        if old_gid != target_graph {
            if let Some(parked) = self.orreries.get_mut(&old_gid) {
                parked.park_physics();
            }
        }
        self.touch_and_evict(target_graph);

        // The remainder runs on the focused window's ctx (now resolving the re-keyed
        // orrery): restore camera + focus, retag the graph-bound leaves, reset the
        // prior session's runtime caches, and swap identity.
        let restored_view =
            view_intent_store::load_view_intent(&session_dir, DEFAULT_FRAME, DEFAULT_PANE)
                .ok()
                .flatten();
        let mut ctx = self.ctx();
        match restored_view.as_ref().and_then(|v| v.camera.as_ref()) {
            Some(snapshot) => {
                ctx.orrery_mut().set_camera(super::snapshot_to_camera(snapshot));
                let (yaw, tilt) = super::snapshot_yaw_tilt(snapshot);
                ctx.orrery_mut().set_yaw(yaw);
                ctx.orrery_mut().set_tilt(tilt);
                ctx.view.centered = true;
            }
            None => ctx.view.centered = false,
        }
        if let Some(url) = restored_view.as_ref().and_then(|v| v.focus.as_deref()) {
            ctx.orrery_mut().select_by_url(url);
        }
        // Restore the pane's layout strategy (None = force-directed). (Layout picker.)
        ctx.orrery_mut().set_layout_strategy(restored_view.as_ref().and_then(|v| v.strategy.clone()));
        // Restore live workbench-mirror mode; its scope re-derives from the open tiles
        // through the render loop, so the flag is all that needs to come back. (Mirror.)
        ctx.view.mirror_tiles = restored_view.as_ref().is_some_and(|v| v.mirror_tiles);
        // Re-point only the panes that were on the outgoing graph, so a second
        // graph-pane pinned to a different graph survives the switch. (Pane-as-unit.)
        ctx.view.frame_layout.retag_graph_bound_from(old_gid, target_graph);
        // Switching toward a graph already shown beside would leave two panes on it;
        // collapse the duplicate so it doesn't render blank. (Pane-as-unit.)
        ctx.view.frame_layout.dedupe_graph_panes();
        ctx.shared.content.constellation.reap_graph(old_gid);
        ctx.view.scrying.clear();
        ctx.shared.content.engine_pins.clear();
        ctx.view.scrying_input_focus = None;
        ctx.view.scrying_rects.clear();
        ctx.shared.content.pages.clear();
        ctx.view.tile_textures.clear();
        ctx.view.snapshot_data_uris.clear();
        ctx.view.scroll.clear();
        ctx.view.focused_tile = None;
        ctx.view.shown_location = None;
        ctx.view.renaming = None;
        // Restore this graph's persisted tiling instead of wiping to an empty
        // workbench (the split shape / tabs / active tab now survive a restart). (A3.)
        ctx.view.workbench = ctx.restore_workbench(&session_dir);
        // Restore the orrery's settled layout from the cartography sidecar, overriding
        // the graph's load-time seed so the spatial view comes back as it was left
        // rather than re-scrambling. (Position sidecar.)
        let present: std::collections::HashSet<forme::GraphMemberId> =
            ctx.orrery().graph().nodes().map(|(_, node)| node.id).collect();
        if let Some(geom) = load_cartography(&session_dir, &present) {
            ctx.orrery_mut().seed_cartography(geom.iter());
            // Restore the importance metric first, so the sizing restore recomputes with it.
            ctx.orrery_mut().apply_cartography_importance_metric(geom.importance_metric());
            // Restore the per-node sizes + the size-by-degree / size-by-importance flags. (Graph signals.)
            ctx.orrery_mut().apply_cartography_sizing(
                geom.size_iter(),
                geom.size_by_degree(),
                geom.size_by_importance(),
            );
            // Restore the custom sprite faces, so a textured node re-opens textured. (Node-rep.)
            ctx.orrery_mut().apply_cartography_sprites(geom.sprite_iter());
            // ...and their collider hulls, so the traced-to-image collider survives too. (Node-rep.)
            ctx.orrery_mut().apply_cartography_sprite_hulls(geom.sprite_hull_iter());
            // ...and the per-node physical materials, so a tuned node re-opens tuned. (Body & face.)
            ctx.orrery_mut().apply_cartography_materials(geom.material_iter());
            // ...and the face overrides LAST (after sprites), so the chosen face wins. (Body & face.)
            ctx.orrery_mut().apply_cartography_faces(geom.face_iter());
        }
        ctx.view.maximized_pane = None;
        ctx.view.active_content = super::ContentPane::Orrery;
        ctx.shared.session.session_dir = session_dir;
        ctx.shared.session.active_session_id = id;
        ctx.view.content_location =
            ctx.orrery().focused_url().unwrap_or("mere://welcome").to_string();
        ctx.refresh_session_thumbnails();
        ctx.view.request_redraw();
    }

    /// Pool `graph`'s orrery under `graph_id` if it isn't already pooled, minting
    /// it from the loaded graph with its own offloaded physics actor (an empty one
    /// opens on the welcome node, like first launch). A no-op when the graph is
    /// already live — its pooled orrery is authoritative, so disk is not reloaded
    /// over it. Touches no focus: both session-switch and open-graph-beside pool
    /// through here. (Window composition P2.)
    fn pool_orrery(&mut self, graph_id: GraphId, graph: Graph, empty: bool) {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.orreries.entry(graph_id) {
            let mut orrery = orrery::Orrery::with_graph(graph);
            if empty {
                orrery.visit("mere://welcome");
            }
            orrery.offload_physics(self.physics_wake.clone());
            slot.insert(orrery);
        }
    }

    /// Mark `graph_id` most-recently-used, then drop the stalest pooled orreries
    /// over the cap that **no pane in any window resolves to** — not just each
    /// window's focused graph, so a second graph-pane's orrery is not evicted out
    /// from under it. Dropping an orrery ends its physics actor and reaps its
    /// content actors; the graph was saved on its last switch-away, so eviction
    /// loses no data (switching / re-opening reloads it). (Window composition P2,
    /// OQ2 unload — now pane-aware.)
    fn touch_and_evict(&mut self, graph_id: GraphId) {
        self.orrery_lru.retain(|g| *g != graph_id);
        self.orrery_lru.push(graph_id);
        if self.orreries.len() <= super::MAX_POOLED_ORRERIES {
            return;
        }
        let live: std::collections::HashSet<GraphId> = self
            .windows
            .values()
            .chain(self.pending_view.iter())
            .flat_map(|v| {
                std::iter::once(v.focused_graph)
                    .chain(v.frame_layout.iter_leaves().map(|(_, _, gid)| gid))
            })
            .collect();
        while self.orreries.len() > super::MAX_POOLED_ORRERIES {
            let Some(stale) = self.orrery_lru.iter().copied().find(|g| !live.contains(g)) else {
                break;
            };
            self.orrery_lru.retain(|g| *g != stale);
            self.orreries.remove(&stale); // drop → physics actor thread ends
            self.shared.content.constellation.reap_graph(stale);
        }
    }

    /// Open session `id`'s graph in a second Orrery pane beside the current one
    /// **without switching focus**: pool its orrery (cold-loading from disk if it
    /// isn't live) and summon an Orrery leaf bound to its `graph_id`, split beside
    /// the primary graph pane. The two graphs then render side by side, each
    /// driving its own pooled orrery (the P2 per-pane render path). No-op if the
    /// session is unknown or its graph already shows in this window. (Window
    /// composition P2 — second graph-pane.)
    pub(super) fn open_graph_beside(&mut self, id: SessionId) {
        let Some(graph_id) = self.shared.session.manifests.get(id).map(|m| m.root_graph_id) else {
            return;
        };
        // Already shown beside in the focused window? Don't double-summon.
        if self
            .focused_view()
            .frame_layout
            .iter_leaves()
            .any(|(_, c, gid)| matches!(c, frame::PaneContent::Orrery) && gid == graph_id)
        {
            return;
        }
        let session_dir = self
            .shared
            .session
            .mere_root
            .join("sessions")
            .join(id.as_uuid().to_string());
        let graph = session_graph_store::load(&session_dir.join(session_graph_store::GRAPH_FILE))
            .ok()
            .flatten()
            .unwrap_or_else(Graph::new);
        let empty = graph.nodes().count() == 0;
        self.pool_orrery(graph_id, graph, empty);
        self.touch_and_evict(graph_id);
        // Summon a second Orrery leaf bound to this graph, split right of the
        // primary graph pane (an even split — two graphs share the band).
        let view = self.focused_view_mut();
        let pane_id = frame::PaneId(view.next_pane_id);
        view.next_pane_id += 1;
        let leaf = frame::PaneNode::Leaf {
            pane_id,
            content: frame::PaneContent::Orrery,
            graph_id,
        };
        let anchor =
            super::frame_view::pane_path(&view.frame_layout, super::GRAPH_PANE).unwrap_or_default();
        if view
            .frame_layout
            .summon_leaf(&anchor, frame::InsertSide::Right, leaf)
        {
            view.frame_layout.set_split_ratio(&anchor, 0.5);
        }
        view.maximized_pane = None;
        view.request_redraw();
        self.shared
            .observability
            .record_frame_layout_changed("second graph pane opened");
    }

    /// Thaw a graph engram (by its manifest id, as the Alembic Engrams row carries it) into a
    /// fresh, ephemeral Orrery pane beside the current one, read-only: browsing an engram does
    /// not mutate it, and the thawed graph pools under its own `GraphId` with no session manifest,
    /// so it is not persisted unless re-saved (editing forks a thaw — the immutable-engram model).
    /// No-op if the id is unparseable, the store is absent, or no engram is stored under it.
    /// (Alembic memory pane — open an engram, slice B2.)
    pub(super) fn open_engram_beside(&mut self, id_str: &str) {
        let Some(id) = eidetic::Hash::parse(id_str)
            .ok()
            .map(eidetic::ManifestId::from_hash)
        else {
            return;
        };
        // Thaw off the private store; the borrow ends here, before the orrery-pool borrow.
        let Some(store) = self.shared.content.store.as_mut() else {
            return;
        };
        let Some(graph) =
            pollster::block_on(session_runtime::graph_engram::open_engram_as_session(store, id))
                .ok()
                .flatten()
        else {
            return;
        };
        // An engram is not a session, so it pools under a fresh, ephemeral graph id.
        let graph_id = GraphId::new();
        let empty = graph.nodes().count() == 0;
        self.pool_orrery(graph_id, graph, empty);
        self.touch_and_evict(graph_id);
        // Summon an Orrery leaf bound to the thawed graph, split right of the primary graph pane
        // (an even split), without switching focus — the same shape as `open_graph_beside`.
        let view = self.focused_view_mut();
        let pane_id = frame::PaneId(view.next_pane_id);
        view.next_pane_id += 1;
        let leaf = frame::PaneNode::Leaf {
            pane_id,
            content: frame::PaneContent::Orrery,
            graph_id,
        };
        let anchor =
            super::frame_view::pane_path(&view.frame_layout, super::GRAPH_PANE).unwrap_or_default();
        if view
            .frame_layout
            .summon_leaf(&anchor, frame::InsertSide::Right, leaf)
        {
            view.frame_layout.set_split_ratio(&anchor, 0.5);
        }
        view.maximized_pane = None;
        view.request_redraw();
        self.shared
            .observability
            .record_frame_layout_changed("engram opened beside");
    }
}

/// A short switcher label for a session with no user-set display name: the first
/// non-intro node's cached host (else its title), or "New" for an empty /
/// welcome-only graph. (Host text path.)
fn derive_session_label(graph: &Graph) -> String {
    graph
        .nodes()
        .map(|(_, node)| node)
        .find(|node| node.url() != "mere://welcome")
        .and_then(|node| {
            node.cached_host
                .clone()
                .filter(|h| !h.trim().is_empty())
                .or_else(|| Some(node.title.clone()))
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "New".to_string())
}
