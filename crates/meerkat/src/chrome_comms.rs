/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Chrome: comms pane, knot editor, new-message + cabal connect.

use super::*;

/// Undo-history depth for the knot editor. A note is short and each entry is a
/// whole-buffer clone, so a bounded stack keeps memory flat without limiting real use.
const KNOT_UNDO_CAP: usize = 200;

impl Chrome {
    /// Toggle the docked comms pane open/closed. Opening records a `Refresh` so
    /// the host loads the latest conversation list.
    pub fn toggle_comms(&mut self) {
        self.comms.toggle();
        if self.comms.is_open() {
            self.comms_intent = Some(CommsIntent::Refresh);
        }
    }

    /// Close the comms pane.
    pub fn close_comms(&mut self) {
        self.comms.close();
    }

    /// Open the docked knot editor, loading `source` into its buffer.
    pub fn open_knot_editor(&mut self, source: impl Into<String>) {
        self.knot_source = TextInput::new(source);
        self.knot_editor_open = true;
        self.knot_target = None;
        self.knot_editor_label = "Editor".to_string();
        self.knot_editor_rect = None;
        self.knot_save_requested = false;
        self.knot_editor_preview = false;
        self.reset_knot_history();
    }

    /// Open the docked knot editor against a graph member's authored body.
    pub fn open_knot_editor_for(
        &mut self,
        target: GraphMemberId,
        label: impl Into<String>,
        source: impl Into<String>,
    ) {
        self.knot_source = TextInput::new(source);
        self.knot_editor_open = true;
        self.knot_target = Some(target);
        self.knot_editor_label = label.into();
        self.knot_editor_rect = None;
        self.knot_save_requested = false;
        self.knot_editor_preview = false;
        self.reset_knot_history();
    }

    /// Close the knot editor.
    pub fn close_knot_editor(&mut self) {
        self.knot_editor_open = false;
        self.knot_target = None;
        self.knot_editor_rect = None;
        self.knot_save_requested = false;
        self.knot_editor_preview = false;
        self.reset_knot_history();
    }

    /// Flip between the source-edit and rendered-preview views of the open note. A
    /// no-op when the editor is closed. Preview only makes sense over a bound tile
    /// (an unbound scratch note has no tile behind to reveal), so it stays on edit
    /// when there is no `knot_target`. (Djot editor — Phase 2 toggle source/preview.)
    pub fn toggle_knot_editor_preview(&mut self) {
        if self.knot_editor_open && self.knot_target.is_some() {
            self.knot_editor_preview = !self.knot_editor_preview;
        }
    }

    /// Clear the editor's undo/redo history (on open/close, so a fresh note never
    /// undoes into the previous one).
    fn reset_knot_history(&mut self) {
        self.knot_undo.clear();
        self.knot_redo.clear();
        self.knot_coalescing = false;
    }

    /// Record the pre-edit source on the undo stack, before a mutating edit applies.
    /// `coalesce_insert` is true for a character/space insert: a run of them coalesces
    /// into one undo entry (so a burst of typing undoes as a unit), by skipping the push
    /// while already coalescing. A non-insert edit (delete, newline) passes `false`, so it
    /// is its own undo step and ends any insert run. Any push clears the redo stack and
    /// caps the history. (Djot editor — Phase 2 undo/redo.)
    pub fn knot_edit_snapshot(&mut self, coalesce_insert: bool) {
        if coalesce_insert && self.knot_coalescing {
            return;
        }
        self.knot_undo.push(self.knot_source.clone());
        if self.knot_undo.len() > KNOT_UNDO_CAP {
            self.knot_undo.remove(0);
        }
        self.knot_redo.clear();
        self.knot_coalescing = coalesce_insert;
    }

    /// End the current insert-coalescing run without snapshotting — for a caret move, so
    /// the next insert starts a fresh undo group even though nothing was deleted.
    pub fn knot_break_coalesce(&mut self) {
        self.knot_coalescing = false;
    }

    /// Undo the last edit: restore the top undo snapshot, moving the current source onto
    /// the redo stack. Returns whether anything was undone.
    pub fn knot_undo_apply(&mut self) -> bool {
        match self.knot_undo.pop() {
            Some(prev) => {
                self.knot_redo
                    .push(std::mem::replace(&mut self.knot_source, prev));
                self.knot_coalescing = false;
                true
            }
            None => false,
        }
    }

    /// Redo the last undone edit: restore the top redo snapshot, moving the current
    /// source back onto the undo stack. Returns whether anything was redone.
    pub fn knot_redo_apply(&mut self) -> bool {
        match self.knot_redo.pop() {
            Some(next) => {
                self.knot_undo
                    .push(std::mem::replace(&mut self.knot_source, next));
                self.knot_coalescing = false;
                true
            }
            None => false,
        }
    }

    /// Toggle the knot editor: open a fresh note, or close it.
    pub fn toggle_knot_editor(&mut self) {
        if self.knot_editor_open {
            self.close_knot_editor();
        } else {
            self.open_knot_editor("# New note\n\nStart typing.\n");
        }
    }

    /// Queue a save of the current knot source for the host to apply to the graph.
    pub fn request_knot_editor_save(&mut self) {
        if self.knot_editor_open {
            self.knot_save_requested = true;
        }
    }

    /// Update the editor's window-space tile rect, or clear to the fixed fallback.
    pub fn set_knot_editor_rect(&mut self, rect: Option<[f32; 4]>) {
        self.knot_editor_rect = rect;
    }

    /// Take the pending knot-editor save request, if it is bound to a graph member.
    pub fn take_knot_editor_save(&mut self) -> Option<(GraphMemberId, String)> {
        if !self.knot_save_requested {
            return None;
        }
        self.knot_save_requested = false;
        self.knot_target
            .map(|target| (target, self.knot_source.text().to_string()))
    }

    /// Open conversation `id`: select it (clearing the prior thread) and record an
    /// `Open` so the host loads its messages from the live `Comms`.
    pub fn select_conversation(&mut self, id: ConversationId) {
        self.comms.select(id.clone());
        self.comms_draft = TextInput::new("");
        self.comms_intent = Some(CommsIntent::Open(id));
    }

    /// Record a send of the composed reply: sync the editing buffer into the draft
    /// and, if it is ready, hand it to the host (which routes it to `Comms::send`
    /// and reloads the thread). A no-op for an empty draft or no selection.
    pub fn send_comms(&mut self) {
        self.comms.set_draft_body(self.comms_draft.text().trim());
        if self.comms.can_send() {
            self.comms_intent = Some(CommsIntent::Send(self.comms.draft.clone()));
        }
    }

    /// Open the compose-new form (drops any open conversation), with fresh
    /// recipient + body buffers.
    pub fn open_new_message(&mut self) {
        self.comms.open_new_message();
        self.comms_new_to = TextInput::new("");
        self.comms_new_body = TextInput::new("");
    }

    /// Close the compose-new form without sending.
    pub fn close_new_message(&mut self) {
        self.comms.close_new_message();
    }

    /// "Share cabal invite": open a new misfin message pre-filled with the cabal
    /// join ticket as its body, so the user just adds a recipient and sends. A
    /// no-op until the cabal (and its ticket) is up.
    pub fn share_cabal_invite(&mut self) {
        let Some(ticket) = self.comms.cabal_ticket.clone() else {
            return;
        };
        self.open_new_message();
        self.comms_new_body = TextInput::new(ticket);
    }

    /// Set the compose-new form's protocol (the Misfin / Cable toggle).
    pub fn set_new_message_protocol(&mut self, protocol: ProtocolKind) {
        self.comms.set_new_protocol(protocol);
    }

    /// Send the compose-new form: build a [`Draft`] from the chosen protocol +
    /// recipient + body and hand it to the host. Misfin targets the typed
    /// `mailbox@host`; Cable targets the (first) murm cabal in the inbox. A no-op
    /// for an empty body, an empty misfin address, or no cable to target.
    pub fn send_new_message(&mut self) {
        let Some(form) = self.comms.new_message.as_ref() else {
            return;
        };
        let protocol = form.protocol;
        let to = self.comms_new_to.text().trim().to_string();
        let body = self.comms_new_body.text().trim().to_string();
        if body.is_empty() {
            return;
        }
        let conversation = match protocol {
            ProtocolKind::Misfin if !to.is_empty() => {
                Some(ConversationId::new(ProtocolKind::Misfin, to))
            }
            ProtocolKind::Misfin => return,
            ProtocolKind::Murm => self
                .comms
                .inbox
                .iter()
                .find(|c| c.id.protocol == ProtocolKind::Murm)
                .map(|c| c.id.clone()),
        };
        let Some(conversation) = conversation else {
            return;
        };
        self.comms_intent = Some(CommsIntent::Send(Draft {
            conversation: Some(conversation),
            body,
            subject: None,
        }));
        // Keep the form open so its send-status line shows the outcome, and a
        // failed send keeps the typed address + body to fix. The user closes it
        // with Cancel.
    }

    /// Record a request to connect the cabal from a received join `ticket` (a
    /// "Join this cabal" on an invite message). The host routes it to the actor.
    pub fn connect_cabal(&mut self, ticket: String) {
        self.comms_intent = Some(CommsIntent::ConnectCabal(ticket));
    }

    /// Take the pending comms request, if any. The host drains it after input and
    /// issues the matching command to the comms actor.
    pub fn take_comms_intent(&mut self) -> Option<CommsIntent> {
        self.comms_intent.take()
    }

    /// Clear the compose buffer + draft after a successful send (the host calls
    /// this when the actor reports `Sent`).
    pub fn clear_comms_draft(&mut self) {
        self.comms.clear_draft();
        self.comms_draft = TextInput::new("");
    }

    /// The text field that currently owns editing / the caret: the comms compose
    /// buffer when the pane has focus, the palette query when the palette is open,
    /// otherwise the omnibar. The host reads this to paint the caret on the right
    /// field.
    pub fn active_field(&self) -> &TextInput {
        if self.comms.is_open() && self.comms.dock.focused {
            &self.comms_draft
        } else if self.palette_open {
            &self.palette_input
        } else {
            &self.omnibar
        }
    }
}
