/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! In-editor completion popups for the knot source: the `/` slash menu (block templates)
//! and (later) `[[` node-link completion. The state + detection + accept live here; the
//! host (`WindowCtx`) drives the refresh (candidate items + caret anchor) and the keyboard,
//! and the chrome view renders it through `cambium::menu`. (Djot editor — Phase 3.)

use super::*;

/// Which trigger opened the completion popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotCompletionKind {
    /// `/` at the start of a line — a menu of block templates.
    Slash,
    /// `[[` — a menu of graph nodes to link.
    Wikilink,
}

/// One completion candidate: the row `label`, and the `insert` text that replaces the
/// trigger + query when it is accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotCompletionItem {
    pub label: String,
    pub insert: String,
}

/// The open completion popup over the knot source.
#[derive(Clone, Debug)]
pub struct KnotCompletion {
    pub kind: KnotCompletionKind,
    /// Byte offset in the source where the trigger begins (the `/` or the first `[` of `[[`);
    /// accepting replaces `trigger_byte..caret`.
    pub trigger_byte: usize,
    /// The text typed after the trigger (filters the candidates).
    pub query: String,
    /// The filtered candidates.
    pub items: Vec<KnotCompletionItem>,
    /// The highlighted row.
    pub selected: usize,
    /// Window-space anchor (just below the caret) for the overlay.
    pub anchor: (f32, f32),
}

/// The byte offset of char index `ci` in `text` (buffer end when past the last char).
fn byte_of_char(text: &str, ci: usize) -> usize {
    text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(text.len())
}

/// The full block-template list for the `/` slash menu. Filtered by the typed query.
fn slash_templates() -> Vec<KnotCompletionItem> {
    let t = |label: &str, insert: &str| KnotCompletionItem {
        label: label.to_string(),
        insert: insert.to_string(),
    };
    vec![
        t("Heading 1", "# "),
        t("Heading 2", "## "),
        t("Heading 3", "### "),
        t("Bullet list", "- "),
        t("Numbered list", "1. "),
        t("Task", "- [ ] "),
        t("Quote", "> "),
        t("Code block", "```\n```"),
        t("Divider", "---\n"),
    ]
}

/// Detect a completion context at `caret_char` in `text`: the trigger kind, the byte offset
/// where the trigger begins, and the query typed after it. `None` when the caret is not in a
/// live trigger (no trigger, a space broke a slash, a `]`/newline broke a wikilink).
pub fn detect_completion(text: &str, caret_char: usize) -> Option<(KnotCompletionKind, usize, String)> {
    let caret_byte = byte_of_char(text, caret_char);
    let before = &text[..caret_byte];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = &before[line_start..];

    // Wikilink: the nearest `[[` on this line not yet closed, with no `]` in the query.
    if let Some(rel) = line.rfind("[[") {
        let query = &line[rel + 2..];
        if !query.contains(']') {
            return Some((
                KnotCompletionKind::Wikilink,
                line_start + rel,
                query.to_string(),
            ));
        }
    }

    // Slash: a `/` as the first non-whitespace on the line, with no space after it yet.
    let indent: usize = line.chars().take_while(|c| c.is_whitespace()).map(char::len_utf8).sum();
    let rest = &line[indent..];
    if let Some(query) = rest.strip_prefix('/') {
        if !query.contains(char::is_whitespace) {
            return Some((
                KnotCompletionKind::Slash,
                line_start + indent,
                query.to_string(),
            ));
        }
    }

    None
}

/// Filter `templates` by a case-insensitive substring match on `query` (empty query = all).
pub fn slash_items(query: &str) -> Vec<KnotCompletionItem> {
    let q = query.to_lowercase();
    slash_templates()
        .into_iter()
        .filter(|t| q.is_empty() || t.label.to_lowercase().contains(&q))
        .collect()
}

impl Chrome {
    /// Move the completion highlight by `delta` (wrapping), if the popup is open.
    pub fn move_knot_completion(&mut self, delta: isize) {
        if let Some(comp) = &mut self.knot_completion {
            let n = comp.items.len();
            if n > 0 {
                let cur = comp.selected as isize;
                comp.selected = (cur + delta).rem_euclid(n as isize) as usize;
            }
        }
    }

    /// Close the completion popup.
    pub fn close_knot_completion(&mut self) {
        self.knot_completion = None;
    }

    /// Accept completion candidate `index`: replace the trigger + query in the source with the
    /// item's `insert` text, then close the popup. A no-op for a stale index. Snapshots for undo.
    pub fn accept_knot_completion(&mut self, index: usize) {
        let Some(comp) = self.knot_completion.take() else {
            return;
        };
        let Some(item) = comp.items.get(index).cloned() else {
            return;
        };
        let caret_byte = byte_of_char(self.knot_source.text(), self.knot_source.caret());
        if comp.trigger_byte > caret_byte {
            return;
        }
        self.knot_edit_snapshot(false);
        // Select trigger..caret, then replace with the insert text.
        self.knot_source.set_caret_byte(comp.trigger_byte, false);
        self.knot_source.set_caret_byte(caret_byte, true);
        self.knot_source.insert_str(&item.insert);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_slash_at_line_start() {
        let t = "a\n/head";
        let (kind, trigger, query) = detect_completion(t, t.chars().count()).unwrap();
        assert_eq!(kind, KnotCompletionKind::Slash);
        assert_eq!(&t[trigger..], "/head");
        assert_eq!(query, "head");
    }

    #[test]
    fn slash_broken_by_a_space_is_not_a_trigger() {
        assert!(detect_completion("/head ", "/head ".chars().count()).is_none());
    }

    #[test]
    fn detects_a_wikilink() {
        let t = "see [[No";
        let (kind, trigger, query) = detect_completion(t, t.chars().count()).unwrap();
        assert_eq!(kind, KnotCompletionKind::Wikilink);
        assert_eq!(&t[trigger..], "[[No");
        assert_eq!(query, "No");
    }

    #[test]
    fn slash_items_filter_by_query() {
        assert!(slash_items("").len() >= 5);
        let heads = slash_items("head");
        assert_eq!(heads.len(), 3);
        assert!(heads.iter().all(|i| i.label.contains("Heading")));
    }

    #[test]
    fn accept_replaces_the_trigger_with_the_insert() {
        let mut c = Chrome::new("mere://test");
        c.open_knot_editor("- one\n/head"); // caret at end (byte 11)
        c.knot_completion = Some(KnotCompletion {
            kind: KnotCompletionKind::Slash,
            trigger_byte: 6, // the "/" after "- one\n"
            query: "head".to_string(),
            items: slash_items("head"),
            selected: 0, // "Heading 1" -> "# "
            anchor: (0.0, 0.0),
        });
        c.accept_knot_completion(0);
        assert_eq!(c.knot_source.text(), "- one\n# ");
        assert!(c.knot_completion.is_none());
    }
}
