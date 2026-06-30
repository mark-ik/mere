/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shell::on_window_event — per-window event dispatch (extracted from window_event).

use super::*;

impl Shell {
    pub(crate) fn on_window_event(
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
            // The window moved to a display with a different DPI (or the OS scale
            // changed): refold it into the chrome scale. (Auto-DPI D1/D3.)
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                wc.set_dpi_scale(scale_factor as f32)
            }
            WindowEvent::Focused(focused) => wc.update_a11y_window_focus(focused),
            // A dropped image file textures the node under it (else the focused node) as a
            // custom sprite face. (Node representation P2 — sprite drop.)
            WindowEvent::DroppedFile(path) => wc.import_sprite_from_file(&path),
            WindowEvent::CursorMoved { position, .. } => {
                wc.view.cursor = (position.x as f32, position.y as f32);
                // A tear-out drag in progress: just reposition the ghost at the new cursor
                // (the render repositions the `.tear-ghost` pill) and route nowhere else.
                // (Tear-out gestures, GA-5.)
                if wc.view.tear_out_drag.is_some() {
                    wc.view.request_redraw();
                    return;
                }
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
                } else if let Some((member, lx, ly)) =
                    wc.scrying_at(wc.view.cursor.0, wc.view.cursor.1)
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
                let (ctrl, shift, alt) = (
                    wc.view.modifiers.ctrl,
                    wc.view.modifiers.shift,
                    wc.view.modifiers.alt,
                );
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
                // Chrome-pane wheel: the engine hit-tests the cursor to the nearest
                // `overflow:scroll` container (roster / settings body / inspector / steward /
                // apparatus / trail / alembic) and scrolls it, clamping to its content extent
                // and chaining — one `scroll_at` replacing the per-pane rect-routing + the
                // manual f32 offsets + the manual lower-clamp. It returns `false` when no scroll
                // container under the cursor moved (the cursor is over the orrery / a content
                // card, or the chrome itself does not scroll), so the wheel falls through to the
                // branches below. The host f32 convention was `offset -= dy`, so feed `-dy` to
                // the engine's additive `scroll_at`. The next render paints + hit-tests at the
                // engine's retained `element_scroll`. (Host-scroll P2.)
                let pane_scrolled = {
                    let view = &mut wc.view;
                    let dom = view.dom.borrow();
                    view.chrome_session
                        .as_mut()
                        .is_some_and(|s| s.scroll_at(&dom, cx, cy, 0.0, -dy))
                };
                if pane_scrolled {
                    wc.view.request_redraw();
                    return;
                }
                let over_card = wc
                    .view
                    .content_rects
                    .iter()
                    .find(|(_, r)| cx >= r[0] && cx < r[2] && cy >= r[1] && cy < r[3])
                    .map(|(member, r)| (*member, r[3] - r[1]));
                if let Some((member, visible_h)) = over_card {
                    let max = (wc.member_content_height(member, visible_h) - visible_h).max(0.0);
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
                    let th = wc.current_toolbar_height() as f32;
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
        if self.windows.get(&window_id).is_some_and(|v| v.pending_exit) {
            self.request_close(event_loop, window_id);
        }
    }
}
