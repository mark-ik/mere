/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! winit ApplicationHandler for Shell: lifecycle + event dispatch.

use super::*;

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
        self.on_user_event();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.on_window_event(event_loop, window_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Apply the cross-window commands queued during this event batch (spawn /
        // close a window), now that every per-window ctx borrow has ended. (MW3.)
        self.apply(event_loop);
    }
}
