/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `ApplicationHandler` impl for [`Shell`](super::Shell). Factored from
//! `main.rs` to keep files under the workspace 600-LOC ceiling.

use std::sync::Arc;

use netrender::NetrenderOptions;
use orrery::WHEEL_PAN_SCALE;
use serval_winit_host::{RenderCore, modifiers_from_winit};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{CursorIcon, Window, WindowId};

use super::observability::Severity;
use super::{Shell, comms_host, fetch, scrying_host, sync, titlebar};

/// The favicon URL to fetch for a freshly-loaded HTML page: the resolved
/// `<link rel="icon">` href if the head declares one, else the well-known
/// `{origin}/favicon.ico` for an http(s) page. `None` for a page with neither (a
/// non-http(s) scheme with no icon link, e.g. gemtext). A lightweight parse of the
/// already-fetched body, reusing serval's `<link>` scan. (Favicon-on-tile.)
pub(crate) fn favicon_url_for(page_url: &str, body: &str) -> Option<String> {
    let base = url::Url::parse(page_url).ok()?;
    let doc = serval_static_dom::StaticDocument::parse(body);
    if let Some(href) = serval_layout::linked_icon_href(&doc) {
        if let Ok(resolved) = base.join(&href) {
            return Some(resolved.to_string());
        }
    }
    // No declared icon: fall back to the well-known location for web pages only.
    if matches!(base.scheme(), "http" | "https") {
        if let Ok(fallback) = base.join("/favicon.ico") {
            return Some(fallback.to_string());
        }
    }
    None
}

/// The platform virtual-key code for a winit key event, for forwarding into a
/// scrying tile's WebView. Named keys map to their Win32 VKs; character keys use
/// the uppercased char (matching Win32 VK_A..VK_Z / VK_0..VK_9). (Scrying X2.)
fn scrying_vk(event: &winit::event::KeyEvent) -> u32 {
    use winit::keyboard::{Key, NamedKey};
    match &event.logical_key {
        Key::Named(n) => match n {
            NamedKey::Enter => 0x0D,
            NamedKey::Tab => 0x09,
            NamedKey::Backspace => 0x08,
            NamedKey::Escape => 0x1B,
            NamedKey::Space => 0x20,
            NamedKey::Delete => 0x2E,
            NamedKey::ArrowLeft => 0x25,
            NamedKey::ArrowUp => 0x26,
            NamedKey::ArrowRight => 0x27,
            NamedKey::ArrowDown => 0x28,
            NamedKey::Home => 0x24,
            NamedKey::End => 0x23,
            NamedKey::PageUp => 0x21,
            NamedKey::PageDown => 0x22,
            _ => 0,
        },
        Key::Character(s) => s
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase() as u32)
            .unwrap_or(0),
        _ => 0,
    }
}

/// Apply one comms actor update to a window's chrome — the per-window half of the
/// MW3 step-5 fan-out. Mirrors the inline mutations the primary's actor-drain does, so a
/// chrome-bearing secondary stays in sync with the primary. (MW3 step 5.)
fn apply_comms_to_chrome(
    view: &mut super::window_view::WindowView,
    update: &comms_host::CommsUpdate,
) {
    match update {
        comms_host::CommsUpdate::Inbox(inbox) => {
            view.chrome_update(|c| c.comms.set_inbox(inbox.clone()));
        }
        comms_host::CommsUpdate::Thread(id, messages) => {
            view.chrome_update(|c| {
                if c.comms.selected() == Some(id) {
                    c.comms.set_thread(messages.clone());
                }
            });
        }
        comms_host::CommsUpdate::Sent(_) => {
            view.chrome_update(|c| c.clear_comms_draft());
        }
        comms_host::CommsUpdate::SendOutcome(line) => {
            view.chrome_update(|c| c.comms.set_send_status(line.clone()));
        }
        comms_host::CommsUpdate::Identity {
            misfin_address,
            cabal_ticket,
        } => {
            view.chrome_update(|c| {
                c.comms
                    .set_identity(misfin_address.clone(), cabal_ticket.clone())
            });
        }
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // winit calls `resumed` on every resume; the primary is created once. After
        // that the view lives in the registry, so this no-ops. (MW2 (d).)
        if self.primary.is_some() {
            return;
        }
        let Some(view) = self.pending_view.take() else {
            return;
        };
        // Boot the shared present core once (the first window): wgpu + netrender. Each
        // window's surface is then created from it, so N windows share one device.
        if self.render_core.is_none() {
            let options = NetrenderOptions {
                tile_cache_size: Some(64),
                enable_vello: true,
                ..Default::default()
            };
            match RenderCore::boot(options) {
                Ok(core) => self.render_core = Some(core),
                Err(err) => {
                    eprintln!("[meerkat] {err}");
                    self.pending_view = Some(view); // boot failed; keep the view to retry
                    event_loop.exit();
                    return;
                }
            }
        }
        // Create the OS window + swapchain surface and key it into the registry. The
        // primary differs from a spawned window only in the bootstrap that follows
        // (a11y install + the initial refresh + restored content).
        let Some((id, window)) = self.create_window(event_loop, view) else {
            event_loop.exit();
            return;
        };
        self.primary = Some(id);
        // Drive the primary-only bootstrap through the ctx (a11y install + the initial
        // switcher / a11y refresh + restored content).
        let mut wc = self.ctx();
        let initial_a11y = wc.build_a11y_projection().tree_update();
        match wc.a11y_bridge.install(&window, initial_a11y) {
            Ok(()) => wc.shared.observability.record_probe(
                "a11y_bridge",
                "installed",
                "OS AccessKit bridge installed",
            ),
            Err(err) => wc.shared.observability.record_probe(
                "a11y_bridge",
                "degraded",
                format!("OS AccessKit bridge unavailable: {err}"),
            ),
        }
        // a11y is installed; safe to show the window now (the order Windows requires).
        window.set_visible(true);
        window.request_redraw();
        wc.refresh_a11y_summary();
        wc.refresh_session_thumbnails();

        // Show the restored focused node's content from the durable cache (so a
        // reload re-opens its card without a navigation). A fresh `mere://welcome`
        // focus is not fetchable, so this is a no-op there.
        if let Some(url) = wc.orrery().focused_url().map(str::to_string) {
            wc.ensure_content(&url);
        }
    }

    /// Drain completed fetches (delivery model 2): a worker woke us via the proxy;
    /// fold each outcome into the content cache and re-render the card.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        // An actor may wake us before the primary window exists; the typed channels
        // buffer, so we drain on the next wake after `resumed`. (MW2 (d).)
        if self.primary.is_none() {
            return;
        }
        let mut wc = self.ctx();
        wc.drain_portable_diagnostics();
        wc.drain_a11y_actions();
        // The kernel inbox dispatch: the one documented place that applies what the
        // actors tell the kernel. Each typed stream is drained and folded into
        // canonical state here on the kernel thread; the actors never touch it.
        let mut card_changed = false;
        let mut graph_changed = false;
        // One `FetchUpdate` stream carries both page documents and subresources.
        while let Ok(update) = wc.shared.inbox.fetch.try_recv() {
            match update {
                fetch::FetchUpdate::Page(outcome) => {
                    let state = match outcome.result {
                        Ok(fetched) => {
                            // Persist so a reload shows this page without
                            // re-fetching. Linked-data harvest now happens in the
                            // content actor (on `Show`), which ships a `Contribution`.
                            wc.save_cached(
                                &outcome.url,
                                fetched.content_type.clone(),
                                fetched.body.as_bytes(),
                            );
                            wc.shared.observability.record_actor(
                                "fetch",
                                "succeeded",
                                Some(outcome.url.clone()),
                            );
                            // Best-effort: fetch this page's favicon so its graph
                            // tile can show a real icon. The bytes route back as
                            // `FetchUpdate::Favicon`, keyed to this page url.
                            // (Favicon-on-tile.)
                            if let Some(icon_url) = favicon_url_for(&outcome.url, &fetched.body) {
                                wc.shared.content.fetch_handle.command(
                                    fetch::FetchCommand::Favicon {
                                        owner_url: outcome.url.clone(),
                                        url: icon_url,
                                    },
                                );
                            }
                            fetch::ContentState::Ready(fetched)
                        }
                        Err(reason) => {
                            let detail = format!("{}: {reason}", outcome.url);
                            wc.shared.observability.record_actor(
                                "fetch",
                                "failed",
                                Some(detail.clone()),
                            );
                            wc.shared.observability.record_diagnostic(
                                "meerkat.actor.fetch.failed",
                                Severity::Warn,
                                detail,
                            );
                            fetch::ContentState::Failed(reason)
                        }
                    };
                    wc.shared.content.pages.insert(outcome.url, state);
                    card_changed = true;
                    // The fetch may have set cookies; persist this persona's session
                    // so a login survives a restart (dirty-gated, no-op when nothing
                    // changed). (Native session store; durability thread.)
                    if let Some(store) = wc.shared.content.store.as_mut() {
                        fetch::persist_cookies(store, wc.shared.session.active_persona);
                    }
                }
                // A subresource (page CSS / media): persist it (its content-type is
                // unknown here) so the page's assets survive restart, then broadcast
                // the bytes to every active node's actor — the fetch stream is keyed
                // by URL, not by which node wanted it; each actor dedups via its own
                // resource store and only the one that wanted it re-renders.
                fetch::FetchUpdate::Subresource(sub) => {
                    wc.shared.observability
                        .record_actor("fetch", "subresource", Some(sub.url.clone()));
                    wc.save_cached(&sub.url, None, &sub.bytes);
                    wc.shared.content.constellation.broadcast_resource(&sub.url, &sub.bytes);
                }
                // A page's favicon arrived: decode it to RGBA and stamp it on the
                // node currently at the owner url, so the orrery paints it on that
                // node's tile. Snapshot persistence carries it across restarts.
                // (Favicon-on-tile.)
                fetch::FetchUpdate::Favicon { owner_url, bytes } => {
                    if let Some(decoded) = serval_layout::decode_image_bytes(&bytes) {
                        graph_changed |= wc.orrery_mut().set_node_favicon(
                            &owner_url,
                            decoded.rgba,
                            decoded.width,
                            decoded.height,
                        );
                    }
                }
            }
        }
        // Drain the find worker's replies: apply the latest query's match rects (older
        // generations are dropped) and feed any wanted subresources back from the cache.
        // Collected first so the receiver borrow ends before the &mut apply. (Find.)
        let mut find_results = Vec::new();
        while let Ok(result) = wc.shared.inbox.find.try_recv() {
            find_results.push(result);
        }
        for result in find_results {
            card_changed |= wc.apply_find_result(result);
        }
        // Drain every active node's actor in one pass: scenes land in the pool, the
        // wanted subresources + harvested contributions come back for the host.
        let drained = wc.shared.content.constellation.drain();
        card_changed |= drained.any_scene;
        if !drained.respawned.is_empty() {
            // A content tile's actor died (panic, isolated to its thread) and the
            // pool respawned it; redraw so the next frame re-Shows it (self-healing).
            tracing::warn!(
                count = drained.respawned.len(),
                "respawned crashed content tile(s)"
            );
            wc.shared.observability.record_actor(
                "content",
                "respawned",
                Some(format!("count={}", drained.respawned.len())),
            );
            wc.shared.observability.record_diagnostic(
                "meerkat.actor.content.respawned",
                Severity::Warn,
                format!("count={}", drained.respawned.len()),
            );
            card_changed = true;
        }
        for (member, urls) in drained.wanted {
            // The actor deduped these; a durable-cache hit feeds that node directly,
            // a miss spawns a network fetch whose bytes broadcast back on arrival.
            for url in urls {
                if let Some(stored) = wc.load_cached(&url) {
                    wc.shared.content.constellation.send_resource(member, url, stored.body);
                } else {
                    wc.shared.content.fetch_handle
                        .command(fetch::FetchCommand::Subresource(url));
                }
            }
        }
        if !drained.contributions.is_empty() {
            // Each contribution carries the graph its harvesting node belongs to.
            // Apply the focused graph's here, through the ctx's bundled orrery; a
            // background graph's contributions route to that graph's pooled orrery
            // with the multi-graph flip (at one focused graph there are none).
            let focused = wc.view.focused_graph;
            graph_changed |= wc.orrery_mut().ingest_graph(|g| {
                let mut changed = false;
                for (gid, contribution) in &drained.contributions {
                    if *gid != focused {
                        continue;
                    }
                    let outcome = linked_data::apply_contribution(g, contribution);
                    changed |= outcome.nodes_created > 0 || outcome.edges_asserted > 0;
                }
                changed
            });
        }
        // The crawl actor (relational-browse V2) harvests off the render path; drain its
        // link + metadata contributions on this same wake and apply them to the crawl's
        // graph, exactly as the content harvests above. (At one focused graph, a crawl
        // seeded elsewhere awaits the multi-graph flip, like content contributions.)
        let crawl_pairs = wc.shared.content.crawl.drain();
        if !crawl_pairs.is_empty() {
            let focused = wc.view.focused_graph;
            graph_changed |= wc.orrery_mut().ingest_graph(|g| {
                let mut changed = false;
                for (gid, contribution) in &crawl_pairs {
                    if *gid != focused {
                        continue;
                    }
                    let outcome = linked_data::apply_contribution(g, contribution);
                    changed |= outcome.nodes_created > 0 || outcome.edges_asserted > 0;
                }
                changed
            });
        }
        // P2P sync status (S5.0): the same wake also carries lane-status changes.
        // Fold the latest into the chrome chip (the host owns the mutation).
        //
        // MW2 (d2) deferred to MW3: the sync-chip + comms writes below target the
        // primary's chrome (`wc.view.runner`). At N>1 they fan out to every window
        // whose template carries that chrome — a two-phase restructure (drain +
        // collect here, then replay per `self.windows.values_mut()` after the ctx
        // borrow ends). Done when the second window exists to test it against, so the
        // fan-out isn't an untested loop over one element through the actor-drain
        // hot path. (Multi-window plan MW3.)
        let mut latest_sync = None;
        while let Ok(update) = wc.shared.inbox.sync.try_recv() {
            wc.shared.observability.record_actor(
                "sync",
                "status",
                Some(format!(
                    "syncing={};ops={}",
                    update.status.syncing, update.status.ops_received
                )),
            );
            latest_sync = Some(sync::to_indicator(&update, sync::LANE_LABEL));
        }
        // Apply to the primary inline; `latest_sync` is replayed onto chrome-bearing
        // secondaries after the ctx borrow ends (step-5 fan-out below), so borrow it
        // here rather than moving it out.
        if let Some(indicator) = &latest_sync {
            wc.view.chrome_update(|c| c.sync = indicator.clone());
            wc.view.request_redraw();
        }
        // Comms (P6c): the comms actor delivers conversation lists + threads here;
        // fold each into the docked pane (the host owns the chrome mutation). Each
        // update is also collected for the step-5 fan-out onto chrome-bearing secondaries.
        let mut comms_changed = false;
        let mut comms_updates: Vec<comms_host::CommsUpdate> = Vec::new();
        while let Ok(update) = wc.shared.inbox.comms.try_recv() {
            comms_updates.push(update.clone());
            match update {
                comms_host::CommsUpdate::Inbox(inbox) => {
                    wc.shared.observability.record_actor(
                        "comms",
                        "inbox",
                        Some(format!("conversations={}", inbox.conversations.len())),
                    );
                    wc.shared.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("inbox".to_string()),
                    );
                    wc.view.chrome_update(|c| c.comms.set_inbox(inbox.clone()));
                    comms_changed = true;
                }
                comms_host::CommsUpdate::Thread(id, messages) => {
                    wc.shared.observability.record_actor(
                        "comms",
                        "thread",
                        Some(format!("{} messages={}", id.key, messages.len())),
                    );
                    wc.shared.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("thread".to_string()),
                    );
                    wc.view.chrome_update(|c| {
                        if c.comms.selected() == Some(&id) {
                            c.comms.set_thread(messages.clone());
                        }
                    });
                    comms_changed = true;
                }
                comms_host::CommsUpdate::Sent(_id) => {
                    wc.shared.observability.record_actor("comms", "sent", None);
                    wc.shared.observability
                        .record_actor("comms", "succeeded", Some("sent".to_string()));
                    wc.view.chrome_update(|c| c.clear_comms_draft());
                    comms_changed = true;
                }
                comms_host::CommsUpdate::SendOutcome(line) => {
                    wc.shared.observability
                        .record_actor("comms", "send_outcome", Some(line.clone()));
                    wc.shared.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("send_outcome".to_string()),
                    );
                    wc.view
                        .chrome_update(|c| c.comms.set_send_status(line.clone()));
                    comms_changed = true;
                }
                comms_host::CommsUpdate::Identity {
                    misfin_address,
                    cabal_ticket,
                } => {
                    wc.shared.observability.record_actor(
                        "comms",
                        "identity",
                        Some(format!(
                            "misfin={};cabal={}",
                            misfin_address,
                            cabal_ticket.is_some()
                        )),
                    );
                    wc.shared.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("identity".to_string()),
                    );
                    wc.view.chrome_update(|c| {
                        c.comms
                            .set_identity(misfin_address.clone(), cabal_ticket.clone())
                    });
                    comms_changed = true;
                }
            }
        }
        if comms_changed {
            wc.view.request_redraw();
        }
        if graph_changed {
            wc.save_session();
        }
        if card_changed || graph_changed {
            wc.view.request_redraw();
        }
        // The physics actor shares this wake: a fresh layout snapshot is waiting to
        // be folded in. The orrery is always shown now, so kick a redraw — `frame()`
        // drains the snapshot and reports whether to keep going (the settle then
        // self-sustains through `needs_redraw`).
        wc.drain_portable_diagnostics();
        wc.view.request_redraw();
        drop(wc);
        // MW3 step 5 (the deferred d2): the drain above ran through the primary's ctx
        // (`ctx()` always bundles the primary), so only the primary was woken. Every
        // secondary window shares the same kernel, caches, and pooled orreries and shows
        // the same physics-animated graph, so this same wake must repaint them too. Fan
        // the redraw out now the ctx borrow has ended (the two-phase shape the d2 note
        // called for).
        self.redraw_secondary_windows();
        // MW3 step 5 chrome fan-out: the sync chip + comms updates were applied to the
        // primary's chrome inline above; replay them onto every *other* window now the
        // ctx borrow has ended. Note a slim leaf is NOT skipped: "slim" omits the
        // *shellbar*, but the leaf keeps the toolbar (where the sync chip lives) and can
        // open the comms pane, so it must see these updates too — driving caught a leaf
        // showing a stale "p2p off" while the primary showed real standing. (MW3 step 5.)
        if latest_sync.is_some() || !comms_updates.is_empty() {
            let primary = self.primary;
            for (id, view) in self.windows.iter_mut() {
                if Some(*id) == primary {
                    continue;
                }
                if let Some(indicator) = &latest_sync {
                    view.chrome_update(|c| c.sync = indicator.clone());
                }
                for update in &comms_updates {
                    apply_comms_to_chrome(view, update);
                }
                view.request_redraw();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // A close request needs no ctx and forks by role: the primary saves the
        // session and exits the app; a secondary just drops its view, leaving the
        // graph and the other windows intact. (MW3.)
        if matches!(event, WindowEvent::CloseRequested) {
            self.request_close(event_loop, window_id);
            return;
        }
        // Route by id: resolve the event's window to its view in the registry. An
        // unknown id (a just-closed window) is dropped. (MW2 (d).)
        let Some(mut wc) = self.window_ctx(window_id) else {
            return;
        };
        match event {
            WindowEvent::Resized(size) => wc.resize(size.width, size.height),
            WindowEvent::Focused(focused) => wc.update_a11y_window_focus(focused),
            // A dropped image file textures the node under it (else the focused node) as a
            // custom sprite face. (Node representation P2 — sprite drop.)
            WindowEvent::DroppedFile(path) => wc.import_sprite_from_file(&path),
            WindowEvent::CursorMoved { position, .. } => {
                wc.view.cursor = (position.x as f32, position.y as f32);
                // A manual window resize in progress: drive it from the move and
                // route nowhere else. (Custom titlebar.)
                if wc.view.resize_drag.is_some() {
                    wc.apply_resize();
                    return;
                }
                // A pending titlebar press that moves past the slop becomes a window
                // drag (the OS takes over from here); below the slop it stays a
                // pending click and the move routes nowhere. (Custom titlebar.)
                if let Some((px, py)) = wc.view.titlebar_press {
                    if (wc.view.cursor.0 - px).hypot(wc.view.cursor.1 - py) > 4.0 {
                        if let Some(window) = wc.view.window.as_ref() {
                            let _ = window.drag_window();
                        }
                        wc.view.titlebar_press = None;
                    }
                    return;
                }
                // A swatch vertex drag in progress reshapes the node's collider hull from
                // the move and routes nowhere else (the swatch owns the gesture). (Swatch —
                // node shape editor, Stage B.)
                if wc.view.swatch_drag.is_some() {
                    wc.drag_swatch_vertex(wc.view.cursor.0, wc.view.cursor.1);
                    return;
                }
                // A row-reorder drag in progress (the configurable menu list) tracks the drop
                // target from the move and routes nowhere else. (Command registry B2.)
                if wc.view.row_reorder_drag.is_some() {
                    wc.drag_row_reorder(wc.view.cursor.0, wc.view.cursor.1);
                    return;
                }
                // Hint the resize edges: the borderless window has no OS frame, so
                // the host sets the resize arrows on hover. (Custom titlebar.)
                wc.update_hover_cursor();
                // Forward to the orrery in content-band coordinates. The orrery is
                // always present (its leaf sits at the band's top-left), so it owns
                // moves it has an in-progress pan / drag for; an active tab drag in
                // the workbench pane takes priority so its drop highlight tracks.
                if wc.view.frame_divider_drag.is_some() {
                    wc.drag_frame_divider();
                } else if wc.view.workbench_gesture {
                    // A workbench tab drag / divider resize in flight: feed the pelt
                    // shell's pointer state machine, which advances the gesture (emitting
                    // DividerMoved while resizing) and carries the drag ghost. (Drag via
                    // pelt TileEvents.)
                    let (cx, cy) = wc.view.cursor;
                    wc.workbench_pointer_move(cx, cy);
                } else if let Some((member, lx, ly)) = wc.scrying_at(wc.view.cursor.0, wc.view.cursor.1)
                {
                    // Hover / drag over the compatibility-view tile feeds the WebView;
                    // the orrery is not panned underneath. (Scrying X2.)
                    wc.view
                        .scrying
                        .forward_mouse(member, lx, ly, scrying_host::MousePress::Move);
                    wc.view.request_redraw();
                } else if let Some((gid, rect)) =
                    wc.orrery_pane_at(wc.view.cursor.0, wc.view.cursor.1)
                {
                    // Route hover to the Orrery pane under the cursor, mapped to that
                    // pane's local space (origin at its leaf rect). A second graph-pane
                    // hovers / fields independently, and its internal cursor stays
                    // current so a wheel-zoom there pivots on the right point. (Window
                    // composition P2 — per-pane input.)
                    let (ox, oy) = (wc.view.cursor.0 - rect[0], wc.view.cursor.1 - rect[1]);
                    // Run the field-hover update, then always redraw on an orrery move so the
                    // per-node hover wash tracks the cursor: the render loop recomputes which
                    // card the cursor is over, and the OrreryRender diff still gates the shell
                    // re-render to actual hover changes. (P0 hover.)
                    wc.pane_orrery_mut(gid).cursor_moved(ox, oy);
                    wc.view.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                // The pointer left the window: drop any field hover so its box doesn't
                // stay stuck on. (Field regions — box-on-interaction.)
                if wc.orrery_mut().clear_active_field() {
                    wc.view.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                wc.view.modifiers = modifiers_from_winit(mods.state());
                let (ctrl, shift, alt) =
                    (wc.view.modifiers.ctrl, wc.view.modifiers.shift, wc.view.modifiers.alt);
                wc.orrery_mut().set_ctrl(ctrl);
                wc.orrery_mut().set_shift(shift);
                // Alt gates the camera orbit drag (Alt+left-drag over the orrery). (Iso orbit.)
                wc.orrery_mut().set_alt(alt);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // A wheel over the compatibility-view tile scrolls the WebView (Win32
                // convention: 120 units per notch); it does not pan the orrery. (X2.)
                if let Some((member, lx, ly)) = wc.scrying_at(wc.view.cursor.0, wc.view.cursor.1) {
                    let delta_y = match delta {
                        MouseScrollDelta::LineDelta(_, y) => (y * 120.0) as i32,
                        MouseScrollDelta::PixelDelta(p) => p.y as i32,
                    };
                    wc.view.scrying.forward_wheel(member, lx, ly, delta_y);
                    wc.view.request_redraw();
                    return;
                }
                // LineDelta is scaled to device px the way the orrery expects;
                // PixelDelta passes through.
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * WHEEL_PAN_SCALE, y * WHEEL_PAN_SCALE),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                // A wheel over a content card scrolls that card (a GPU UV-window
                // shift over its tall texture). Over the orrery pane it drives the
                // orrery (pan, or Ctrl-zoom); over the workbench pane (off a tile) it
                // does nothing.
                let (cx, cy) = wc.view.cursor;
                if wc
                    .roster_leaf_rect()
                    .is_some_and(|r| cx >= r[0] && cx < r[2] && cy >= r[1] && cy < r[3])
                {
                    // Render clamps to the current roster content extent; keeping
                    // the wheel route here prevents a roster scroll from panning
                    // the orrery underneath.
                    wc.view.roster_scroll = (wc.view.roster_scroll - dy).max(0.0);
                    wc.view.request_redraw();
                    return;
                }
                // A wheel over an open settings tile scrolls its body (the
                // `.settings-pane-body` container); serval clips + draws the thumb,
                // and the offset feeds the shell ScrollOffsets + the hit-test. Render
                // clamps it to the content extent. (Menu / pane scroll.)
                if wc
                    .view
                    .settings_rects
                    .iter()
                    .any(|(_, r)| cx >= r[0] && cx < r[2] && cy >= r[1] && cy < r[3])
                {
                    wc.view.settings_scroll = (wc.view.settings_scroll - dy).max(0.0);
                    wc.view.request_redraw();
                    return;
                }
                // A wheel over a scrollable utility pane (inspector / steward /
                // apparatus) scrolls that pane rather than panning the orrery
                // underneath. Render clamps each to its live content extent.
                if let Some(content) = wc
                    .laid_leaves()
                    .into_iter()
                    .find(|l| cx >= l.rect[0] && cx < l.rect[2] && cy >= l.rect[1] && cy < l.rect[3])
                    .map(|l| l.content)
                {
                    let scroll = match content {
                        frame::PaneContent::Inspector => Some(&mut wc.view.inspector_scroll),
                        frame::PaneContent::Steward => Some(&mut wc.view.steward_scroll),
                        frame::PaneContent::Apparatus => Some(&mut wc.view.apparatus_scroll),
                        frame::PaneContent::Trail => Some(&mut wc.view.trail_scroll),
                        frame::PaneContent::Alembic => Some(&mut wc.view.alembic_scroll),
                        _ => None,
                    };
                    if let Some(s) = scroll {
                        *s = (*s - dy).max(0.0);
                        wc.view.request_redraw();
                        return;
                    }
                }
                let over_card = wc
                    .view
                    .content_rects
                    .iter()
                    .find(|(_, r)| cx >= r[0] && cx < r[2] && cy >= r[1] && cy < r[3])
                    .map(|(member, r)| (*member, r[3] - r[1]));
                if let Some((member, visible_h)) = over_card {
                    let max =
                        (wc.shared.content.constellation.content_height(member) as f32 - visible_h).max(0.0);
                    let offset = wc.view.scroll.entry(member).or_insert(0.0);
                    // Wheel up (dy > 0) scrolls toward the top; down toward the bottom.
                    *offset = (*offset - dy).clamp(0.0, max);
                    wc.view.request_redraw();
                } else if let Some((gid, _)) = wc.orrery_pane_at(cx, cy) {
                    // Pan / Ctrl-zoom the Orrery pane under the cursor, routed through the document:
                    // the wheel dispatches to the orrery pane element's `on_wheel` (which queues its
                    // delta), then drains into gyre. A cursor over the workbench / a utility pane
                    // resolves to no Orrery leaf, so the wheel does nothing there. (cond 5 input
                    // bridge; per-pane: a second graph-pane navigates independently.)
                    let th = wc.toolbar_height() as f32;
                    if cy >= th && wc.orrery_wheel_through_document(gid, cx, cy, dx, dy) {
                        wc.view.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                wc.on_mouse_input(state, button);
                // The custom close control sets `pending_exit` (input has no
                // event-loop handle); it's honored below, after the ctx borrow ends,
                // forked by role like `CloseRequested`. (MW3: per-window close.)
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                // While a compatibility-view tile holds the keyboard, route keys into
                // its WebView; Escape releases focus back to the chrome. (Scrying X2.)
                if let Some(member) = wc.view.scrying_input_focus {
                    use winit::keyboard::{Key, NamedKey};
                    if pressed && matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                        wc.view.scrying_input_focus = None;
                        return;
                    }
                    let mods = scrying_host::KeyMods {
                        shift: wc.view.modifiers.shift,
                        ctrl: wc.view.modifiers.ctrl,
                        alt: wc.view.modifiers.alt,
                        meta: wc.view.modifiers.meta,
                    };
                    wc.view.scrying.forward_key(
                        member,
                        scrying_vk(&event),
                        event.text.as_ref().map(|s| s.as_str()),
                        pressed,
                        mods,
                    );
                    wc.view.request_redraw();
                    return;
                }
                if pressed {
                    wc.on_key_pressed(&event.logical_key);
                }
            }
            WindowEvent::Ime(ime) => wc.handle_ime(ime),
            WindowEvent::RedrawRequested => wc.render(),
            _ => {}
        }
        // End the ctx pass explicitly: `WindowCtx`'s `Drop` reads this window's viewports
        // back out of the shared orreries (camera on the view) and releases the `self`
        // borrow, so the registry op below is reachable. (Without this, the `Drop` impl
        // holds the borrow to end of scope and the `pending_exit` check below cannot run.)
        drop(wc);
        // The custom close control set `pending_exit` on this window's view; honor it
        // the same way as `CloseRequested`, forked by role. The ctx borrow has ended,
        // so the registry op is reachable. Queued `SpawnWindow`s drain in
        // `about_to_wait`. (MW3: per-window close.)
        if self
            .windows
            .get(&window_id)
            .is_some_and(|v| v.pending_exit)
        {
            self.request_close(event_loop, window_id);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Apply the cross-window commands queued during this event batch (spawn /
        // close a window), now that every per-window ctx borrow has ended. (MW3.)
        self.apply(event_loop);
    }
}

impl Shell {
    /// Wake every non-primary window to repaint. The actor-drain in `user_event` writes
    /// shared state through the primary's ctx; the secondaries show the same pooled
    /// orreries, caches, and physics, so they redraw on the same wake. A no-op while only
    /// the primary exists. (Multi-window MW3 step 5 — the `user_event` redraw fan-out.)
    fn redraw_secondary_windows(&self) {
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
    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        mut view: super::window_view::WindowView,
    ) -> Option<(WindowId, Arc<Window>)> {
        // Borderless: the OS title bar (and its accent border) is off; the chrome's
        // toolbar band is the titlebar, with host-drawn window controls + edge
        // resize (see `titlebar` + `input`). A min size keeps the bar usable.
        let attributes = Window::default_attributes()
            .with_title("Meerkat — Mere chrome on serval")
            .with_decorations(false)
            .with_visible(false)
            .with_min_inner_size(PhysicalSize::new(480u32, 320u32))
            .with_inner_size(PhysicalSize::new(view.width, view.height));
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
    fn apply(&mut self, event_loop: &ActiveEventLoop) {
        if self.commands.is_empty() {
            return;
        }
        for command in std::mem::take(&mut self.commands) {
            match command {
                super::ShellCommand::SpawnWindow => self.spawn_window(event_loop),
                super::ShellCommand::CloseWindow(id) => self.close_window(id),
                super::ShellCommand::CreateSession => {
                    self.create_session();
                }
                super::ShellCommand::SwitchSession(id) => self.switch_session(id),
                super::ShellCommand::CycleSession(forward) => self.cycle_session(forward),
                super::ShellCommand::CloseSession(id) => self.close_session(id),
                super::ShellCommand::OpenGraphBeside(id) => self.open_graph_beside(id),
                super::ShellCommand::OpenEngramBeside(id) => self.open_engram_beside(&id),
            }
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
    fn spawn_window(&mut self, event_loop: &ActiveEventLoop) {
        // Can't happen via the keyboard verb (it's post-resume), but guard anyway.
        if self.render_core.is_none() {
            return;
        }
        let view = self.build_window_view();
        if let Some((id, window)) = self.create_window(event_loop, view) {
            // Install this secondary's own AccessKit bridge *before* showing it (the
            // "adapter before first show" rule, same order the primary uses in
            // `resumed`): build its a11y projection through its ctx, then install the
            // adapter against its window. `window_ctx` mints the per-window bridge on
            // first access. (MW3 step 6 — per-window a11y.)
            if let Some(mut wc) = self.window_ctx(id) {
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
    fn close_window(&mut self, id: WindowId) {
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

    /// Honor a close request for `window_id`, forked by role: the primary saves its
    /// session and exits the app; a secondary just drops its view, leaving the graph
    /// and the other windows intact. Both `CloseRequested` and the custom close
    /// control route here. (MW3.)
    fn request_close(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId) {
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

impl super::WindowCtx<'_> {
    /// Drive a manual window resize from the current cursor (custom titlebar). The
    /// opposite edge(s) of the press-time rect stay anchored; the dragged edge(s)
    /// follow the cursor by its screen-space delta from the press, clamped to the
    /// minimum size. Left/top edges move the window origin (`set_outer_position`),
    /// right/bottom only grow it. The follow-up `Resized` event reconfigures the
    /// surface.
    fn apply_resize(&self) {
        let Some(drag) = self.view.resize_drag else { return };
        let Some(window) = self.view.window.as_ref() else {
            return;
        };
        use winit::window::ResizeDirection as D;
        let outer = window
            .outer_position()
            .map(|p| (p.x as f32, p.y as f32))
            .unwrap_or((drag.start_outer.0 as f32, drag.start_outer.1 as f32));
        let screen_x = outer.0 + self.view.cursor.0;
        let screen_y = outer.1 + self.view.cursor.1;
        let dx = screen_x - drag.start_cursor_screen.0;
        let dy = screen_y - drag.start_cursor_screen.1;
        let start_left = drag.start_outer.0 as f32;
        let start_top = drag.start_outer.1 as f32;
        let start_right = start_left + drag.start_size.0 as f32;
        let start_bottom = start_top + drag.start_size.1 as f32;
        let (min_w, min_h) = (480.0_f32, 320.0_f32);
        let mut left = start_left;
        let mut top = start_top;
        let mut right = start_right;
        let mut bottom = start_bottom;
        match drag.dir {
            D::West | D::NorthWest | D::SouthWest => {
                left = (start_left + dx).min(start_right - min_w)
            }
            D::East | D::NorthEast | D::SouthEast => {
                right = (start_right + dx).max(start_left + min_w)
            }
            _ => {}
        }
        match drag.dir {
            D::North | D::NorthWest | D::NorthEast => {
                top = (start_top + dy).min(start_bottom - min_h)
            }
            D::South | D::SouthWest | D::SouthEast => {
                bottom = (start_bottom + dy).max(start_top + min_h)
            }
            _ => {}
        }
        let new_w = (right - left).round().max(min_w) as u32;
        let new_h = (bottom - top).round().max(min_h) as u32;
        window.set_outer_position(PhysicalPosition::new(
            left.round() as i32,
            top.round() as i32,
        ));
        let _ = window.request_inner_size(PhysicalSize::new(new_w, new_h));
    }

    /// Update the cursor for a hover over the window's resize edges (custom
    /// titlebar). Edges show the matching resize arrows; the window controls and
    /// the interior keep the default arrow. Only re-sets the cursor on a change.
    fn update_hover_cursor(&mut self) {
        let (x, y) = self.view.cursor;
        let band_h = self.toolbar_height();
        let icon = if titlebar::control_at(x, y, self.view.width, band_h).is_some() {
            CursorIcon::Default
        } else if let Some(dir) = titlebar::resize_dir_at(x, y, self.view.width, self.view.height) {
            titlebar::resize_cursor(dir)
        } else if self.over_link(x, y) {
            CursorIcon::Pointer // a hand over a clickable link (browser feel)
        } else {
            CursorIcon::Default
        };
        if icon != self.view.cursor_icon {
            self.view.cursor_icon = icon;
            if let Some(window) = self.view.window.as_ref() {
                window.set_cursor(icon);
            }
        }
    }

    pub(super) fn drain_portable_diagnostics(&mut self) {
        while let Ok(event) = self.shared.inbox.diagnostics.try_recv() {
            self.shared.observability.record_portable_event(event);
        }
    }
}
