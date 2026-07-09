/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! IME (input-method editor) wiring for the chrome (grab-bag G2.1).
//!
//! The library half is complete one layer down: [`xilem_serval::TextInput`] holds
//! an IME preedit (rendered inline at the caret, out of the committed buffer), and
//! the C1 caret seam ([`crate::pane_session::PaneSession::caret_rect`]) gives the
//! caret's screen rect. This is the meerkat call site, mirroring pelt-live: enable
//! the platform IME (`set_ime_allowed`, in `create_window`), route
//! [`winit::event::Ime`] into the focused chrome field, and point the candidate
//! window at the caret (`set_ime_cursor_area`).
//!
//! meerkat's twist over pelt-live's single field: the chrome has several text
//! inputs (omnibar / palette / comms), so preedit + the cursor area target the
//! *focused* one, resolved the same way [`WindowCtx::caret_field`] resolves the
//! painted caret.

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::Ime;
use xilem_serval::{Key, KeyEvent};

use crate::serval_render::CARET_WIDTH;

use super::WindowCtx;

/// Which chrome text field currently holds focus — mirrors the field mapping in
/// [`WindowCtx::caret_field`], so the IME preedit and the painted caret target the
/// same input.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusedField {
    Omnibar,
    Palette,
    CommsTo,
    CommsBody,
    CommsDraft,
    KnotEditor,
}

impl WindowCtx<'_> {
    /// Route a winit IME event into the focused chrome field.
    ///
    /// `Preedit` shows the in-progress composition inline at the caret (not yet
    /// committed); `Commit` clears it and inserts the committed text via the
    /// focus-routed key path (a `Character` event, the same path Latin typing
    /// takes); `Disabled` drops any composition. The candidate window follows the
    /// caret after preedit / commit. (G2.1 IME T1–T3.)
    pub(super) fn handle_ime(&mut self, ime: Ime) {
        match ime {
            Ime::Preedit(text, _cursor) => {
                self.set_focused_preedit(text);
                self.update_ime_cursor_area();
                self.view.request_redraw();
            }
            Ime::Commit(text) => {
                self.set_focused_preedit(String::new());
                if !text.is_empty() {
                    self.multi
                        .dispatch_key(self.view.projection_id, KeyEvent::new(Key::Character(text)));
                }
                self.update_ime_cursor_area();
                self.view.request_redraw();
            }
            Ime::Disabled => {
                self.set_focused_preedit(String::new());
                self.view.request_redraw();
            }
            // The platform turned IME on for the window; nothing to fold in until a
            // preedit / commit arrives.
            Ime::Enabled => {}
        }
    }

    /// The focused chrome field, by the same mapping as
    /// [`caret_field`](Self::caret_field). `None` when nothing is focused.
    pub(crate) fn focused_field_kind(&self) -> Option<FocusedField> {
        let focus = Some(self.multi.focus(self.view.projection_id)?);
        Some(if focus == self.input_under_class("comms-new-to") {
            FocusedField::CommsTo
        } else if focus == self.input_under_class("comms-new-body") {
            FocusedField::CommsBody
        } else if focus == self.input_under_class("comms-compose") {
            FocusedField::CommsDraft
        } else if focus == self.input_under_class("knot-editor-source") {
            FocusedField::KnotEditor
        } else if self.chrome().palette_open {
            FocusedField::Palette
        } else {
            FocusedField::Omnibar
        })
    }

    /// Set (or clear, with an empty string) the IME preedit on the focused field.
    /// A no-op when nothing is focused.
    fn set_focused_preedit(&mut self, text: String) {
        let Some(kind) = self.focused_field_kind() else {
            return;
        };
        self.chrome_update(move |c| {
            let field = match kind {
                FocusedField::Omnibar => &mut c.omnibar,
                FocusedField::Palette => &mut c.palette_input,
                FocusedField::CommsTo => &mut c.comms_new_to,
                FocusedField::CommsBody => &mut c.comms_new_body,
                FocusedField::CommsDraft => &mut c.comms_draft,
                FocusedField::KnotEditor => &mut c.knot_source,
            };
            field.set_preedit(text);
        });
    }

    /// Set the focused field's caret to char-boundary byte `byte` (clamped); `extend` grows
    /// the selection. The write half of [`caret_field`](Self::caret_field), routed through the
    /// same `chrome_update` mapping as [`set_focused_preedit`](Self::set_focused_preedit).
    fn set_focused_caret_byte(&mut self, byte: usize, extend: bool) {
        let Some(kind) = self.focused_field_kind() else {
            return;
        };
        self.chrome_update(move |c| {
            let field = match kind {
                FocusedField::Omnibar => &mut c.omnibar,
                FocusedField::Palette => &mut c.palette_input,
                FocusedField::CommsTo => &mut c.comms_new_to,
                FocusedField::CommsBody => &mut c.comms_new_body,
                FocusedField::CommsDraft => &mut c.comms_draft,
                FocusedField::KnotEditor => &mut c.knot_source,
            };
            field.set_caret_byte(byte, extend);
        });
    }

    /// Soft-wrap ArrowUp (`delta = -1`) / ArrowDown (`delta = +1`) for the focused knot-editor
    /// textarea: move the caret one *visual* row (parley's wrapped rows) via the chrome session's
    /// retained layout, keeping a sticky goal column — instead of the buffer handler's hard-`\n`
    /// move (which jumps a whole wrapped paragraph). `extend` grows the selection. Returns whether
    /// it handled the key: `false` (so the caller routes it normally) unless the knot editor holds
    /// focus and its layout knows the caret's row.
    ///
    /// The goal x is reused only across an *uninterrupted run* — the caret still at the byte the
    /// last vertical move left it. Any edit / click / horizontal move changes the byte, so the
    /// goal reseeds from the caret's current x (no scattered resets needed). (Soft-wrap caret nav.)
    pub(super) fn soft_wrap_nav(&mut self, delta: isize, extend: bool) -> bool {
        if !matches!(self.focused_field_kind(), Some(FocusedField::KnotEditor)) {
            return false;
        }
        let Some(node) = self.multi.focus(self.view.projection_id) else {
            return false;
        };
        let caret_byte = self.caret_field(node).caret_byte_in_render();
        let goal = match self.view.soft_wrap_goal {
            Some((last_byte, gx)) if last_byte == caret_byte => Some(gx),
            _ => None,
        };
        let moved = {
            let Some(session) = self.view.chrome_session.as_ref() else {
                return false;
            };
            session.caret_byte_vertical(node, caret_byte, delta, goal)
        };
        let Some((new_byte, goal_x)) = moved else {
            return false;
        };
        self.view.soft_wrap_goal = Some((new_byte, goal_x));
        self.set_focused_caret_byte(new_byte, extend);
        self.view.request_redraw();
        true
    }

    /// Point the IME candidate window at the focused field's caret via the chrome
    /// session's [`caret_rect`](crate::pane_session::PaneSession::caret_rect) — the
    /// same rect the painted caret uses, so the popup sits exactly under it. No-op
    /// without a focused field or a built chrome session.
    fn update_ime_cursor_area(&self) {
        let (Some(window), Some(node)) = (
            self.view.window.as_ref(),
            self.multi.focus(self.view.projection_id),
        ) else {
            return;
        };
        // Caret byte within the field's *rendered* text (after any preedit), so the
        // candidate window sits after the composing run.
        let caret_byte = self.caret_field(node).caret_byte_in_render();
        let dom = self.view.dom.borrow();
        let Some(session) = self.view.chrome_session.as_ref() else {
            return;
        };
        if let Some((x, y, w, h)) = session.caret_rect(&dom, node, caret_byte, CARET_WIDTH) {
            window.set_ime_cursor_area(
                PhysicalPosition::new(x, y),
                PhysicalSize::new(w.max(1.0), h.max(1.0)),
            );
        }
    }
}
