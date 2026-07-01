/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shell window-management ops: create/close/spawn/copy/apply.

use super::*;

impl Shell {
    /// Wake every non-primary window to repaint. The actor-drain in `user_event` writes
    /// shared state through the primary's ctx; the secondaries show the same pooled
    /// orreries, caches, and physics, so they redraw on the same wake. A no-op while only
    /// the primary exists. (Multi-window MW3 step 5 — the `user_event` redraw fan-out.)
    pub(crate) fn redraw_secondary_windows(&self) {
        for (id, view) in self.windows.iter() {
            if Some(*id) != self.primary {
                view.request_redraw();
            }
        }
    }

    /// Create an OS window for `view`, attach a swapchain surface from the shared
    /// (already-booted) render core, show it, and key it into the registry. Returns
    /// the new window's id + handle, or `None` if window / surface creation failed.
    /// Shared by `resumed` (the primary) and `spawn_window` (secondaries), so both
    /// windows are minted the same way — the seam that makes a second window cheap.
    /// (MW3: one device, N surfaces.)
    pub(crate) fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        mut view: crate::window_view::WindowView,
    ) -> Option<(WindowId, Arc<Window>)> {
        // Borderless: the OS title bar (and its accent border) is off; the chrome's
        // toolbar band is the titlebar, with host-drawn window controls + edge
        // resize (see `titlebar` + `input`). A min size keeps the bar usable.
        let attributes = Window::default_attributes()
            .with_title("Meerkat — Mere chrome on serval")
            .with_decorations(false)
            .with_visible(false)
            // Logical sizing so the on-screen window is the same size at any DPI; winit
            // makes the physical surface `logical × scale_factor`, and the chrome scales
            // to match (Auto-DPI D1). `inner_size()` below then reports physical px.
            .with_min_inner_size(LogicalSize::new(480u32, 320u32))
            .with_inner_size(LogicalSize::new(view.width, view.height));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                eprintln!("[meerkat] window creation failed: {err}");
                return None;
            }
        };
        let size = window.inner_size();
        view.width = size.width.max(1);
        view.height = size.height.max(1);
        // This window's display DPI (its monitor's scale_factor). Stored per-window
        // (D3); the shared chrome sheet is (re)built to it now so this window's first
        // frame is correct, and the render guard re-syncs the sheet if another window on
        // a different-density monitor renders. (Auto-DPI D1 → D3.)
        let dpi = window.scale_factor() as f32;
        view.dpi_scale = dpi;
        if (self.shared.presentation.dpi_scale - dpi).abs() > 1e-3 {
            self.shared.presentation.dpi_scale = dpi;
            self.shared.presentation.rebuild_chrome_sheet();
        }
        let surface = match self
            .render_core
            .as_ref()
            .expect("render core booted before window creation")
            .create_surface(window.clone(), view.width, view.height)
        {
            Ok(surface) => surface,
            Err(err) => {
                eprintln!("[meerkat] {err}");
                return None;
            }
        };
        view.surface = Some(surface);
        view.window = Some(Arc::clone(&window));
        // Allow the platform IME so composed input (CJK, transliteration, dead-key
        // accents) is delivered as `WindowEvent::Ime` and routed into the focused
        // chrome field; `set_ime_cursor_area` then follows the caret. (G2.1 IME.)
        window.set_ime_allowed(true);
        let id = window.id();
        self.windows.insert(id, view);
        // The window is left hidden: on Windows the AccessKit adapter must be created
        // before the window is first shown, so the caller installs a11y (the primary)
        // or skips it (a secondary) and then calls `set_visible(true)`. (MW3.)
        Some((id, window))
    }

    /// Drain and apply the queued cross-window commands. Called from `about_to_wait`
    /// once the per-window ctx borrows that queued them have ended. (MW3, the
    /// deferred MW2 (e).)
    pub(crate) fn apply(&mut self, event_loop: &ActiveEventLoop) {
        if self.commands.is_empty() {
            return;
        }
        for command in std::mem::take(&mut self.commands) {
            match command {
                crate::ShellCommand::SpawnWindow => self.spawn_window(event_loop),
                crate::ShellCommand::TearOut { node, from } => {
                    self.spawn_torn_window(event_loop, node, from)
                }
                crate::ShellCommand::CopyNodeAcross { node, from, to } => {
                    self.copy_node_across(node, from, to)
                }
                crate::ShellCommand::MoveNodeAcross { node, from, to } => {
                    self.move_node_across(node, from, to)
                }
                crate::ShellCommand::ForkNode { node, from } => {
                    // Mint the fork session + pooled graph (Shell), then open a window
                    // bound to it (needs the event loop). The render core is booted by
                    // the time any gesture fires, but guard like `spawn_window`.
                    if self.render_core.is_some() {
                        if let Some(graph_id) = self.fork_session_from(node, from) {
                            let view = self.build_window_view_for(graph_id);
                            self.spawn_window_with_view(event_loop, view);
                        }
                    }
                }
                crate::ShellCommand::BranchNode { node, from } => {
                    // Mint + persist the branch graphlet in the donor's session (Shell),
                    // then open a window on the donor's *same* graph, scoped to it.
                    if self.render_core.is_some() {
                        if let Some(graphlet_id) = self.branch_graphlet_from(node, from) {
                            // The anchor node's label, for the branch chrome chip (P2).
                            let anchor_label = self.orreries.get(&from).and_then(|o| {
                                let g = o.graph();
                                g.get_node_by_id(node)
                                    .map(|(key, _)| g.node_display_label(key))
                            });
                            let mut view = self.build_window_view_for(from);
                            view.branch_graphlet = Some(graphlet_id);
                            if let Some(label) = anchor_label {
                                view.chrome_update(|c| {
                                    c.branch_label = Some(format!("\u{2387} {label}"))
                                });
                            }
                            self.spawn_window_with_view(event_loop, view);
                        }
                    }
                }
                crate::ShellCommand::RecordBranchMember {
                    graph,
                    graphlet,
                    node,
                } => self.record_branch_member(graph, graphlet, node),
                crate::ShellCommand::ReconcileGraphlets { graph } => {
                    self.reconcile_linked_graphlets(graph)
                }
                crate::ShellCommand::ReconcileGraphlet { graph, graphlet } => {
                    self.reconcile_linked_graphlet(graph, graphlet)
                }
                crate::ShellCommand::KeepGraphletAsSession { graph, graphlet } => {
                    self.keep_graphlet_as_session(graph, graphlet)
                }
                crate::ShellCommand::ToggleGraphletFamilySelector {
                    graph,
                    graphlet,
                    family,
                } => self.toggle_graphlet_family_selector(graph, graphlet, family),
                crate::ShellCommand::BranchGraphlet { graph, graphlet } => {
                    if self.render_core.is_some() {
                        if let Some(branch_id) = self.branch_existing_graphlet(graph, graphlet) {
                            let label = self.graphlet_window_label(graph, branch_id, "branch");
                            let mut view = self.build_window_view_for(graph);
                            view.branch_graphlet = Some(branch_id);
                            if let Some(label) = label {
                                view.chrome_update(|c| c.branch_label = Some(label));
                            }
                            self.spawn_window_with_view(event_loop, view);
                        }
                    }
                }
                crate::ShellCommand::OpenExistingGraphlet { graph, graphlet } => {
                    if self.render_core.is_some()
                        && self
                            .graphlets
                            .get(&graph)
                            .and_then(|idx| idx.get(graphlet))
                            .is_some()
                    {
                        let label = self.graphlet_window_label(graph, graphlet, "graphlet");
                        let mut view = self.build_window_view_for(graph);
                        view.branch_graphlet = Some(graphlet);
                        if let Some(label) = label {
                            view.chrome_update(|c| c.branch_label = Some(label));
                        }
                        self.spawn_window_with_view(event_loop, view);
                    }
                }
                crate::ShellCommand::OpenLinkedGraphlet {
                    node,
                    from,
                    kind,
                    selectors,
                    chip,
                } => {
                    // Mint the Linked graphlet of `kind` under `selectors` (Shell), then open
                    // a window scoped to it (reusing the branch-window scope path). (Phase 3
                    // slice 2 / 2+.)
                    if self.render_core.is_some() {
                        if let Some(graphlet_id) = self.linked_graphlet(node, from, kind, selectors)
                        {
                            let anchor_label = self.orreries.get(&from).and_then(|o| {
                                let g = o.graph();
                                g.get_node_by_id(node)
                                    .map(|(key, _)| g.node_display_label(key))
                            });
                            let mut view = self.build_window_view_for(from);
                            view.branch_graphlet = Some(graphlet_id);
                            if let Some(label) = anchor_label {
                                view.chrome_update(|c| {
                                    c.branch_label = Some(format!("\u{25ce} {chip}: {label}"))
                                });
                            }
                            self.spawn_window_with_view(event_loop, view);
                        }
                    }
                }
                crate::ShellCommand::CrystallizeSelection { from } => {
                    // Freeze the multi-selection into a Session graphlet + scope the orrery to it
                    // in place (ruling 1 — not a new window). (Swatch primitive — P3b crystallize.)
                    if let Some((kind, count)) = self.crystallize_selection(from) {
                        tracing::info!(?kind, count, "crystallized the selection into a graphlet");
                    }
                }
                crate::ShellCommand::CloseWindow(id) => self.close_window(id),
                crate::ShellCommand::CreateSession => {
                    self.create_session();
                }
                crate::ShellCommand::SwitchSession(id) => self.switch_session(id),
                crate::ShellCommand::CycleSession(forward) => self.cycle_session(forward),
                crate::ShellCommand::CloseSession(id) => self.close_session(id),
                crate::ShellCommand::OpenGraphBeside(id) => self.open_graph_beside(id),
                crate::ShellCommand::OpenEngramBeside(id) => self.open_engram_beside(&id),
            }
        }
    }

    /// Tear a node out into a new **leaf** window (G1/G2): a workbench-only window on the
    /// donor graph `from` showing `node`'s tile, with no orrery pane of its own (built by
    /// [`build_leaf_view_for`](Self::build_leaf_view_for)). The tile resolves to the shared
    /// pooled orrery, so node edits propagate to the donor and closing the leaf does not
    /// delete the node. (Tear-out gestures G1/G2.)
    pub(crate) fn spawn_torn_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        node: uuid::Uuid,
        from: crate::GraphId,
    ) {
        if self.render_core.is_none() {
            return;
        }
        self.shared.observability.record_probe(
            "tear_out",
            "leaf",
            format!("node={node} from={}", from.as_uuid()),
        );
        let view = self.build_leaf_view_for(from, node);
        self.spawn_window_with_view(event_loop, view);
    }

    /// Cross-graph copy (G5): mint a copy of `node` (from graph `from`) in graph `to`,
    /// with `CopiedFrom` provenance back to the source. The source is cloned out of the
    /// donor before the destination is mutated (two pool entries; the clone breaks the
    /// aliasing). Placed at the origin for v0 (physics settles it; a drop-point placement
    /// is a refinement). (Tear-out gestures G5.)
    pub(crate) fn copy_node_across(
        &mut self,
        node: uuid::Uuid,
        from: crate::GraphId,
        to: crate::GraphId,
    ) {
        let source = self
            .orreries
            .get(&from)
            .and_then(|o| o.graph().get_node_by_id(node).map(|(_, n)| n.clone()));
        let Some(source) = source else { return };
        let copied = if let Some(dest) = self.orreries.get_mut(&to) {
            dest.ingest_graph(|g| {
                g.copy_node_from_xy(&source, Some(from.as_uuid().to_string()), 0.0, 0.0);
                true
            })
        } else {
            false
        };
        if copied {
            self.shared.observability.record_probe(
                "tear_out",
                "copy",
                format!("node={node} to={}", to.as_uuid()),
            );
            // The destination graph changed; repaint every window showing it.
            self.ctx().view.request_redraw();
            self.redraw_secondary_windows();
        }
    }

    /// Cross-graph move (G5): copy `node` into `to` (reusing [`copy_node_across`]), then
    /// release it from the source graph `from`. Unlike `delete_focused_node` this is a
    /// *relocation*, not a user delete, so it records no eidetic tombstone; it does reap the
    /// node's source-side activation (it re-activates if focused in `to`). The release uses
    /// the same `remove_node` + `reconcile_derived` path as `remove_focused`, targeted by
    /// uuid. (Tear-out gestures G5 — move.)
    pub(crate) fn move_node_across(
        &mut self,
        node: uuid::Uuid,
        from: crate::GraphId,
        to: crate::GraphId,
    ) {
        // Copy first (it reads the node out of `from`), then drop it from the source.
        self.copy_node_across(node, from, to);
        let released = if let Some(src) = self.orreries.get_mut(&from) {
            src.ingest_graph(|g| match g.get_node_by_id(node) {
                Some((key, _)) => g.remove_node(key),
                None => false,
            })
        } else {
            false
        };
        if released {
            self.shared.content.constellation.reap(node);
            self.shared.observability.record_probe(
                "tear_out",
                "move",
                format!("node={node} to={}", to.as_uuid()),
            );
            // The source graph shrank; repaint every window showing it.
            self.ctx().view.request_redraw();
            self.redraw_secondary_windows();
        }
    }

    /// Open a second OS window over the shared session (Cmd/Ctrl+Shift+N → a queued
    /// `SpawnWindow`). The render core is already booted (the primary did it on the
    /// first resume), so the new view shares the graph, actors, and caches, differing
    /// only in its own chrome + surface + frame. It is a slim workbench-only leaf
    /// (`WindowKind::Leaf` + slim chrome — MW3 step 4, set in [`build_window_view`]).
    ///
    /// Live actor-driven updates reach it through the `user_event` fan-out (redraw for
    /// shared graph/content changes, plus chrome writes once it carries chrome — MW3
    /// step 5). The secondary still has no AccessKit bridge — the bridge is shell-
    /// singular, installed against the primary — so per-window a11y is MW3 step 6.
    ///
    /// [`build_window_view`]: Shell::build_window_view
    pub(crate) fn spawn_window(&mut self, event_loop: &ActiveEventLoop) {
        // Can't happen via the keyboard verb (it's post-resume), but guard anyway.
        if self.render_core.is_none() {
            return;
        }
        let view = self.build_window_view();
        self.spawn_window_with_view(event_loop, view);
    }

    /// Create the OS window + surface around an already-built `view`, install its
    /// per-window AccessKit bridge (before first show), and reveal it. Shared by the
    /// plain spawn ([`spawn_window`](Self::spawn_window)) and a **fork**, which passes a
    /// fork-graph-bound view from
    /// [`build_window_view_for`](Shell::build_window_view_for). (Multi-window MW3;
    /// tear-out gestures G4.)
    pub(crate) fn spawn_window_with_view(
        &mut self,
        event_loop: &ActiveEventLoop,
        view: crate::window_view::WindowView,
    ) {
        if let Some((id, window)) = self.create_window(event_loop, view) {
            // Install this secondary's own AccessKit bridge *before* showing it (the
            // "adapter before first show" rule, same order the primary uses in
            // `resumed`): build its a11y projection through its ctx, then install the
            // adapter against its window. `window_ctx` mints the per-window bridge on
            // first access. (MW3 step 6 — per-window a11y.)
            if let Some(wc) = self.window_ctx(id) {
                let projection = wc.build_a11y_projection().tree_update();
                match wc.a11y_bridge.install(&window, projection) {
                    Ok(()) => wc.shared.observability.record_probe(
                        "a11y_bridge",
                        "installed",
                        "secondary-window AccessKit bridge installed",
                    ),
                    Err(err) => wc.shared.observability.record_probe(
                        "a11y_bridge",
                        "degraded",
                        format!("secondary AccessKit bridge unavailable: {err}"),
                    ),
                }
            }
            window.set_visible(true);
            // Take keyboard focus on the freshly-spawned window too (same focus-on-launch
            // reason as the primary): a torn-out window the user just created should be ready
            // for input without a click.
            window.focus_window();
            window.request_redraw();
            self.shared.observability.record_probe(
                "multi_window",
                "spawned",
                format!("windows={}", self.windows.len()),
            );
        }
    }

    /// Close a secondary window: drop its view, which releases its surface and OS
    /// window. The primary is exempt — its close saves the session and exits the app
    /// through [`Shell::request_close`]. (MW3.)
    pub(crate) fn close_window(&mut self, id: WindowId) {
        if Some(id) == self.primary {
            return;
        }
        if self.windows.remove(&id).is_some() {
            // Drop this secondary's AccessKit bridge with its window (MW3 step 6) — its
            // adapter is subclassed onto the now-gone OS window. The primary's bridge is
            // the separate `a11y_bridge` field and is untouched.
            self.secondary_a11y_bridges.remove(&id);
            self.shared.observability.record_probe(
                "multi_window",
                "closed",
                format!("windows={}", self.windows.len()),
            );
        }
    }

    fn graphlet_window_label(
        &self,
        graph: crate::GraphId,
        graphlet: forme::GraphletId,
        fallback: &str,
    ) -> Option<String> {
        let g = self.graphlets.get(&graph)?.get(graphlet)?;
        let kind = g
            .kind
            .as_ref()
            .map(|kind| format!("{kind:?}"))
            .unwrap_or_else(|| fallback.to_string());
        let anchor = g.primary_anchor.or_else(|| g.anchors.first().copied());
        let anchor_label = anchor.and_then(|node| {
            self.orreries.get(&graph).and_then(|o| {
                let graph = o.graph();
                graph
                    .get_node_by_id(node)
                    .map(|(key, _)| graph.node_display_label(key))
            })
        });
        Some(match anchor_label {
            Some(label) => format!("{kind}: {label}"),
            None => format!("{kind} #{graphlet}"),
        })
    }

    /// Close every *secondary* window whose focused graph is `graph`. The primary is
    /// exempt (a session-delete caller has already switched it to a survivor). Used when a
    /// session is deleted: its graph's **branch + leaf** windows die with it (brief §4.2),
    /// while **forks** — which live on their own `GraphId` — survive untouched. More
    /// generally this is the multi-window half of session-delete: a window must not outlive
    /// the graph it shows. (Tear-out gestures G6.)
    pub(crate) fn close_windows_on_graph(&mut self, graph: crate::GraphId) {
        let doomed: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(id, view)| Some(**id) != self.primary && view.focused_graph == graph)
            .map(|(id, _)| *id)
            .collect();
        for id in doomed {
            self.close_window(id);
        }
    }

    /// Honor a close request for `window_id`, forked by role: the primary saves its
    /// session and exits the app; a secondary just drops its view, leaving the graph
    /// and the other windows intact. Both `CloseRequested` and the custom close
    /// control route here. (MW3.)
    pub(crate) fn request_close(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId) {
        if Some(window_id) == self.primary {
            if let Some(mut wc) = self.window_ctx(window_id) {
                wc.save_session();
            }
            event_loop.exit();
        } else {
            self.close_window(window_id);
        }
    }
}
