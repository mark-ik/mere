/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shell session lifecycle: create/fork/branch/switch/cycle/close.

use super::*;

impl crate::Shell {
    /// Mint a fresh, empty session and make it active (persisting the current one
    /// first). The new session opens on the welcome node. Returns its id. (MG3.)
    pub(crate) fn create_session(&mut self) -> SessionId {
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
    pub(crate) fn fork_session_from(&mut self, node: uuid::Uuid, from: GraphId) -> Option<GraphId> {
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
        let mut fork_orrery = mere::orrery::Orrery::with_graph(fork_graph);
        fork_orrery.set_current_session(self.shared.session.current_session_count);
        self.orreries.insert(graph_id, fork_orrery);
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
    pub(crate) fn branch_graphlet_from(
        &mut self,
        node: uuid::Uuid,
        from: GraphId,
    ) -> Option<mere::forme::GraphletId> {
        let session_dir = self.graph_session_dir(from)?;
        // Load-or-default the donor's graphlet index into the pool, mint the branch,
        // persist. The window carries the returned id.
        let graphlets = self
            .graphlets
            .entry(from)
            .or_insert_with(|| mere::graphlets::SessionGraphlets::load(&session_dir));
        let id = graphlets.record_branch(node, mere::graphlets::default_spec_for(node));
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

    /// Mint a **Linked** graphlet (Phase 3 slice 2) of `kind` seeded on `node`, derived from
    /// graph `from` under the `selectors` edge projection, persisted, returning its id for a
    /// scoped window to carry. Unlike a branch (a hand-built roster), its members are
    /// *derived* from the graph and reconcile on drift. The caller opens a window scoped to
    /// it. `None` if the donor graph or its session is gone. (Graphlet wiring Phase 3 — the
    /// manual Linked consumer; `kind` + `selectors` are the projection-vocabulary control.)
    pub(crate) fn linked_graphlet(
        &mut self,
        node: uuid::Uuid,
        from: GraphId,
        kind: mere::forme::GraphletKind,
        selectors: Vec<String>,
    ) -> Option<mere::forme::GraphletId> {
        let session_dir = self.graph_session_dir(from)?;
        // Clone the graph as the derivation source (a borrow-split is a refinement; the
        // BFS derivation is cheap).
        let graph = self.orreries.get(&from)?.graph().clone();
        let spec = mere::forme::GraphletSpec {
            kind,
            anchors: vec![node.to_string()],
            primary_anchor: Some(node.to_string()),
            selectors,
        };
        let graphlets = self
            .graphlets
            .entry(from)
            .or_insert_with(|| mere::graphlets::SessionGraphlets::load(&session_dir));
        let id = graphlets.record_linked(&graph, spec);
        if let Err(err) = graphlets.save(&session_dir) {
            tracing::warn!(%err, dir = ?session_dir, "failed to persist the linked graphlet");
        }
        self.shared.observability.record_probe(
            "graphlet",
            "linked",
            format!("node={node} graphlet={id}"),
        );
        Some(id)
    }

    /// Crystallize graph `from`'s current multi-selection into a **Session** graphlet (P3b): classify
    /// the selection's induced subgraph, freeze the selected nodes as a graphlet tagged with the
    /// dominant shape kind, persist, and scope that orrery to the frozen set in place (reconciliation
    /// ruling 1 — scope the one Navigator, no new window). Returns `(kind, count)` for a note, or
    /// `None` when fewer than two nodes are selected. (Swatch primitive — P3b crystallize.)
    pub(crate) fn crystallize_selection(
        &mut self,
        from: GraphId,
    ) -> Option<(mere::forme::GraphletKind, usize)> {
        let session_dir = self.graph_session_dir(from)?;
        // Selection + dominant shape (a read-only borrow, dropped before the mutations below).
        let (members, kind) = {
            let orrery = self.orreries.get(&from)?;
            let members = orrery.selected_members();
            if members.len() < 2 {
                return None;
            }
            let kind = crate::graphlet_classifier::classify_selection(orrery.graph(), &members)
                .first()?
                .kind
                .clone();
            (members, kind)
        };
        // Freeze the selection as a Session graphlet tagged with the kind, persist.
        let graphlets = self
            .graphlets
            .entry(from)
            .or_insert_with(|| mere::graphlets::SessionGraphlets::load(&session_dir));
        let id = graphlets.record_session(kind.clone(), members.clone());
        if let Err(err) = graphlets.save(&session_dir) {
            tracing::warn!(%err, dir = ?session_dir, "failed to persist the crystallized graphlet");
        }
        // Scope the orrery to the frozen set, in place (ruling 1). The next frame shows the crop.
        if let Some(orrery) = self.orreries.get_mut(&from) {
            orrery.scope_to_members(members.iter().copied());
        }
        self.shared.observability.record_probe(
            "graphlet",
            "crystallize",
            format!("kind={kind:?} count={} graphlet={id}", members.len()),
        );
        Some((kind, members.len()))
    }

    /// Reconcile graph `graph`'s Linked graphlets against the current graph and persist any
    /// that drifted (Phase 3 slice 2+ — data-level drift). Queued by `save_session` after a
    /// mutation. A no-op when nothing drifted (the diff returns empty). (Graphlet wiring.)
    pub(crate) fn reconcile_linked_graphlets(&mut self, graph: GraphId) {
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

    pub(crate) fn reconcile_linked_graphlet(
        &mut self,
        graph: GraphId,
        graphlet: mere::forme::GraphletId,
    ) {
        let Some(session_dir) = self.graph_session_dir(graph) else {
            return;
        };
        let src = match self.orreries.get(&graph) {
            Some(o) => o.graph().clone(),
            None => return,
        };
        if let Some(idx) = self.graphlets.get_mut(&graph)
            && idx.reconcile(&src, graphlet).is_some()
            && let Err(err) = idx.save(&session_dir)
        {
            tracing::warn!(%err, dir = ?session_dir, "failed to persist reconciled graphlet");
        }
    }

    pub(crate) fn keep_graphlet_as_session(&mut self, graph: GraphId, graphlet: mere::forme::GraphletId) {
        let Some(session_dir) = self.graph_session_dir(graph) else {
            return;
        };
        if let Some(idx) = self.graphlets.get_mut(&graph)
            && idx.keep_as_session(graphlet)
            && let Err(err) = idx.save(&session_dir)
        {
            tracing::warn!(%err, dir = ?session_dir, "failed to persist unlinked graphlet");
        }
    }

    pub(crate) fn toggle_graphlet_family_selector(
        &mut self,
        graph: GraphId,
        graphlet: mere::forme::GraphletId,
        family: mere::kernel::graph::EdgeFamily,
    ) {
        let Some(session_dir) = self.graph_session_dir(graph) else {
            return;
        };
        if let Some(idx) = self.graphlets.get_mut(&graph)
            && idx.toggle_family_selector(graphlet, family)
            && let Err(err) = idx.save(&session_dir)
        {
            tracing::warn!(%err, dir = ?session_dir, "failed to persist graphlet selector change");
        }
    }

    pub(crate) fn branch_existing_graphlet(
        &mut self,
        graph: GraphId,
        graphlet: mere::forme::GraphletId,
    ) -> Option<mere::forme::GraphletId> {
        let session_dir = self.graph_session_dir(graph)?;
        let idx = self
            .graphlets
            .entry(graph)
            .or_insert_with(|| mere::graphlets::SessionGraphlets::load(&session_dir));
        let id = idx.branch_from_graphlet(graphlet)?;
        if let Err(err) = idx.save(&session_dir) {
            tracing::warn!(%err, dir = ?session_dir, "failed to persist branched graphlet");
        }
        Some(id)
    }

    /// Grow a branch graphlet's roster (Phase 2 slice 2): the branch window navigated to
    /// `node`, so it joins graphlet `graphlet`'s lineage in graph `graph`'s index.
    /// Persists on a real change (dedup'd, so revisits are cheap no-ops). No-op if the
    /// index or graphlet is gone. (Tear-out gestures G3.)
    pub(crate) fn record_branch_member(
        &mut self,
        graph: GraphId,
        graphlet: mere::forme::GraphletId,
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
    pub(crate) fn graph_session_dir(&self, graph: GraphId) -> Option<std::path::PathBuf> {
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
    pub(crate) fn switch_session(&mut self, target: SessionId) {
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
    pub(crate) fn cycle_session(&mut self, forward: bool) {
        let mut ids: Vec<SessionId> = self
            .shared
            .session
            .manifests
            .iter()
            .map(|(id, _)| id)
            .collect();
        if ids.len() < 2 {
            return;
        }
        ids.sort_by_key(|id| *id.as_uuid());
        let Some(pos) = ids
            .iter()
            .position(|id| *id == self.shared.session.active_session_id)
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
    pub(crate) fn close_session(&mut self, target: SessionId) {
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
        self.ctx().refresh_session_labels();
        self.focused_view_mut().request_redraw();
    }
}
