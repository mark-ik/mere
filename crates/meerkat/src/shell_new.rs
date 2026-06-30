/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shell construction: the new / new_with_session_dir bootstrap.

use super::*;

impl Shell {
    pub(crate) fn new(
        proxy: winit::event_loop::EventLoopProxy<()>,
        diagnostics_rx: Receiver<DiagnosticEvent>,
    ) -> Self {
        Self::new_with_session_dir(proxy, diagnostics_rx, default_mere_root())
    }

    pub(crate) fn new_with_session_dir(
        proxy: winit::event_loop::EventLoopProxy<()>,
        diagnostics_rx: Receiver<DiagnosticEvent>,
        mere_root: PathBuf,
    ) -> Self {
        let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        // Shared per-user root (`<data_dir>/mere`): settings, the content cache, and
        // comms live here; per-session graph/frame/views live under sessions/<id>/.
        let _ = std::fs::create_dir_all(&mere_root);
        // Bring up the session registry: scan sessions/, migrate a pre-MG1 flat
        // graph in, or seed one default session. The active session's dir is where
        // the graph + frame + views load from. (Multi-graph MG1.)
        let (manifests, active_session_id) = bootstrap_sessions(&mere_root);
        let session_dir = mere_root
            .join("sessions")
            .join(active_session_id.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        // Restore persisted settings (active-tab cap, theme, shellbar edge) from the
        // shared root so they apply across sessions, not per-graph.
        let saved_settings = settings_store::load_settings(&mere_root)
            .ok()
            .flatten()
            .unwrap_or_default();
        // The active session's persona (v0: the single default persona) — the boundary its
        // persona-scoped files are filed under. Resolved from the active manifest so the wiring
        // is persona-ready when personas become user-managed.
        let active_persona = manifests
            .get(active_session_id)
            .map(|m| m.persona_id)
            .unwrap_or_else(session_runtime::PersonaId::default_persona);
        // The persona's UI settings (command registry P4/S3): the curated context menu + the
        // command-usage frequencies behind auto-suggest. Loaded before `mere_root` is moved into
        // the session struct below.
        let persona_ui = session_runtime::load_persona_settings(&mere_root, active_persona)
            .ok()
            .flatten()
            .unwrap_or_default();
        let menu_actions = persona_ui.menu_actions.unwrap_or_else(default_menu_actions);
        let command_usage = persona_ui.command_usage;
        let eviction_policy = persona_ui.eviction_policy;
        let mut chrome = Chrome::new("mere://welcome");
        chrome.settings.tab_cap = saved_settings.tab_cap;
        let runner = window_view::shell_runner(dom.clone(), chrome);
        let content_location = runner.state().chrome.content_location().to_string();
        // Durable content cache (S3.2c), shared (persona-scoped) under the mere root
        // so sessions don't re-fetch each other's pages; `None` disables caching.
        let store = match FjallStore::open(mere_root.join("content")) {
            Ok(mut store) => {
                // Restore this persona's persisted HTTP session, so a login survives
                // an app restart (native session store; durability thread).
                fetch::load_cookies(&mut store, active_persona);
                // Seed the browsing-trace schema so the live trail recorder can
                // save (capture/provenance/consent plan, C1).
                if let Err(err) =
                    pollster::block_on(eidetic::browsing::bootstrap_browsing_schema(&mut store))
                {
                    tracing::warn!(?err, "browsing-trace schema bootstrap failed");
                }
                // Retention (plan C4): keep the N most recent traces and age out the
                // rest, so the trail does not grow unbounded across sessions. A
                // per-launch housekeeping pass; bounding intra-session growth is a
                // follow-on. N is tuned in settings.json (`retention_keep_n`).
                const RETENTION_KEEP_N_DEFAULT: usize = 10_000;
                let keep_n = saved_settings
                    .retention_keep_n
                    .unwrap_or(RETENTION_KEEP_N_DEFAULT);
                let pruned = pollster::block_on(async {
                    let mut memory =
                        eidetic::browsing::BrowsingMemory::load(&mut store, 64).await?;
                    memory.apply_quota(&mut store, keep_n).await
                });
                match pruned {
                    Ok(0) => {
                        tracing::debug!(keep_n, "browsing-trace retention pass: nothing aged out")
                    }
                    Ok(aged) => tracing::info!(aged, keep_n, "browsing-trace retention pass"),
                    Err(err) => tracing::warn!(?err, "browsing-trace retention pass failed"),
                }
                Some(store)
            }
            Err(err) => {
                tracing::warn!(%err, "content cache unavailable; running without it");
                None
            }
        };
        let graph_file = session_dir.join(session_graph_store::GRAPH_FILE);
        let restored = match session_graph_store::load(&graph_file) {
            Ok(Some(graph)) => {
                tracing::info!(path = ?graph_file, "restored the session graph");
                Some(graph)
            }
            Ok(None) => None,
            Err(err) => {
                tracing::warn!(%err, path = ?graph_file, "session graph load failed; starting fresh");
                None
            }
        };
        let mut orrery = match restored {
            Some(graph) => Orrery::with_graph(graph),
            None => {
                // The orrery opens on one node and grows from there as the user
                // navigates (the graph-rooted browse loop).
                let mut orrery = Orrery::new();
                if !content_location.is_empty() {
                    orrery.visit(&content_location);
                }
                orrery
            }
        };
        // Restore the view-intent (camera + focused node) so the spatial view and
        // the open card persist across restarts. A restored camera suppresses the
        // first-frame recenter; the focused node re-selects (if it still exists).
        let restored_view =
            view_intent_store::load_view_intent(&session_dir, DEFAULT_FRAME, DEFAULT_PANE)
                .ok()
                .flatten();
        let restored_camera = restored_view.as_ref().and_then(|v| v.camera);
        if let Some(snapshot) = &restored_camera {
            orrery.set_camera(snapshot_to_camera(snapshot));
            let (yaw, tilt) = snapshot_yaw_tilt(snapshot);
            orrery.set_yaw(yaw);
            orrery.set_tilt(tilt);
        }
        if let Some(url) = restored_view.as_ref().and_then(|v| v.focus.as_deref()) {
            orrery.select_by_url(url);
        }
        // Restore the orrery pane's layout strategy at boot (None = force-directed),
        // recomputed on the first frame from the node set. (Layout picker.)
        orrery.set_layout_strategy(restored_view.as_ref().and_then(|v| v.strategy.clone()));
        session_ops::restore_hidden_relations(&mut orrery, restored_view.as_ref());
        // Always-offload physics (P6): move the orrery's gyre simulation onto its
        // own armillary actor thread, so a heavy settle never blocks compositing or
        // input. It wakes the loop through the same winit proxy as the other
        // actors; the host folds each layout snapshot into the orrery's read model
        // on the next frame.
        let physics_proxy = proxy.clone();
        let physics_wake: armillary::Wake = Arc::new(move || {
            let _ = physics_proxy.send_event(());
        });
        orrery.offload_physics(physics_wake.clone());
        // The fetch actor wakes the loop through the winit proxy; armillary takes
        // the wake as a host-neutral callback.
        let fetch_proxy = proxy.clone();
        let fetch_wake: armillary::Wake = Arc::new(move || {
            let _ = fetch_proxy.send_event(());
        });
        let (fetch_handle, fetch_rx) = fetch::spawn_fetcher(fetch_wake);
        // The find-in-page worker lays out the focused page off the UI thread (a full
        // serval layout costs ~1-2s, far too slow per keystroke) and ships back match
        // rects, woken through the same proxy.
        let find_proxy = proxy.clone();
        let find_wake: armillary::Wake = Arc::new(move || {
            let _ = find_proxy.send_event(());
        });
        let (find_worker, find_rx) = find_worker::spawn_find_worker(find_wake);
        // The content actor renders the focused card off the UI thread (it owns the
        // serval cascade + nematic engines + a per-tile subresource cache on its own
        // thread) and ships scenes / wanted subresources / harvested linked data
        // back through the same wake.
        let content_proxy = proxy.clone();
        let content_wake: armillary::Wake = Arc::new(move || {
            let _ = content_proxy.send_event(());
        });
        // The crawl actor shares the content wake: its updates schedule the same frame
        // drain that picks up content-actor updates. (Relational-browse V2.)
        let mut crawl = crawl::CrawlSession::new(content_wake.clone());
        // Restore the crawl scope / depth the settings lane last persisted.
        if let Some(scope) = saved_settings
            .crawl_scope
            .as_deref()
            .and_then(crawl::HostScope::from_key)
        {
            crawl.set_scope(scope);
        }
        if let Some(depth) = saved_settings.crawl_depth {
            crawl.set_max_depth(depth);
        }
        if let Some(whole_site) = saved_settings.crawl_sitemap {
            crawl.set_seed_sitemap(whole_site);
        }
        if let Some(pages) = saved_settings.crawl_max_pages {
            crawl.set_max_pages(pages);
        }
        let mut constellation = Constellation::new(content_wake);
        constellation.set_cap(saved_settings.tab_cap);
        // Seed the actor pool's deactivated-engine set so a globally-disabled
        // document engine renders the fallback off-thread too. (engine-picker Phase 1b.)
        constellation
            .set_disabled_engines(saved_settings.disabled_engines.iter().cloned().collect());
        // Seed the installed DocumentScript origin bindings (§11.4 follow-on #2):
        // resolved from `script-bindings.json` (user form) + installed mod manifests
        // under `<mere_root>/mods/` ("installed extension" form), both against the
        // session script-permissions, so a fresh navigation to a bound origin
        // auto-attaches its script (the App-default Allow narrowed by any
        // session-scope opinion). User bindings take precedence on origin overlap
        // (first match wins in `binding_for`), so they lead the merged list.
        let mut script_bindings = crate::content::script::load_resolved_bindings(
            &mere_root,
            &saved_settings.script_permissions,
        );
        script_bindings.extend(crate::content::script::load_mod_bindings(
            &mere_root,
            &saved_settings.script_permissions,
        ));
        constellation.set_script_bindings(script_bindings);
        // The p2p sync actor: an armillary actor whose run closure owns a tokio
        // runtime (built on its thread) that binds the transport + joins the tessera
        // demo moot, polling status back through the same wake shape as fetch/content.
        // Setup failure disables p2p, not the shell.
        let sync_proxy = proxy.clone();
        let sync_wake: armillary::Wake = Arc::new(move || {
            let _ = sync_proxy.send_event(());
        });
        let (sync_handle, sync_rx) = sync::spawn_sync(sync_wake, sync::DEMO_MOOT);
        // The comms actor: owns the live `Comms` (misfin + murm adapters over local
        // stores under the session dir) on its own tokio runtime, waking the loop
        // through the same winit proxy. Setup failure disables comms, not the shell.
        let comms_proxy = proxy.clone();
        let comms_wake: armillary::Wake = Arc::new(move || {
            let _ = comms_proxy.send_event(());
        });
        let (comms_handle, comms_rx) = comms_host::spawn_comms(comms_wake, mere_root.clone());
        // The host's own nematic engine registry, for rendering snapshot cards
        // from the durable cache without a live actor (Card #4).
        let mut engine_registry = EngineRegistry::new();
        for engine in nematic::engines() {
            engine_registry.register(engine);
        }
        // Resolve the active theme's chrome tokens once and bake the chrome CSS
        // from them (theming pass). A runtime theme switch (settings / apparatus)
        // rebuilds this from the registry; today it opens on the default theme.
        let mut theme = ThemeRegistry::default();
        // Load user / mod theme files (`<mere_root>/themes/*.json`) so a saved
        // active user theme resolves and they appear in the picker. A malformed
        // file is skipped + logged, never fatal. (Seed-palette themes T3/T4.)
        for def in theme_store::load_user_themes(&mere_root) {
            let id = def.id.clone();
            if let Err(e) = theme.add_user_theme(def) {
                tracing::warn!(theme = %id, error = %e, "skipping invalid user theme");
            }
        }
        // Honor the saved theme (falls back to the registry default), and keep the
        // registry so the apparatus pane can switch at runtime. (Theme switcher.)
        let active_theme_id = saved_settings
            .theme_id
            .clone()
            .unwrap_or_else(|| theme.active_theme().resolved_id);
        let resolution = theme.set_active_theme(&active_theme_id);
        let active_theme_id = resolution.resolved_id;
        let chrome_theme = resolution.tokens.chrome;
        // Build the chrome at the user's persisted zoom (the display DPI factor folds in
        // once the window exists and `app_handler` rebuilds). (UI scale.)
        let mut chrome_sheet = scale_px(chrome_sheet(&chrome_theme), saved_settings.ui_zoom);
        // The knot editor's syntax highlighting: colour the `syntax-*` classes the
        // styled field emits, derived perceptually from the active theme's seeds by
        // tinct (so they track a theme switch), falling back to a dark triad.
        let syntax_seeds = theme
            .theme_def(&active_theme_id)
            .map(|d| d.seeds)
            .unwrap_or_else(meerkat::knot_highlight::fallback_seeds);
        chrome_sheet.extend(meerkat::knot_highlight::syntax_css(&syntax_seeds));
        // Theme the orrery's backdrop + edges from the same resolved theme. (A2.)
        let (orrery_backdrop, orrery_edge) = orrery_palette(&resolution.tokens);
        orrery.set_palette(orrery_backdrop, orrery_edge);
        // The document-lane palette for content cards, from the same theme. (P3.)
        let document_palette = document_palette(&resolution.tokens);
        // The user's persisted document typography (embedded JSON in settings),
        // or the built-in look. Composed with the palette per render. (Typography.)
        let document_sheet = saved_settings
            .document_typography
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        // The content region opens as a single graph pane (orrery / tiled
        // workbench); summoning the roster splits it. (Frame tree, F1.)
        let active_graph = manifests
            .get(active_session_id)
            .map(|m| m.root_graph_id)
            .unwrap_or_default();
        let mut frame_layout = default_content_frame(active_graph);
        // The frame is **window-scoped** (Model B, MG5): load it from the shared root,
        // not the session. A pre-MG5 install saved it per-session, so carry the active
        // session's layout up once if the shared one is absent.
        let mut next_pane_id = 1u64;
        let restored_frame = frame_layout_store::load_frame_layout(&mere_root)
            .ok()
            .flatten()
            .or_else(|| {
                frame_layout_store::load_frame_layout(&session_dir)
                    .ok()
                    .flatten()
            });
        if let Some(mut restored) = restored_frame {
            // Keep the restored layout only if it carries the graph (Orrery) pane;
            // a pre-coexistence layout (graph pane saved as Workbench) is stale, so
            // fall back to the default single Orrery pane. (Workbench-as-pane.)
            if restored
                .iter_leaves()
                .any(|(_, c, _)| matches!(c, PaneContent::Orrery))
            {
                next_pane_id = restored
                    .iter_leaves()
                    .map(|(id, _, _)| id.0)
                    .max()
                    .unwrap_or(0)
                    + 1;
                // Reattach only the graph-bound leaves whose graph_id is nil / stale
                // (no live session) to the active graph; a leaf pinned to a *valid*
                // graph (a second graph-pane restored from a prior run) stays put, so
                // it reloads instead of being clobbered onto the active graph. (MG5;
                // pane-as-unit restore.)
                let valid_graphs: HashSet<GraphId> =
                    manifests.iter().map(|(_, m)| m.root_graph_id).collect();
                restored.retag_graph_bound_invalid(&valid_graphs, active_graph);
                frame_layout = restored;
            }
        }
        let a11y_proxy = proxy.clone();
        let mut view = window_view::WindowView::new(
            window_view::WindowKind::Primary,
            active_graph,
            dom,
            runner,
            Workbench::new(),
        );
        view.centered = restored_camera.is_some();
        view.content_location = content_location;
        view.frame_layout = frame_layout;
        view.next_pane_id = next_pane_id;
        // Collapse any duplicate Orrery panes a persisted layout accumulated (two
        // panes on one graph render the extra blank); keep one per graph. (Pane-as-unit.)
        view.frame_layout.dedupe_graph_panes();
        // Restore this session's persisted workbench tiling at boot (not just on a
        // later session switch), so split shape / tabs / active tab survive a restart;
        // pruned to the loaded graph's members. (A3 persistence.)
        let present_members: HashSet<forme::GraphMemberId> =
            orrery.graph().nodes().map(|(_, node)| node.id).collect();
        view.workbench = session_ops::load_workbench(&session_dir, &present_members);
        // Restore live workbench-mirror mode at boot; the render loop re-scopes the
        // orrery to the just-restored open tiles. (Workbench mirror.)
        view.mirror_tiles = restored_view.as_ref().is_some_and(|v| v.mirror_tiles);
        // Restore the orrery's settled layout from the cartography sidecar at boot,
        // overriding the graph's load-time seed (the live layout is never committed to
        // graph.json). (Position sidecar.)
        if let Some(geom) = session_ops::load_cartography(&session_dir, &present_members) {
            orrery.seed_cartography(geom.iter());
            // Restore the importance metric first, so the sizing restore below recomputes with it.
            orrery.apply_cartography_importance_metric(geom.importance_metric());
            // Restore the per-node sizes + the size-by-degree / size-by-importance scene flags
            // alongside the positions. (Node-rep / graph signals.)
            orrery.apply_cartography_sizing(
                geom.size_iter(),
                geom.size_by_degree(),
                geom.size_by_importance(),
            );
            // Restore the custom sprite faces, so a textured node re-opens textured. (Node-rep.)
            orrery.apply_cartography_sprites(geom.sprite_iter());
            // ...and their collider hulls, so the traced-to-image collider survives too. (Node-rep.)
            orrery.apply_cartography_sprite_hulls(geom.sprite_hull_iter());
            // ...and the per-node physical materials, so a tuned node re-opens tuned. (Body & face.)
            orrery.apply_cartography_materials(geom.material_iter());
            // ...and the face overrides LAST, so a node switched off its sprite face re-opens on
            // the chosen face, not back on Sprite from the sprite restore above. (Body & face.)
            orrery.apply_cartography_faces(geom.face_iter());
        }
        // Pool every graph a restored pane resolves to, not just the active one, so a
        // second graph-pane (persisted from a prior run) loads instead of leaving a
        // blank pane the user can't dismiss. Each cold-loads its graph from its
        // session dir and offloads its own physics, like the active orrery above; the
        // render then centres it on first frame. (Window composition — pane-as-unit
        // restore.)
        let mut orreries: HashMap<GraphId, Orrery> = HashMap::from([(active_graph, orrery)]);
        let mut orrery_lru: Vec<GraphId> = vec![active_graph];
        let extra_graphs: HashSet<GraphId> = view
            .frame_layout
            .iter_leaves()
            .filter(|(_, c, gid)| matches!(c, PaneContent::Orrery) && *gid != active_graph)
            .map(|(_, _, gid)| gid)
            .collect();
        for gid in extra_graphs {
            let dir = manifests
                .iter()
                .find(|(_, m)| m.root_graph_id == gid)
                .map(|(id, m)| {
                    m.storage_path.clone().unwrap_or_else(|| {
                        mere_root.join("sessions").join(id.as_uuid().to_string())
                    })
                });
            let graph = dir.and_then(|d| {
                session_graph_store::load(&d.join(session_graph_store::GRAPH_FILE))
                    .ok()
                    .flatten()
            });
            let mut extra = match graph {
                Some(g) => Orrery::with_graph(g),
                None => Orrery::new(),
            };
            extra.offload_physics(physics_wake.clone());
            orreries.insert(gid, extra);
            orrery_lru.push(gid);
        }
        // Apply the persisted "inertia" (linear damping) to every pooled orrery, so a
        // restart honors the saved physics setting. (Physics settings.)
        for orrery in orreries.values_mut() {
            orrery.set_physics_damping(saved_settings.physics_damping);
        }
        let mut app = Self {
            shared: SharedState {
                content: Content {
                    constellation,
                    pages: HashMap::new(),
                    store,
                    fetch_handle,
                    find_worker,
                    engine_registry,
                    engine_pins: HashMap::new(),
                    route_policy: inker::routing::EngineRoutePolicy::default(),
                    engine_activation: engine_activation::EngineActivation::new(
                        saved_settings.disabled_engines.clone(),
                    ),
                    crawl,
                    capture_consent: saved_settings
                        .capture_consent
                        .as_deref()
                        .and_then(crate::browse_capture::CaptureConsent::from_key)
                        .unwrap_or_default(),
                },
                session: Session {
                    manifests,
                    active_session_id,
                    active_persona,
                    session_dir,
                    mere_root,
                    session_labels: HashMap::new(),
                    host_text: text::HostText::new(),
                },
                presentation: Presentation {
                    theme,
                    chrome_theme,
                    chrome_sheet,
                    active_theme_id,
                    saved_tab_cap: saved_settings.tab_cap,
                    shellbar_edge: saved_settings.shellbar_edge,
                    shellbar_hidden: saved_settings.shellbar_hidden,
                    physics_damping: saved_settings.physics_damping,
                    user_zoom: saved_settings.ui_zoom,
                    // 1.0 until the window exists; `create_window` reads the real
                    // `scale_factor()` and rebuilds at the display's density. (Auto-DPI D1.)
                    dpi_scale: 1.0,
                    document_palette,
                    document_sheet,
                    menu_actions,
                    command_usage,
                    eviction_policy,
                },
                comms_handle,
                sync_handle,
                inbox: KernelInbox {
                    fetch: fetch_rx,
                    find: find_rx,
                    sync: sync_rx,
                    comms: comms_rx,
                    diagnostics: diagnostics_rx,
                },
                observability: HostObservability::new(),
            },
            orreries,
            orrery_lru,
            graphlets: HashMap::new(),
            windows: HashMap::new(),
            primary: None,
            pending_view: Some(view),
            render_core: None,
            clipboard: arboard::Clipboard::new().ok(),
            a11y_bridge: a11y_bridge::AccessKitBridge::new({
                let proxy = a11y_proxy.clone();
                move || {
                    let _ = proxy.send_event(());
                }
            }),
            secondary_a11y_bridges: HashMap::new(),
            a11y_proxy,
            a11y_action_routes: HashMap::new(),
            commands: Vec::new(),
            physics_wake,
            // A fresh launch starts "active" (no idle pass before the user has done
            // anything) with no recorded pass yet. (Alembic B1.)
            last_activity: std::time::Instant::now(),
            last_forgetting: None,
            _kernel: armillary::KernelThread::new(),
        };
        let pane_count = app
            .pending_view
            .as_ref()
            .expect("pending primary view")
            .frame_layout
            .iter_leaves()
            .count();
        app.shared
            .observability
            .record_startup(&app.shared.presentation.active_theme_id, pane_count);
        // The initial switcher-thumbnail + a11y refresh run in `resumed`, once the
        // primary view is keyed into the registry (a ctx needs a window id). (MW2 (d).)
        app
    }
}
