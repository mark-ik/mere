/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! WindowCtx config ops: settings, UI scale, menu actions, physics.

use super::*;

impl WindowCtx<'_> {
    /// Make the focused node's content available. A network address already in
    /// this session's content map is left as-is; otherwise a durable cache hit is
    /// shown without re-fetching (so a reload need not hit the network), and a
    /// miss marks it `Loading` and spawns a fetch.
    pub(crate) fn ensure_content(&mut self, url: &str) {
        // Local knot notes are authored, not fetched: the content is the node's own
        // `Node.body` (slice 3), so produce a Ready `text/x-knot` state from it and let
        // the normal render path (DjotKnotEngine) build the document — the same path a
        // fetched page takes, no special-casing downstream. (Slice 2 — the local-knot
        // producer.)
        //
        // TODO(co-op browsing): a `knot://` address authored by a *peer* resolves over
        // the federation / sync layer, not the local graph — the later networked-notes
        // problem. Local authorship is all this branch covers.
        if url.starts_with("knot://") {
            if !self.shared.content.pages.contains_key(url) {
                let body = self
                    .orrery()
                    .graph()
                    .get_node_by_url(url)
                    .and_then(|(_, node)| node.body.clone())
                    .filter(|b| !b.is_empty())
                    .unwrap_or_else(|| {
                        // A fresh note opens with a starter so it is not blank; the
                        // editor replaces it with the real body once it writes one
                        // (slice 4). The title is the address path.
                        let name = url.strip_prefix("knot://").unwrap_or("note");
                        format!("# {name}\n\nA new knot note. Start writing.\n")
                    });
                self.shared.content.pages.insert(
                    url.to_string(),
                    fetch::ContentState::Ready(fetch::Fetched {
                        content_type: Some("text/x-knot".to_string()),
                        body,
                    }),
                );
            }
            return;
        }
        if !fetch::is_fetchable(url) || self.shared.content.pages.contains_key(url) {
            return;
        }
        if let Some(stored) = self.load_cached(url) {
            let fetched = crate::fetched_from(stored);
            // A cached page skips the network `FetchUpdate::Page` favicon discovery,
            // so do it here when the node still has no favicon (a first-time cache
            // hit, or after a host change cleared the old one). (Favicon-on-tile.)
            let needs_favicon = self
                .orrery()
                .graph()
                .get_node_by_url(url)
                .is_none_or(|(_, node)| node.favicon_rgba.is_none());
            if needs_favicon {
                if let Some(icon_url) = crate::app_handler::favicon_url_for(url, &fetched.body) {
                    self.shared
                        .content
                        .fetch_handle
                        .command(fetch::FetchCommand::Favicon {
                            owner_url: url.to_string(),
                            url: icon_url,
                        });
                }
            }
            self.shared
                .content
                .pages
                .insert(url.to_string(), fetch::ContentState::Ready(fetched));
            return;
        }
        self.shared
            .content
            .pages
            .insert(url.to_string(), fetch::ContentState::Loading);
        self.shared
            .observability
            .record_actor("fetch", "started", Some(url.to_string()));
        self.shared
            .content
            .fetch_handle
            .command(fetch::FetchCommand::Page(url.to_string()));
    }

    /// Toggle between the orrery (Cartography) and the tiled workbench (Tree).
    /// Entering Tree seeds the open set from the focused node and its graph
    /// neighbors, so the tiled view reflects the node you toggled on; exiting
    /// clears it. The constellation reconciles its actors to the resulting needed
    /// set on the next frame — spawning the tiles, reaping what's no longer shown
    /// (background-flagged nodes excepted).
    pub(crate) fn toggle_workbench(&mut self) {
        // Clear the omnibar suggestions dropdown so it doesn't hang over the tiles.
        self.view.chrome_update(Chrome::close_suggestions);
        if self.workbench_open() {
            self.close_workbench();
            self.view.request_redraw();
            return;
        }
        // Summon the workbench pane beside the orrery, then tile the selection.
        self.open_workbench();
        self.view.workbench.clear_tiles();
        {
            for member in self.selection_working_set() {
                self.view.workbench.open_tile(member);
            }
            // Focus the node the open was seeded from (the primary selection), so the
            // omnibar shows its URL; fall back to the first opened tile.
            self.set_focused_tile(
                self.orrery()
                    .selected_members()
                    .first()
                    .copied()
                    .or_else(|| self.view.workbench.open_members().first().copied()),
            );
        }
        self.view.request_redraw();
    }

    /// The members a selection-driven open acts on. A multi-selection is its own
    /// nodes (opened in splits). A single selection expands to the **active tabs in
    /// that node's graphlet** — its connected component intersected with the warm-tab
    /// set, plus the node itself — so you gather the live cluster around it. An empty
    /// selection yields nothing. Shared by entering the workbench and the right-click
    /// menu.
    pub(crate) fn selection_working_set(&self) -> Vec<GraphMemberId> {
        let selected = self.orrery().selected_members();
        if selected.len() > 1 {
            return selected; // multi-select → the selection
        }
        match selected.first() {
            Some(&focus) => self
                .orrery()
                .connected_members(focus)
                .into_iter()
                .filter(|m| *m == focus || self.shared.content.constellation.is_active(*m))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Apply the chrome's current settings to the host: the active-tab cap to the
    /// actor pool. Called after a chrome interaction that could have changed them.
    /// Persists to the settings sidecar when the value actually changed (so an
    /// unrelated chrome click doesn't re-write the file).
    pub(crate) fn sync_settings(&mut self) {
        let cap = self.view.chrome().settings.tab_cap;
        self.shared.content.constellation.set_cap(cap);
        if cap != self.shared.presentation.saved_tab_cap {
            self.shared.presentation.saved_tab_cap = cap;
            self.persist_settings();
        }
    }

    /// Write the current settings to the session's `settings.json` sidecar. A
    /// failure is logged, not fatal (the shell runs without persistence).
    pub(crate) fn persist_settings(&self) {
        // Preserve the on-disk DocumentScript permission opinion (§11.4): the host
        // caches it nowhere — it is read on demand at attach and edited via the
        // settings lane, not reconstructed from runtime state here — so this save
        // path must not clobber it back to the default. (Follow-on #1.)
        let preserved = settings_store::load_settings(&self.shared.session.mere_root)
            .ok()
            .flatten()
            .unwrap_or_default();
        let script_permissions = preserved.script_permissions;
        // The retention cap is read at launch + tuned in settings.json, not held in
        // runtime state, so preserve it across saves (like script_permissions).
        let retention_keep_n = preserved.retention_keep_n;
        let startup_unlock_mode = preserved.startup_unlock_mode;
        let settings = PersistedSettings {
            tab_cap: self.shared.presentation.saved_tab_cap,
            theme_id: Some(self.shared.presentation.active_theme_id.clone()),
            shellbar_edge: self.shared.presentation.shellbar_edge,
            shellbar_hidden: self.shared.presentation.shellbar_hidden,
            physics_damping: self.shared.presentation.physics_damping,
            disabled_engines: self.shared.content.engine_activation.global_disabled_vec(),
            // The document typography as embedded JSON; `None` keeps the file
            // clean when it is still the built-in look. (Typography surface.)
            document_typography: (self.shared.presentation.document_sheet
                != document_canvas::DocumentStyleSheet::default())
            .then(|| serde_json::to_value(&self.shared.presentation.document_sheet).ok())
            .flatten(),
            script_permissions,
            // Persist only non-default crawl settings (None = default), matching the
            // document_typography pattern so a pristine settings.json stays clean.
            crawl_scope: {
                let scope = self.shared.content.crawl.scope();
                (scope != crate::crawl::HostScope::SameHost).then(|| scope.as_key().to_string())
            },
            crawl_depth: {
                let depth = self.shared.content.crawl.max_depth();
                (depth != crate::crawl::CrawlPolicy::default().max_depth).then_some(depth)
            },
            crawl_sitemap: self.shared.content.crawl.seed_sitemap().then_some(true),
            crawl_max_pages: {
                let pages = self.shared.content.crawl.max_pages();
                (pages != crate::crawl::CrawlPolicy::default().max_pages).then_some(pages)
            },
            // Persist only a non-default consent level (None = the `full` default),
            // matching the crawl-setting pattern so a pristine settings.json stays clean.
            capture_consent: {
                let consent = self.shared.content.capture_consent;
                (consent != crate::browse_capture::CaptureConsent::Full)
                    .then(|| consent.as_key().to_string())
            },
            retention_keep_n,
            // The user's chrome zoom (Ctrl +/-/0); the display DPI factor is not persisted
            // (it is read fresh from the window each launch). (UI scale.)
            ui_zoom: self.shared.presentation.user_zoom,
            startup_unlock_mode,
            snapshot_idle_refresh: self.shared.presentation.snapshot_idle_refresh,
            // The byte cap itself is read at launch + tuned in settings.json, not held in
            // runtime state, so preserve it across saves (like retention_keep_n above).
            snapshot_byte_cap_mb: preserved.snapshot_byte_cap_mb,
        };
        if let Err(err) = settings_store::save_settings(&self.shared.session.mere_root, &settings) {
            tracing::warn!(%err, "failed to persist settings");
        }
    }

    /// Rebuild the chrome at the current `ui_scale`, invalidate the cached toolbar
    /// height (it re-measures from the new sheet) and the host-drawn window-control
    /// texture (re-rasterised at the new band height), then redraw. Shared by the
    /// zoom (Ctrl +/-/0) and display-DPI change paths. (UI scale.)
    pub(crate) fn refresh_ui_scale(&mut self) {
        // Bake the shared sheet at this window's dpi (D3) × the shared user_zoom.
        self.shared.presentation.dpi_scale = self.view.dpi_scale;
        self.shared.presentation.rebuild_chrome_sheet();
        self.view.toolbar_h = 0;
        self.view.window_controls_tex = None;
        self.view.request_redraw();
    }

    /// Fold a new display DPI factor (winit `ScaleFactorChanged`, or the initial
    /// `scale_factor()` at window creation) into **this window's** chrome scale (D3 —
    /// per-window). A no-op when unchanged. The shared sheet rebuilds to this window's
    /// dpi here (and re-syncs at another window's render if it differs). (Auto-DPI D3.)
    pub(crate) fn set_dpi_scale(&mut self, dpi: f32) {
        if (self.view.dpi_scale - dpi).abs() < 1e-3 {
            return;
        }
        self.view.dpi_scale = dpi;
        self.refresh_ui_scale();
    }

    /// Step the user's chrome zoom (Ctrl +/-) or reset it (Ctrl 0 -> the 1.1
    /// baseline). Clamped to a usable band, rebuilt, and persisted. (UI scale.)
    pub(crate) fn adjust_user_zoom(&mut self, delta: f32) {
        let target = if delta == 0.0 {
            1.1
        } else {
            self.shared.presentation.user_zoom + delta
        };
        let clamped = (target * 100.0).round() / 100.0;
        let clamped = clamped.clamp(0.6, 3.0);
        if (self.shared.presentation.user_zoom - clamped).abs() < 1e-3 {
            return;
        }
        self.shared.presentation.user_zoom = clamped;
        self.refresh_ui_scale();
        self.persist_settings();
    }

    /// Toggle the shellbar's visibility on this window and persist the new state. The
    /// content band grows (hidden) or shrinks (shown), so the orrery recenters once,
    /// mirroring a shellbar move. When hidden, the strip can be right-clicked no more, so
    /// it is revealed again from the command palette / `>shellbar`. (Hide-shellbar.)
    pub(crate) fn toggle_shellbar_visibility(&mut self) {
        self.shared.presentation.shellbar_hidden = !self.shared.presentation.shellbar_hidden;
        self.view.centered = false; // the content band changed size; recenter the orrery once
        self.persist_settings();
        self.view.request_redraw();
    }

    /// Persist the persona's curated context menu (command registry P4) to the persona settings
    /// store (`personas/<id>/settings/ui.json`). v0 uses the single default persona; v1 threads
    /// the active one. A failure is logged, not fatal.
    pub(crate) fn persist_menu_actions(&self) {
        let settings = session_runtime::PersonaSettings {
            menu_actions: Some(self.shared.presentation.menu_actions.clone()),
            command_usage: self.shared.presentation.command_usage.clone(),
            eviction_policy: self.shared.presentation.eviction_policy,
            // Carried through, not reset: this save rewrites the whole settings file, and
            // the launch counter is bumped once at boot (shell_new.rs), not here. (B5.)
            session_count: self.shared.session.current_session_count,
        };
        if let Err(err) = session_runtime::save_persona_settings(
            &self.shared.session.mere_root,
            self.shared.session.active_persona,
            &settings,
        ) {
            tracing::warn!(%err, "failed to persist persona menu settings");
        }
    }

    /// Cycle the short-term eviction policy to the next rung (the Alembic Recent header control)
    /// and persist it; the next `run_forgetting_pass` uses the new policy. (Editable eviction
    /// policy, B4.)
    pub(crate) fn cycle_eviction_policy(&mut self) {
        self.shared.presentation.eviction_policy =
            self.shared.presentation.eviction_policy.cycled();
        self.persist_menu_actions();
        self.view.request_redraw();
    }

    /// Record one invocation of registry command `id` — the frequency signal behind the context
    /// menu's auto-suggestions (command registry S3). Called at the command-invocation hook for
    /// both host commands and cataloged context actions; persists the updated counts.
    ///
    /// v1 persists on every invocation (the file is tiny); a debounce / write-on-idle is the
    /// refinement if the write rate ever matters.
    pub(crate) fn record_command_usage(&mut self, id: &str) {
        *self
            .shared
            .presentation
            .command_usage
            .entry(id.to_string())
            .or_insert(0) += 1;
        self.persist_menu_actions();
    }

    /// Toggle a registry command's membership in the context menu (the `pelt/menu` page, command
    /// registry P4): present → removed, absent → appended. This is the "add any command / remove
    /// a gesture" edit; it persists the new list to the persona store.
    pub(crate) fn toggle_menu_action(&mut self, id: &str) {
        let actions = &mut self.shared.presentation.menu_actions;
        if let Some(pos) = actions.iter().position(|a| a == id) {
            actions.remove(pos);
        } else {
            actions.push(id.to_string());
        }
        self.persist_menu_actions();
        self.view.request_redraw();
    }

    /// Move a context-menu command one place up or down in the order (the `pelt/menu` ▲ / ▼
    /// controls, command registry P4) by swapping it with its neighbor, then persist. A no-op at
    /// the ends or for an id not in the menu.
    pub(crate) fn move_menu_action(&mut self, id: &str, up: bool) {
        let actions = &mut self.shared.presentation.menu_actions;
        let Some(pos) = actions.iter().position(|a| a == id) else {
            return;
        };
        let swap = if up {
            pos.checked_sub(1)
        } else if pos + 1 < actions.len() {
            Some(pos + 1)
        } else {
            None
        };
        if let Some(swap) = swap {
            actions.swap(pos, swap);
            self.persist_menu_actions();
            self.view.request_redraw();
        }
    }

    /// Move context-menu command `id` to where `target` sits in the order — the drag-reorder
    /// drop (command registry B2). Removes `id`, then inserts it at `target`'s slot ("drop before
    /// the target"), and persists. Where the ▲ / ▼ buttons swap neighbors one step at a time,
    /// this lands a command anywhere in a single drag. A no-op if either id isn't in the menu or
    /// they're already adjacent in place.
    pub(crate) fn reorder_menu_action_to(&mut self, id: &str, target: &str) {
        let before = self.shared.presentation.menu_actions.clone();
        crate::list_pane::reorder_before(&mut self.shared.presentation.menu_actions, id, target);
        if self.shared.presentation.menu_actions != before {
            self.persist_menu_actions();
            self.view.request_redraw();
        }
    }

    /// Restore the context menu to the registry default order (command registry P4) and persist.
    pub(crate) fn reset_menu_actions(&mut self) {
        self.shared.presentation.menu_actions = meerkat::command::DEFAULT_MENU_ACTIONS
            .iter()
            .map(|s| s.to_string())
            .collect();
        self.persist_menu_actions();
        self.view.request_redraw();
    }

    /// The current "inertia" physics setting (linear damping), for the apparatus
    /// readout. (Physics settings.)
    pub(crate) fn physics_damping(&self) -> f32 {
        self.shared.presentation.physics_damping
    }

    /// Adjust the "inertia" setting (linear damping) by `delta`, clamped to a sane
    /// range, apply it to **every** pooled orrery (the setting is global), persist
    /// it, and redraw so the apparatus readout updates. The apparatus −/+ buttons and
    /// the omnibar drive this. Lower damping keeps more drift after a settle; higher
    /// brings nodes to rest sooner. (Physics settings.)
    pub(crate) fn adjust_physics_damping(&mut self, delta: f32) {
        let current = self.shared.presentation.physics_damping;
        let next = (current + delta).clamp(0.5, 8.0);
        if (next - current).abs() < f32::EPSILON {
            return;
        }
        self.shared.presentation.physics_damping = next;
        for orrery in self.orreries.values_mut() {
            orrery.set_physics_damping(next);
        }
        self.persist_settings();
        self.view.request_redraw();
    }
}
