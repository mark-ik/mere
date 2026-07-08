/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! WindowCtx frame tree: graph/pane/orrery accessors, workbench, comms.

use super::*;

impl WindowCtx<'_> {
    // ── Frame tree (F1) ──────────────────────────────────────────────────────

    /// The active session's root graph id (the `GraphId` graph-bound leaves carry).
    /// Falls back to a fresh id if the active manifest is somehow missing. (MG5.)
    pub(crate) fn active_graph_id(&self) -> GraphId {
        self.shared
            .session
            .manifests
            .get(self.shared.session.active_session_id)
            .map(|m| m.root_graph_id)
            .unwrap_or_default()
    }

    /// The `graph_id` a freshly-summoned leaf of `content` should carry: the
    /// **focused** pane's graph for graph-bound panes (so a pane summoned beside the
    /// one you're in shows your graph), a nil (unbound) id for window-chrome. Was the
    /// active session's graph; now keyed off `focused_graph` directly — equal today,
    /// but the pane-as-unit choice once focus can leave the active session. (MG5;
    /// Window composition — pane-as-unit.)
    pub(crate) fn leaf_graph_id(&self, content: &PaneContent) -> GraphId {
        if content.follows_active_graph() {
            self.view.focused_graph
        } else {
            GraphId::nil()
        }
    }

    /// The focused window's orrery, resolved from the pool by the window's tracked
    /// `focused_graph`. The read half of the bundled `self.orrery` field P1
    /// removed: P2 hands the ctx the whole pool so render / input can resolve any
    /// pane's orrery, and the window-focused-graph sites reach the focused one
    /// here. Resolution-identical to P1's bundling key. (Window composition P2.)
    pub(crate) fn orrery(&self) -> &Orrery {
        self.orreries
            .get(&self.view.focused_graph)
            .expect("focused orrery is pooled")
    }

    /// The focused window's orrery, mutable — the write half of the removed
    /// `self.orrery` field. (Window composition P2.)
    pub(crate) fn orrery_mut(&mut self) -> &mut Orrery {
        let gid = self.view.focused_graph;
        self.orreries
            .get_mut(&gid)
            .expect("focused orrery is pooled")
    }

    /// A specific pooled orrery by `graph_id`, mutable — the per-pane resolution a
    /// render / hit-test drives the orrery a pane resolves to with. Panics if the
    /// graph isn't pooled, which a laid-out leaf's graph always is. (Window
    /// composition P2.)
    pub(crate) fn pane_orrery_mut(&mut self, graph_id: GraphId) -> &mut Orrery {
        self.orreries
            .get_mut(&graph_id)
            .expect("a laid-out pane's graph is pooled")
    }

    /// Read twin of [`pane_orrery_mut`](Self::pane_orrery_mut). (Window
    /// composition P2.)
    pub(crate) fn pane_orrery(&self, graph_id: GraphId) -> &Orrery {
        self.orreries
            .get(&graph_id)
            .expect("a laid-out pane's graph is pooled")
    }

    /// Whether the tiled-workbench pane is open (a Workbench leaf exists).
    pub(crate) fn workbench_open(&self) -> bool {
        self.pane_of_content(&PaneContent::Workbench).is_some()
    }

    /// Whether the workbench is the active content pane (open + last-interacted) —
    /// so navigation (omnibar / Ctrl+Enter / Back-Forward) targets its focused tile
    /// rather than the orrery's selected node. (Workbench-as-pane.)
    pub(crate) fn workbench_active(&self) -> bool {
        self.view.active_content == crate::ContentPane::Workbench && self.workbench_open()
    }

    /// Summon the workbench pane beside the orrery (Tree projection) and make it the
    /// active content pane. Idempotent — only summons when not already open.
    pub(crate) fn open_workbench(&mut self) {
        if !self.workbench_open() {
            self.view.workbench.ensure_tiled();
            let id = PaneId(self.view.next_pane_id);
            self.view.next_pane_id += 1;
            let graph_id = self.leaf_graph_id(&PaneContent::Workbench);
            let leaf = PaneNode::Leaf {
                pane_id: id,
                content: PaneContent::Workbench,
                graph_id,
            };
            let anchor = frame_view::pane_path(&self.view.frame_layout, crate::GRAPH_PANE)
                .unwrap_or_default();
            if self
                .view
                .frame_layout
                .summon_leaf(&anchor, InsertSide::Right, leaf)
            {
                self.view.frame_layout.set_split_ratio(&anchor, 0.6);
            }
            self.view.maximized_pane = None;
            self.shared
                .observability
                .record_pane_toggle(&PaneContent::Workbench, true);
            self.shared
                .observability
                .record_frame_layout_changed("workbench opened");
        }
        self.view.active_content = crate::ContentPane::Workbench;
    }

    /// Close the workbench pane: reap its tiles' actors, clear the tiles, drop the
    /// pane leaf, and hand focus back to the orrery.
    pub(crate) fn close_workbench(&mut self) {
        self.persist_workbench_boundary_thumbnails();
        for member in self.view.workbench.open_members() {
            self.shared.content.constellation.reap(member);
        }
        self.view.workbench.clear_tiles();
        if let Some(id) = self.pane_of_content(&PaneContent::Workbench) {
            if let Some(path) = frame_view::pane_path(&self.view.frame_layout, id) {
                self.view.frame_layout.close_leaf(&path);
            }
        }
        self.view.focused_tile = None;
        self.view.active_content = crate::ContentPane::Orrery;
        self.view.maximized_pane = None;
        self.shared
            .observability
            .record_pane_toggle(&PaneContent::Workbench, false);
        self.shared
            .observability
            .record_frame_layout_changed("workbench closed");
    }

    /// Keep a Comms frame leaf in sync with the chrome's comms-open state. The comms
    /// pane is chrome-rendered (its compose field + click handlers live in the
    /// chrome), so it stays there; this only reserves / drops a frame leaf so the
    /// other panes make room, and the render positions the chrome overlay into it.
    /// (Comms pane.)
    pub(crate) fn sync_comms_pane(&mut self) {
        let open = self.chrome().comms.is_open();
        match (open, self.pane_of_content(&PaneContent::Comms)) {
            (true, None) => {
                let id = PaneId(self.view.next_pane_id);
                self.view.next_pane_id += 1;
                let graph_id = self.leaf_graph_id(&PaneContent::Comms);
                let leaf = PaneNode::Leaf {
                    pane_id: id,
                    content: PaneContent::Comms,
                    graph_id,
                };
                let anchor = frame_view::pane_path(&self.view.frame_layout, crate::GRAPH_PANE)
                    .unwrap_or_default();
                if self
                    .view
                    .frame_layout
                    .summon_leaf(&anchor, InsertSide::Right, leaf)
                {
                    self.view.frame_layout.set_split_ratio(&anchor, 0.66);
                }
                self.view.maximized_pane = None;
                self.shared
                    .observability
                    .record_pane_toggle(&PaneContent::Comms, true);
                self.shared
                    .observability
                    .record_frame_layout_changed("comms opened");
            }
            (false, Some(id)) => {
                if let Some(path) = frame_view::pane_path(&self.view.frame_layout, id) {
                    self.view.frame_layout.close_leaf(&path);
                }
                self.view.maximized_pane = None;
                self.shared
                    .observability
                    .record_pane_toggle(&PaneContent::Comms, false);
                self.shared
                    .observability
                    .record_frame_layout_changed("comms closed");
            }
            _ => {}
        }
    }

    /// Toggle a sibling pane (roster / apparatus / …): close it if its content is
    /// open, else summon it as a right split beside the graph pane (the graph keeps
    /// the larger share). Anchoring at the graph leaf — not the root — lets a
    /// second pane nest rather than fail against a non-leaf root. A layout change
    /// clears any maximize. (Frame tree, F1.)
    pub(crate) fn toggle_pane(&mut self, content: PaneContent) {
        let recorded_content = content.clone();
        let mut opened = false;
        if let Some(id) = self.pane_of_content(&content) {
            if let Some(path) = frame_view::pane_path(&self.view.frame_layout, id) {
                self.view.frame_layout.close_leaf(&path);
            }
        } else {
            let id = PaneId(self.view.next_pane_id);
            self.view.next_pane_id += 1;
            let graph_id = self.leaf_graph_id(&content);
            let leaf = PaneNode::Leaf {
                pane_id: id,
                content,
                graph_id,
            };
            let anchor =
                frame_view::pane_path(&self.view.frame_layout, GRAPH_PANE).unwrap_or_default();
            if self
                .view
                .frame_layout
                .summon_leaf(&anchor, InsertSide::Right, leaf)
            {
                self.view.frame_layout.set_split_ratio(&anchor, 0.7);
            }
            opened = true;
        }
        self.view.maximized_pane = None;
        self.shared
            .observability
            .record_pane_toggle(&recorded_content, opened);
        self.shared
            .observability
            .record_frame_layout_changed(format!(
                "{} {}",
                recorded_content.tag(),
                if opened { "opened" } else { "closed" }
            ));
        self.view.request_redraw();
    }

    /// Read-only system rows for the apparatus Overview section.
    pub(crate) fn apparatus_system_rows(&self) -> Vec<(String, String)> {
        vec![
            (
                "Nodes".to_string(),
                self.orrery().graph().nodes().count().to_string(),
            ),
            (
                "Active actors".to_string(),
                self.shared.content.constellation.active_count().to_string(),
            ),
            (
                "Tab cap".to_string(),
                self.shared.presentation.saved_tab_cap.to_string(),
            ),
            (
                "Theme".to_string(),
                self.shared.presentation.active_theme_id.clone(),
            ),
            (
                "Uptime".to_string(),
                self.shared.observability.snapshot().uptime,
            ),
        ]
    }

    /// Live apparatus table stats, owned by the apparatus surface rather than by `platen`.
    pub(crate) fn apparatus_table_stats(&self) -> Vec<crate::apparatus::ApparatusTableStat> {
        use crate::apparatus::ApparatusTableStat;

        let stats = self.orrery().graph().table_stats();
        let log = &self.shared.session.graph_delta_log;
        let mut rows = vec![
            ApparatusTableStat::present("Node table", "kernel", stats.node_count as u64, "rows"),
            ApparatusTableStat::present(
                "Relation table",
                "kernel",
                stats.relation_count as u64,
                "relations",
            )
            .with_detail(format!("across {} links", stats.edge_count)),
            ApparatusTableStat::present("Field table", "kernel", stats.field_count as u64, "rows"),
            ApparatusTableStat::present(
                "Coupling table",
                "kernel",
                stats.coupling_count as u64,
                "rows",
            ),
            ApparatusTableStat::present(
                "History table",
                "kernel",
                stats.history_owner_count as u64,
                "owners",
            )
            .with_detail(format!(
                "{} entries / {} visits",
                stats.history_entry_count, stats.history_visit_count
            )),
        ];
        rows.push(if log.enabled() {
            ApparatusTableStat::present(
                "Session delta log",
                "capture",
                log.entry_count(),
                "entries",
            )
            .with_estimated_bytes(log.byte_count())
            .with_session_deltas(log.entry_count())
        } else {
            ApparatusTableStat::unavailable("Session delta log", "capture", "not recording")
        });
        match self.focused_member() {
            Some(member) => {
                let constellation = &self.shared.content.constellation;
                let Some(engine_id) = constellation.engine_id(member) else {
                    rows.push(ApparatusTableStat::unavailable(
                        "Document tables",
                        "engine",
                        "focused node is not active",
                    ));
                    return rows;
                };
                match constellation.engine_stats(member) {
                    Some(engine) => {
                        rows.push(
                            ApparatusTableStat::present(
                                "Document DOM",
                                "engine",
                                engine.dom.live_nodes as u64,
                                "nodes",
                            )
                            .with_estimated_bytes(engine.dom.estimated_bytes as u64)
                            .with_detail(format!(
                                "{} attrs; {} el / {} text / {} comments / {} docs",
                                engine.dom.attribute_count,
                                engine.dom.node_kinds.elements,
                                engine.dom.node_kinds.text,
                                engine.dom.node_kinds.comments,
                                engine.dom.node_kinds.documents
                            )),
                        );
                        rows.push(match engine.layout {
                            Some(layout) => ApparatusTableStat::present(
                                "Document layout",
                                "engine",
                                layout.fragment_count as u64,
                                "fragments",
                            )
                            .with_last_dirty_set_size(layout.restyled_elements as u64)
                            .with_detail(format!(
                                "{:?} / {:?}; {} rebuilt",
                                layout.applied, layout.damage, layout.boxes_rebuilt
                            )),
                            None => ApparatusTableStat::unavailable(
                                "Document layout",
                                "engine",
                                "layout batch stats unavailable on this lane",
                            ),
                        });
                    }
                    None => {
                        rows.push(ApparatusTableStat::unavailable(
                            "Document DOM",
                            "engine",
                            format!("not available on current lane ({engine_id})"),
                        ));
                        rows.push(ApparatusTableStat::unavailable(
                            "Document layout",
                            "engine",
                            format!("not available on current lane ({engine_id})"),
                        ));
                    }
                }
                match constellation.scene_stats(member) {
                    Some(scene) => {
                        rows.push(
                            ApparatusTableStat::present(
                                "Document scene",
                                "scene",
                                scene.op_count as u64,
                                "ops",
                            )
                            .with_estimated_bytes(scene.encoded_bytes as u64),
                        );
                    }
                    None if constellation.packet(member).is_some() => {
                        rows.push(ApparatusTableStat::unavailable(
                            "Document scene",
                            "scene",
                            "not available on document lane",
                        ))
                    }
                    None if constellation.scene(member).is_some() => {
                        rows.push(ApparatusTableStat::unavailable(
                            "Document scene",
                            "scene",
                            "scene stats missing for current frame",
                        ))
                    }
                    None => rows.push(ApparatusTableStat::unavailable(
                        "Document scene",
                        "scene",
                        format!("awaiting first scene on current lane ({engine_id})"),
                    )),
                }
            }
            None => rows.push(ApparatusTableStat::unavailable(
                "Document tables",
                "engine",
                "no focused node",
            )),
        }
        rows
    }

    /// Refresh and snapshot the host observability cache for Apparatus.
    pub(crate) fn apparatus_observability(&mut self) -> ObservabilitySnapshot {
        self.refresh_a11y_summary();
        self.shared.observability.snapshot()
    }

    /// Toggle maximize of the pane under the cursor (full-screen and back). With a
    /// single pane this is a no-op visually. (Frame tree, F1.)
    pub(crate) fn toggle_maximize(&mut self) {
        if self.view.maximized_pane.is_some() {
            self.view.maximized_pane = None;
        } else if let Some(pane) = self.pane_at(self.view.cursor.0, self.view.cursor.1) {
            self.view.maximized_pane = Some(pane);
        }
        self.view.request_redraw();
    }

    /// Whether more than one graph (Orrery) pane is open — gates the close-pane
    /// affordances (you can't close the last graph view). (Window composition —
    /// pane-as-unit.)
    pub(crate) fn has_multiple_graph_panes(&self) -> bool {
        self.view
            .frame_layout
            .iter_leaves()
            .filter(|(_, c, _)| matches!(c, PaneContent::Orrery))
            .count()
            > 1
    }

    /// Close the focused graph (Orrery) pane when more than one is open — the
    /// dismiss for a second graph-pane (Ctrl+W, the `close_pane` command, the orrery
    /// context menu). A no-op with a single graph pane (never close the last graph
    /// view). Focus hands off to a surviving graph pane; the closed graph's orrery
    /// stays pooled (the LRU evicts it later). (Window composition — pane-as-unit.)
    pub(crate) fn close_focused_graph_pane(&mut self) {
        let orrery_panes: Vec<(PaneId, GraphId)> = self
            .view
            .frame_layout
            .iter_leaves()
            .filter(|(_, c, _)| matches!(c, PaneContent::Orrery))
            .map(|(id, _, gid)| (id, gid))
            .collect();
        if orrery_panes.len() <= 1 {
            return;
        }
        // The focused graph pane (else the first one), and a survivor to focus next.
        let focused = self.view.focused_graph;
        let (target_pane, _) = orrery_panes
            .iter()
            .find(|(_, g)| *g == focused)
            .copied()
            .unwrap_or(orrery_panes[0]);
        if let Some(path) = frame_view::pane_path(&self.view.frame_layout, target_pane) {
            self.view.frame_layout.close_leaf(&path);
        }
        self.view.maximized_pane = None;
        if let Some((_, surviving)) = orrery_panes.iter().find(|(p, _)| *p != target_pane) {
            self.focus_pane_graph(*surviving);
        }
        self.shared
            .observability
            .record_frame_layout_changed("graph pane closed");
        self.view.request_redraw();
    }
}
