/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shell accessors: window ctx, orrery, focused view, window-view builders.

use super::*;

impl Shell {
    /// Borrow the **primary** window as a handling context. Before `resumed` keys the
    /// primary in, it falls back to the `pending_view` — so the headless test harness
    /// (which never resumes) and the `new()`-time bootstrap both still resolve a view.
    /// (MW2 (d).)
    pub(crate) fn ctx(&mut self) -> WindowCtx<'_> {
        let view = match self.primary {
            Some(id) => self
                .windows
                .get_mut(&id)
                .expect("primary window missing from registry"),
            None => self
                .pending_view
                .as_mut()
                .expect("a primary or pending view"),
        };
        // Pass the whole pool; the ctx resolves the focused (or a per-pane) orrery
        // by `graph_id`. (Window composition P2; was a single bundled orrery, P1.)
        let pool_count = self.orreries.len();
        let mut wc = WindowCtx {
            view,
            multi: &mut self.multi,
            shared: &mut self.shared,
            orreries: &mut self.orreries,
            graphlets: &self.graphlets,
            clipboard: &mut self.clipboard,
            a11y_bridge: &mut self.a11y_bridge,
            a11y_action_routes: &mut self.a11y_action_routes,
            render_core: self.render_core.as_ref(),
            commands: &mut self.commands,
            orrery_pool_count: pool_count,
            branch_scope_restore: None,
        };
        // Install this window's per-pane cameras + selection into the shared orreries
        // for the pass, and a branch window's graphlet scope; the ctx's `Drop` reads them
        // back. (View state on the view.)
        wc.install_viewports();
        wc.install_selections();
        wc.install_scope();
        wc
    }

    /// Read-only borrow of the primary window's view (registry entry, else the
    /// pending bootstrap view). For read paths that don't need the full ctx — the
    /// agent harness's `&Shell` closures. (MW2 (d).)
    #[cfg(any(test, feature = "agent-harness"))]
    pub(crate) fn view(&self) -> &window_view::WindowView {
        match self.primary {
            Some(id) => &self.windows[&id],
            None => self
                .pending_view
                .as_ref()
                .expect("a primary or pending view"),
        }
    }

    /// This window's local view-state, read-only, resolved from the shared multi-runner by
    /// the focused (primary/pending) view's `projection_id` — the read path for the
    /// harness's `&Shell` assertions, which can't build a `WindowCtx`. (Slice 3.)
    #[cfg(any(test, feature = "agent-harness"))]
    pub(crate) fn window_local(&self) -> &window_view::WindowLocal {
        &self.multi.state().windows[self.view().projection_id.0]
    }

    /// The focused window's orrery resolved from the pool — for the read/write paths
    /// that reach the orrery off `Shell` directly (the agent harness; the per-window
    /// `WindowCtx` bundles it as `self.orrery`). (Window composition P1.)
    #[cfg(any(test, feature = "agent-harness"))]
    pub(crate) fn orrery(&self) -> &Canvas {
        let gid = self.view().focused_graph;
        self.orreries.get(&gid).expect("focused orrery is pooled")
    }

    #[cfg(any(test, feature = "agent-harness"))]
    pub(crate) fn orrery_mut(&mut self) -> &mut Canvas {
        let gid = self.view().focused_graph;
        self.orreries
            .get_mut(&gid)
            .expect("focused orrery is pooled")
    }

    /// Borrow window `id` as a handling context: its view from the registry plus the
    /// shared state and shell singletons the active window drives. `None` if no such
    /// window. The construction is the per-window seam — a ctx reaches exactly one
    /// view, never the registry or its siblings. (MW2 (d).)
    pub(crate) fn window_ctx(&mut self, id: WindowId) -> Option<WindowCtx<'_>> {
        let view = self.windows.get_mut(&id)?;
        let pool_count = self.orreries.len();
        // Resolve this window's AccessKit bridge: the primary keeps the long-standing
        // `a11y_bridge` field (so its path and the harness are unchanged); a secondary
        // (leaf) gets its own, minted on first access with the shared wake proxy. (MW3
        // step 6 — per-window a11y.)
        let a11y_bridge = if Some(id) == self.primary {
            &mut self.a11y_bridge
        } else {
            let proxy = self.a11y_proxy.clone();
            self.secondary_a11y_bridges
                .entry(id)
                .or_insert_with(move || {
                    a11y_bridge::AccessKitBridge::new(move || {
                        let _ = proxy.send_event(());
                    })
                })
        };
        let mut wc = WindowCtx {
            view,
            multi: &mut self.multi,
            shared: &mut self.shared,
            orreries: &mut self.orreries,
            graphlets: &self.graphlets,
            clipboard: &mut self.clipboard,
            a11y_bridge,
            a11y_action_routes: &mut self.a11y_action_routes,
            render_core: self.render_core.as_ref(),
            commands: &mut self.commands,
            orrery_pool_count: pool_count,
            branch_scope_restore: None,
        };
        // Install this window's per-pane cameras + selection + branch scope for the pass;
        // `Drop` reads them back. (View state on the view.)
        wc.install_viewports();
        wc.install_selections();
        wc.install_scope();
        Some(wc)
    }

    /// The focused window's view (primary, or the pending bootstrap view) — the same
    /// primary-or-pending resolution `ctx()` uses, for the Shell-level session ops.
    /// Unlike `ctx()` it touches only the view, so it is valid mid-re-key when the
    /// focused orrery is not yet pooled under the new graph id. (Window composition
    /// P1, multi-graph.)
    pub(crate) fn focused_view_mut(&mut self) -> &mut window_view::WindowView {
        match self.primary {
            Some(id) => self
                .windows
                .get_mut(&id)
                .expect("primary window missing from registry"),
            None => self
                .pending_view
                .as_mut()
                .expect("a primary or pending view"),
        }
    }

    /// Read-only twin of [`Self::focused_view_mut`] — reads the focused graph id
    /// before a re-key without holding a mutable borrow.
    pub(crate) fn focused_view(&self) -> &window_view::WindowView {
        match self.primary {
            Some(id) => &self.windows[&id],
            None => self
                .pending_view
                .as_ref()
                .expect("a primary or pending view"),
        }
    }

    /// Mint a fresh per-window view over the shared session: its own chrome +
    /// workbench runners (a second pair of genet document authorities) and a default
    /// single-orrery content frame bound to the active graph. The view-session bits
    /// start at rest (no restored camera / frame); a spawned window opens on the
    /// shared graph the way the primary first did. The caller (`SpawnWindow`) creates
    /// the OS window + surface around it. (Multi-window MW3.)
    pub(crate) fn build_window_view(&mut self) -> window_view::WindowView {
        let active_graph = self
            .shared
            .session
            .manifests
            .get(self.shared.session.active_session_id)
            .map(|m| m.root_graph_id)
            .unwrap_or_default();
        self.build_window_view_for(active_graph)
    }

    /// As [`build_window_view`](Self::build_window_view), but binds the new view to
    /// `bind_graph` rather than the active session's root graph — a **fork** opens its
    /// window onto a freshly pooled graph while the active session stays on the donor.
    /// (Tear-out gestures G4.)
    pub(crate) fn build_window_view_for(&mut self, bind_graph: GraphId) -> window_view::WindowView {
        let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        let mut chrome = Chrome::new("mere://welcome");
        chrome.settings.tab_cap = self.shared.presentation.saved_tab_cap;
        chrome.slim = true; // a spawned window is a leaf: slim chrome (no shellbar / switcher)
        let content_location = chrome.content_location().to_string();
        // Mint this window's local view-state and push its projection into the shared
        // multi-runner: append the `WindowLocal` first (so `windows[i]` exists for the
        // projection's logic to read), then push a projection whose logic lenses `shell_view`
        // onto index `i`. The leaf reads the one shared chrome (crawl chip) with no
        // per-window copy and no fan-out — a second window is a second projection over the
        // one state. `push_projection` returns `ProjectionId(i)` since both vecs only ever
        // append (a close tombstones, never pops), keeping the index aligned. (One state, N
        // windows — Slice 3.)
        let i = self.multi.state().windows.len();
        let local = window_view::WindowLocal::new(chrome);
        self.multi.update(|app| app.windows.push(local));
        let projection_id = self.multi.push_projection(
            dom.clone(),
            Box::new(move |app| window_view::shell_view(app, i)),
        );
        let mut view = window_view::WindowView::new(
            window_view::WindowKind::Leaf,
            bind_graph,
            dom,
            projection_id,
            Workbench::new(),
        );
        view.content_location = content_location;
        view.frame_layout = default_content_frame(bind_graph);
        view.next_pane_id = 1;
        view
    }

    /// Build a torn-out **leaf** view (G2): a workbench-only window on the donor's pooled
    /// graph `bind_graph`, with `node` opened as its single tile and **no orrery pane of its
    /// own**. The tile resolves to the same pooled orrery the donor shows, so node edits
    /// propagate both ways and closing the leaf does not delete the node (brief §4.1).
    /// (Tear-out gestures G2.)
    pub(crate) fn build_leaf_view_for(
        &mut self,
        bind_graph: GraphId,
        node: uuid::Uuid,
    ) -> window_view::WindowView {
        let mut view = self.build_window_view_for(bind_graph);
        // Swap the default orrery frame for a single Workbench pane and open the torn node
        // as its tile, focused.
        view.frame_layout = leaf_workbench_frame(bind_graph);
        view.workbench.ensure_tiled();
        view.workbench.open_tile(node);
        view.active_content = crate::ContentPane::Workbench;
        view.focused_tile = Some(node);
        view
    }
}
