/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! WindowCtx: resize apply, hover cursor, portable diagnostics drain.

use super::*;

impl crate::WindowCtx<'_> {
    /// Drive a manual window resize from the current cursor (custom titlebar). The
    /// opposite edge(s) of the press-time rect stay anchored; the dragged edge(s)
    /// follow the cursor by its screen-space delta from the press, clamped to the
    /// minimum size. Left/top edges move the window origin (`set_outer_position`),
    /// right/bottom only grow it. The follow-up `Resized` event reconfigures the
    /// surface.
    pub(crate) fn apply_resize(&self) {
        let Some(drag) = self.view.resize_drag else {
            return;
        };
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
    pub(crate) fn update_hover_cursor(&mut self) {
        let (x, y) = self.view.cursor;
        let band_h = self.current_toolbar_height();
        let icon = if titlebar::control_at(
            x,
            y,
            self.view.width,
            band_h,
            self.shared.presentation.ui_scale(),
        )
        .is_some()
        {
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

    pub(crate) fn drain_portable_diagnostics(&mut self) {
        while let Ok(event) = self.shared.inbox.diagnostics.try_recv() {
            self.shared.observability.record_portable_event(event);
        }
    }
}
