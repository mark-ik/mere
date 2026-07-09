/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Chrome: comms pane, knot editor, new-message + cabal connect.

use super::*;

/// The byte offset of char index `ci` in `text` (the buffer end when past the last char),
/// bridging `TextInput`'s char-index caret to its byte-offset selection setters.
fn byte_of_char(text: &str, ci: usize) -> usize {
    text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(text.len())
}

/// The list marker `after` (a line past its indent) begins with, if any: an unordered /
/// task bullet, or an ordered `N.` / `N)`. The returned string includes the trailing space.
fn list_marker(after: &str) -> Option<String> {
    for m in ["- [ ] ", "- [x] ", "- ", "* ", "+ "] {
        if after.starts_with(m) {
            return Some(m.to_string());
        }
    }
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        let rest = &after[digits.len()..];
        if rest.starts_with(". ") {
            return Some(format!("{digits}. "));
        }
        if rest.starts_with(") ") {
            return Some(format!("{digits}) "));
        }
    }
    None
}

/// The marker that continues `marker` on the next line: an ordered number increments; a
/// task item resets to unchecked; an unordered bullet repeats.
fn next_list_marker(marker: &str) -> String {
    let digits: String = marker.chars().take_while(|c| c.is_ascii_digit()).collect();
    if let Ok(n) = digits.parse::<usize>() {
        return format!("{}{}", n + 1, &marker[digits.len()..]);
    }
    match marker {
        "- [ ] " | "- [x] " => "- [ ] ".to_string(),
        other => other.to_string(),
    }
}

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
        self.knot_close_after_save = false;
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
        self.knot_close_after_save = false;
        self.knot_editor_preview = false;
        self.reset_knot_history();
    }

    /// Close the knot editor.
    pub fn close_knot_editor(&mut self) {
        self.knot_editor_open = false;
        self.knot_target = None;
        self.knot_editor_rect = None;
        self.knot_save_requested = false;
        self.knot_close_after_save = false;
        self.knot_editor_preview = false;
        self.reset_knot_history();
    }

    /// Request a close that autosaves first. For a bound note, queue a save and defer the
    /// actual close to the host (which drains the save while the target is still bound, then
    /// closes) — so closing never drops unsaved edits. An unbound scratch note (no tile to
    /// write) closes immediately. This is what the × button and the editor toggle call.
    /// (Djot editor — Phase 2 autosave-on-close.)
    pub fn request_knot_editor_close(&mut self) {
        if self.knot_editor_open && self.knot_target.is_some() {
            self.knot_save_requested = true;
            self.knot_close_after_save = true;
        } else {
            self.close_knot_editor();
        }
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
    /// undoes into the previous one), plus the structural expand chain.
    fn reset_knot_history(&mut self) {
        self.knot_history.clear();
        self.knot_expand_stack.clear();
        self.knot_completion = None;
    }

    /// Grow the selection to the smallest enclosing djot container (Alt-Up), pushing the
    /// current range so [`shrink_selection`](Self::shrink_selection) can step back. A no-op
    /// (returns `false`) when nothing encloses the current selection any further. (Djot
    /// editor — Phase 3 structural selection.)
    pub fn grow_selection(&mut self) -> bool {
        let text = self.knot_source.text().to_string();
        let (clo, chi) = self.knot_source.selection();
        let blo = byte_of_char(&text, clo);
        let bhi = byte_of_char(&text, chi);
        let Some(range) = illume::expand_selection(&text, blo..bhi) else {
            return false;
        };
        if range.start == blo && range.end == bhi {
            return false;
        }
        self.knot_expand_stack.push((blo, bhi));
        self.knot_source.set_caret_byte(range.start, false);
        self.knot_source.set_caret_byte(range.end, true);
        true
    }

    /// Shrink the selection back to the range the last [`grow_selection`](Self::grow_selection)
    /// expanded from (Alt-Down). A no-op when the expand chain is empty.
    pub fn shrink_selection(&mut self) -> bool {
        let Some((blo, bhi)) = self.knot_expand_stack.pop() else {
            return false;
        };
        self.knot_source.set_caret_byte(blo, false);
        self.knot_source.set_caret_byte(bhi, true);
        true
    }

    /// Record the pre-edit source on the undo stack, before a mutating edit applies.
    /// `coalesce_insert` is true for a character/space insert: a run of them coalesces
    /// into one undo entry (so a burst of typing undoes as a unit), by skipping the push
    /// while already coalescing. A non-insert edit (delete, newline) passes `false`, so it
    /// is its own undo step and ends any insert run. Any push clears the redo stack and
    /// caps the history. (Djot editor — Phase 2 undo/redo.)
    pub fn knot_edit_snapshot(&mut self, coalesce_insert: bool) {
        self.knot_history.snapshot(&self.knot_source, coalesce_insert);
        // An edit invalidates the structural expand chain (the ranges no longer align).
        self.knot_expand_stack.clear();
    }

    /// End the current insert-coalescing run without snapshotting — for a caret move, so
    /// the next insert starts a fresh undo group even though nothing was deleted. A caret
    /// move also breaks the structural expand chain.
    pub fn knot_break_coalesce(&mut self) {
        self.knot_history.break_coalesce();
        self.knot_expand_stack.clear();
    }

    /// Undo the last edit: restore the top undo snapshot into the source. Returns whether
    /// anything was undone.
    pub fn knot_undo_apply(&mut self) -> bool {
        self.knot_history.undo(&mut self.knot_source)
    }

    /// Redo the last undone edit: restore the top redo snapshot into the source. Returns
    /// whether anything was redone.
    pub fn knot_redo_apply(&mut self) -> bool {
        self.knot_history.redo(&mut self.knot_source)
    }

    /// Smart list continuation on Enter: in a list item, insert a newline that keeps the
    /// same indent and marker (ordered markers increment, task items reset to unchecked);
    /// on an *empty* item (just the marker) end the list instead by clearing that marker.
    /// Returns whether it handled the Enter — `false` means the caller inserts a plain
    /// newline. (Djot editor — Phase 3 smart list continuation.)
    pub fn continue_list_on_enter(&mut self) -> bool {
        let text = self.knot_source.text().to_string();
        let caret_byte = byte_of_char(&text, self.knot_source.caret());
        let line_start = text[..caret_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line = &text[line_start..caret_byte];
        let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let after = &line[indent.len()..];
        let Some(marker) = list_marker(after) else {
            return false;
        };
        if after[marker.len()..].trim().is_empty() {
            // Empty item: exit the list by clearing this line's indent + marker.
            self.knot_source.set_caret_byte(line_start, false);
            self.knot_source.set_caret_byte(caret_byte, true);
            self.knot_source.insert_str("");
            return true;
        }
        self.knot_source
            .insert_str(&format!("\n{indent}{}", next_list_marker(&marker)));
        true
    }

    /// Auto-pair a wrapping delimiter over a selection: with text selected, typing `(`, `[`,
    /// `{`, `*`, `_`, `` ` ``, `~`, `"` or `'` wraps it (open + selection + close) and keeps
    /// the inner text selected, so wraps nest (e.g. `*`-then-`*` gives `**bold**`). Returns
    /// whether it wrapped; `false` (no selection, or not a pair char) falls through to a
    /// normal insert. (Djot editor — Phase 3 auto-pairs.)
    pub fn wrap_selection_if_pair(&mut self, open: char) -> bool {
        if self.knot_source.has_selection() && xilem_serval::pair_close(open).is_some() {
            self.knot_edit_snapshot(false);
            xilem_serval::wrap_selection(&mut self.knot_source, open)
        } else {
            false
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

#[cfg(test)]
mod tests {
    use super::*;

    fn editor_with(source: &str, caret_byte: usize) -> Chrome {
        let mut c = Chrome::new("mere://test");
        c.open_knot_editor(source);
        c.knot_source.set_caret_byte(caret_byte, false);
        c
    }

    #[test]
    fn enter_continues_an_unordered_list() {
        let mut c = editor_with("- one", 5);
        assert!(c.continue_list_on_enter());
        assert_eq!(c.knot_source.text(), "- one\n- ");
    }

    #[test]
    fn enter_increments_an_ordered_list() {
        let mut c = editor_with("1. first", 8);
        assert!(c.continue_list_on_enter());
        assert_eq!(c.knot_source.text(), "1. first\n2. ");
    }

    #[test]
    fn enter_on_an_empty_item_ends_the_list() {
        let mut c = editor_with("- one\n- ", 8);
        assert!(c.continue_list_on_enter());
        assert_eq!(c.knot_source.text(), "- one\n");
    }

    #[test]
    fn enter_outside_a_list_is_not_handled() {
        let mut c = editor_with("plain text", 10);
        assert!(!c.continue_list_on_enter());
        assert_eq!(c.knot_source.text(), "plain text");
    }

    #[test]
    fn wrap_selection_wraps_and_nests() {
        let mut c = editor_with("hello world", 0);
        c.knot_source.set_caret_byte(0, false);
        c.knot_source.set_caret_byte(5, true); // select "hello"
        assert!(c.wrap_selection_if_pair('*'));
        assert_eq!(c.knot_source.text(), "*hello* world");
        // The inner text stays selected, so a repeat wrap nests.
        assert!(c.wrap_selection_if_pair('_'));
        assert_eq!(c.knot_source.text(), "*_hello_* world");
    }

    #[test]
    fn wrap_without_selection_is_declined() {
        let mut c = editor_with("hello", 5);
        assert!(!c.wrap_selection_if_pair('*'));
        assert_eq!(c.knot_source.text(), "hello");
    }

    #[test]
    fn grow_then_shrink_returns_to_the_start() {
        // A collapsed caret inside a paragraph grows to enclose more text; shrink steps back
        // to exactly the original range. (Structural selection over illume's container tree.)
        let mut c = editor_with("# Title\n\nhello world\n", 15); // caret inside "world"
        let before = c.knot_source.selection();
        assert!(c.grow_selection(), "grow expands from the caret");
        let grown = c.knot_source.selection();
        assert_ne!(grown, before, "the selection expanded");
        assert!(grown.1 - grown.0 > before.1 - before.0, "the range got wider");
        assert!(c.shrink_selection(), "shrink steps back");
        assert_eq!(c.knot_source.selection(), before, "back to the original range");
        assert!(!c.shrink_selection(), "nothing left to shrink");
    }
}
