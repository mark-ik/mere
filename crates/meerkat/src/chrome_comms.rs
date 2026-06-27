/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Chrome: comms pane, knot editor, new-message + cabal connect.

use super::*;

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
    }

    /// Close the knot editor.
    pub fn close_knot_editor(&mut self) {
        self.knot_editor_open = false;
    }

    /// Toggle the knot editor: open a fresh note, or close it.
    pub fn toggle_knot_editor(&mut self) {
        if self.knot_editor_open {
            self.close_knot_editor();
        } else {
            self.open_knot_editor("# New note\n\nStart typing.\n");
        }
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
