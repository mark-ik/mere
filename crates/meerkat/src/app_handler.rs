/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `ApplicationHandler` impl for [`App`](super::App). Factored from
//! `main.rs` to keep files under the workspace 600-LOC ceiling.

use std::sync::Arc;

use netrender::NetrenderOptions;
use orrery::WHEEL_PAN_SCALE;
use serval_winit_host::{SurfaceHost, modifiers_from_winit};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{CursorIcon, Window, WindowId};

use super::observability::Severity;
use super::{App, comms_host, fetch, sync, titlebar};

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        // Borderless: the OS title bar (and its accent border) is off; the chrome's
        // toolbar band is the titlebar, with host-drawn window controls + edge
        // resize (see `titlebar` + `input`). A min size keeps the bar usable.
        let attributes = Window::default_attributes()
            .with_title("Meerkat — Mere chrome on serval")
            .with_decorations(false)
            .with_visible(false)
            .with_min_inner_size(PhysicalSize::new(480u32, 320u32))
            .with_inner_size(PhysicalSize::new(self.width, self.height));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create meerkat window"),
        );
        let size = window.inner_size();
        self.width = size.width.max(1);
        self.height = size.height.max(1);

        // The shared serval-on-winit present stack: wgpu + netrender boot, surface
        // configured at the window size.
        let options = NetrenderOptions {
            tile_cache_size: Some(64),
            enable_vello: true,
            ..Default::default()
        };
        match SurfaceHost::boot(window.clone(), self.width, self.height, options) {
            Ok(host) => self.host = Some(host),
            Err(err) => {
                eprintln!("[meerkat] {err}");
                event_loop.exit();
                return;
            }
        }
        let initial_a11y = self.build_a11y_projection().tree_update();
        match self.a11y_bridge.install(&window, initial_a11y) {
            Ok(()) => self.observability.record_probe(
                "a11y_bridge",
                "installed",
                "OS AccessKit bridge installed",
            ),
            Err(err) => self.observability.record_probe(
                "a11y_bridge",
                "degraded",
                format!("OS AccessKit bridge unavailable: {err}"),
            ),
        }
        self.refresh_a11y_summary();
        window.set_visible(true);
        window.request_redraw();
        self.window = Some(window);

        // Show the restored focused node's content from the durable cache (so a
        // reload re-opens its card without a navigation). A fresh `mere://welcome`
        // focus is not fetchable, so this is a no-op there.
        if let Some(url) = self.orrery.focused_url().map(str::to_string) {
            self.ensure_content(&url);
        }
    }

    /// Drain completed fetches (delivery model 2): a worker woke us via the proxy;
    /// fold each outcome into the content cache and re-render the card.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        self.drain_portable_diagnostics();
        // The kernel inbox dispatch: the one documented place that applies what the
        // actors tell the kernel. Each typed stream is drained and folded into
        // canonical state here on the kernel thread; the actors never touch it.
        let mut card_changed = false;
        let mut graph_changed = false;
        // One `FetchUpdate` stream carries both page documents and subresources.
        while let Ok(update) = self.inbox.fetch.try_recv() {
            match update {
                fetch::FetchUpdate::Page(outcome) => {
                    let state = match outcome.result {
                        Ok(fetched) => {
                            // Persist so a reload shows this page without
                            // re-fetching. Linked-data harvest now happens in the
                            // content actor (on `Show`), which ships a `Contribution`.
                            self.save_cached(
                                &outcome.url,
                                fetched.content_type.clone(),
                                fetched.body.as_bytes(),
                            );
                            self.observability.record_actor(
                                "fetch",
                                "succeeded",
                                Some(outcome.url.clone()),
                            );
                            fetch::ContentState::Ready(fetched)
                        }
                        Err(reason) => {
                            let detail = format!("{}: {reason}", outcome.url);
                            self.observability.record_actor(
                                "fetch",
                                "failed",
                                Some(detail.clone()),
                            );
                            self.observability.record_diagnostic(
                                "meerkat.actor.fetch.failed",
                                Severity::Warn,
                                detail,
                            );
                            fetch::ContentState::Failed(reason)
                        }
                    };
                    self.content.insert(outcome.url, state);
                    card_changed = true;
                }
                // A subresource (page CSS / media): persist it (its content-type is
                // unknown here) so the page's assets survive restart, then broadcast
                // the bytes to every active node's actor — the fetch stream is keyed
                // by URL, not by which node wanted it; each actor dedups via its own
                // resource store and only the one that wanted it re-renders.
                fetch::FetchUpdate::Subresource(sub) => {
                    self.observability
                        .record_actor("fetch", "subresource", Some(sub.url.clone()));
                    self.save_cached(&sub.url, None, &sub.bytes);
                    self.constellation.broadcast_resource(&sub.url, &sub.bytes);
                }
            }
        }
        // Drain every active node's actor in one pass: scenes land in the pool, the
        // wanted subresources + harvested contributions come back for the host.
        let drained = self.constellation.drain();
        card_changed |= drained.any_scene;
        if !drained.respawned.is_empty() {
            // A content tile's actor died (panic, isolated to its thread) and the
            // pool respawned it; redraw so the next frame re-Shows it (self-healing).
            tracing::warn!(
                count = drained.respawned.len(),
                "respawned crashed content tile(s)"
            );
            self.observability.record_actor(
                "content",
                "respawned",
                Some(format!("count={}", drained.respawned.len())),
            );
            self.observability.record_diagnostic(
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
                if let Some(stored) = self.load_cached(&url) {
                    self.constellation.send_resource(member, url, stored.body);
                } else {
                    self.fetch_handle
                        .command(fetch::FetchCommand::Subresource(url));
                }
            }
        }
        if !drained.contributions.is_empty() {
            graph_changed |= self.orrery.ingest_graph(|g| {
                let mut changed = false;
                for contribution in &drained.contributions {
                    let outcome = linked_data::apply_contribution(g, contribution);
                    changed |= outcome.nodes_created > 0 || outcome.edges_asserted > 0;
                }
                changed
            });
        }
        // P2P sync status (S5.0): the same wake also carries lane-status changes.
        // Fold the latest into the chrome chip (the host owns the mutation).
        let mut latest_sync = None;
        while let Ok(update) = self.inbox.sync.try_recv() {
            self.observability.record_actor(
                "sync",
                "status",
                Some(format!(
                    "syncing={};ops={}",
                    update.status.syncing, update.status.ops_received
                )),
            );
            latest_sync = Some(sync::to_indicator(&update.status, sync::LANE_LABEL));
        }
        if let Some(indicator) = latest_sync {
            self.runner.update(|c| c.sync = indicator.clone());
            self.request_redraw();
        }
        // Comms (P6c): the comms actor delivers conversation lists + threads here;
        // fold each into the docked pane (the host owns the chrome mutation).
        let mut comms_changed = false;
        while let Ok(update) = self.inbox.comms.try_recv() {
            match update {
                comms_host::CommsUpdate::Inbox(inbox) => {
                    self.observability.record_actor(
                        "comms",
                        "inbox",
                        Some(format!("conversations={}", inbox.conversations.len())),
                    );
                    self.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("inbox".to_string()),
                    );
                    self.runner.update(|c| c.comms.set_inbox(inbox.clone()));
                    comms_changed = true;
                }
                comms_host::CommsUpdate::Thread(id, messages) => {
                    self.observability.record_actor(
                        "comms",
                        "thread",
                        Some(format!("{} messages={}", id.key, messages.len())),
                    );
                    self.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("thread".to_string()),
                    );
                    self.runner.update(|c| {
                        if c.comms.selected() == Some(&id) {
                            c.comms.set_thread(messages.clone());
                        }
                    });
                    comms_changed = true;
                }
                comms_host::CommsUpdate::Sent(_id) => {
                    self.observability.record_actor("comms", "sent", None);
                    self.observability
                        .record_actor("comms", "succeeded", Some("sent".to_string()));
                    self.runner.update(|c| c.clear_comms_draft());
                    comms_changed = true;
                }
                comms_host::CommsUpdate::SendOutcome(line) => {
                    self.observability
                        .record_actor("comms", "send_outcome", Some(line.clone()));
                    self.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("send_outcome".to_string()),
                    );
                    self.runner
                        .update(|c| c.comms.set_send_status(line.clone()));
                    comms_changed = true;
                }
                comms_host::CommsUpdate::Identity {
                    misfin_address,
                    cabal_ticket,
                } => {
                    self.observability.record_actor(
                        "comms",
                        "identity",
                        Some(format!(
                            "misfin={};cabal={}",
                            misfin_address,
                            cabal_ticket.is_some()
                        )),
                    );
                    self.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("identity".to_string()),
                    );
                    self.runner.update(|c| {
                        c.comms
                            .set_identity(misfin_address.clone(), cabal_ticket.clone())
                    });
                    comms_changed = true;
                }
            }
        }
        if comms_changed {
            self.request_redraw();
        }
        if graph_changed {
            self.save_session();
        }
        if card_changed || graph_changed {
            self.request_redraw();
        }
        // The physics actor shares this wake: a fresh layout snapshot is waiting to
        // be folded in. The orrery is always shown now, so kick a redraw — `frame()`
        // drains the snapshot and reports whether to keep going (the settle then
        // self-sustains through `needs_redraw`).
        self.drain_portable_diagnostics();
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.window.as_ref().map(|w| w.id()) != Some(window_id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.save_session();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::Focused(focused) => self.update_a11y_window_focus(focused),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                // A manual window resize in progress: drive it from the move and
                // route nowhere else. (Custom titlebar.)
                if self.resize_drag.is_some() {
                    self.apply_resize();
                    return;
                }
                // A pending titlebar press that moves past the slop becomes a window
                // drag (the OS takes over from here); below the slop it stays a
                // pending click and the move routes nowhere. (Custom titlebar.)
                if let Some((px, py)) = self.titlebar_press {
                    if (self.cursor.0 - px).hypot(self.cursor.1 - py) > 4.0 {
                        if let Some(window) = self.window.as_ref() {
                            let _ = window.drag_window();
                        }
                        self.titlebar_press = None;
                    }
                    return;
                }
                // Hint the resize edges: the borderless window has no OS frame, so
                // the host sets the resize arrows on hover. (Custom titlebar.)
                self.update_hover_cursor();
                // Forward to the orrery in content-band coordinates. The orrery is
                // always present (its leaf sits at the band's top-left), so it owns
                // moves it has an in-progress pan / drag for; an active tab drag in
                // the workbench pane takes priority so its drop highlight tracks.
                let th = self.toolbar_height() as f32;
                if self.frame_divider_drag.is_some() {
                    self.drag_frame_divider();
                } else if self.divider_drag.is_some() {
                    self.drag_divider();
                } else if self.tab_drag.is_some() {
                    // Follow the drag: the drop-target highlight tracks the pointer.
                    self.request_redraw();
                } else if self.orrery.cursor_moved(self.cursor.0, self.cursor.1 - th) {
                    self.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = modifiers_from_winit(mods.state());
                self.orrery.set_ctrl(self.modifiers.ctrl);
                self.orrery.set_shift(self.modifiers.shift);
            }
            WindowEvent::MouseWheel { delta, .. } => {
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
                let (cx, cy) = self.cursor;
                let over_card = self
                    .content_rects
                    .iter()
                    .find(|(_, r)| cx >= r[0] && cx < r[2] && cy >= r[1] && cy < r[3])
                    .map(|(member, r)| (*member, r[3] - r[1]));
                if let Some((member, visible_h)) = over_card {
                    let max =
                        (self.constellation.content_height(member) as f32 - visible_h).max(0.0);
                    let offset = self.scroll.entry(member).or_insert(0.0);
                    // Wheel up (dy > 0) scrolls toward the top; down toward the bottom.
                    *offset = (*offset - dy).clamp(0.0, max);
                    self.request_redraw();
                } else {
                    let th = self.toolbar_height() as f32;
                    let in_workbench = self
                        .workbench_leaf_rect()
                        .is_some_and(|wr| cx >= wr[0] && cx < wr[2] && cy >= wr[1] && cy < wr[3]);
                    if cy >= th && !in_workbench && self.orrery.wheel(dx, dy) {
                        self.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.on_mouse_input(state, button);
                // The custom close control sets `pending_exit` (input has no
                // event-loop handle); honor it here, saving the session as the OS
                // CloseRequested path does.
                if self.pending_exit {
                    self.save_session();
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    self.on_key_pressed(&event.logical_key);
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }
}

impl App {
    /// Drive a manual window resize from the current cursor (custom titlebar). The
    /// opposite edge(s) of the press-time rect stay anchored; the dragged edge(s)
    /// follow the cursor by its screen-space delta from the press, clamped to the
    /// minimum size. Left/top edges move the window origin (`set_outer_position`),
    /// right/bottom only grow it. The follow-up `Resized` event reconfigures the
    /// surface.
    fn apply_resize(&self) {
        let Some(drag) = self.resize_drag else { return };
        let Some(window) = self.window.as_ref() else {
            return;
        };
        use winit::window::ResizeDirection as D;
        let outer = window
            .outer_position()
            .map(|p| (p.x as f32, p.y as f32))
            .unwrap_or((drag.start_outer.0 as f32, drag.start_outer.1 as f32));
        let screen_x = outer.0 + self.cursor.0;
        let screen_y = outer.1 + self.cursor.1;
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
        let (x, y) = self.cursor;
        let band_h = self.toolbar_height();
        let icon = if titlebar::control_at(x, y, self.width, band_h).is_some() {
            CursorIcon::Default
        } else if let Some(dir) = titlebar::resize_dir_at(x, y, self.width, self.height) {
            titlebar::resize_cursor(dir)
        } else {
            CursorIcon::Default
        };
        if icon != self.cursor_icon {
            self.cursor_icon = icon;
            if let Some(window) = self.window.as_ref() {
                window.set_cursor(icon);
            }
        }
    }

    pub(super) fn drain_portable_diagnostics(&mut self) {
        while let Ok(event) = self.diagnostics_rx.try_recv() {
            self.observability.record_portable_event(event);
        }
    }
}
