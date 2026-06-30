/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mouse-button routing: the region dispatcher (on_mouse_input). Kept whole pending runtime-verified pass extraction.

use super::*;

impl WindowCtx<'_> {
    /// Route a mouse button press/release by region. A left press in the chrome
    /// band (toolbar + any open dropdown) hit-tests + dispatches the chrome; any
    /// other press in the content band, and every release, goes to the orrery in
    /// content-band coordinates (its viewport top sits at the toolbar bottom).
    pub(crate) fn on_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        match state {
            ElementState::Pressed => self.on_mouse_press(button),
            ElementState::Released => self.on_mouse_release(button),
        }
    }
}

mod press;
mod release;
