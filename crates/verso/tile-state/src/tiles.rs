/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Workbench tile state.
//!
//! Per Mark's framing: **every tile is a facet of a graph node**.
//! The node is the *anchor* — what the user explicitly opened. The
//! tile carries that anchor plus the user's **within-tile
//! navigation history** (URLs visited under this anchor without
//! creating new graph nodes).
//!
//! Per the node-per-tile + lineage facet design committed in the
//! [tear-out operations brief](../../../../../design_docs/mere_docs/research/2026-05-11_tearout_operations_brief.md)
//! §6.1: opening a new tile mints a node in `kernel::graph`;
//! navigating within that tile extends its history without minting
//! new nodes. Within-tile back/forward operates on this history;
//! cross-tile navigation is the tile-strip switcher.
//!
//! History is **short-term memory** per the
//! [memory tiers brief](../../../../../design_docs/mere_docs/research/2026-05-11_memory_tiers_brief.md)
//! — engram consolidation is an explicit, separate gesture.

use std::collections::HashMap;
use std::time::SystemTime;

use inker::{EngineDocument, SurfaceProducer};
use kernel::graph::NodeKey;

use crate::surface_tile::SurfaceTileState;

/// One entry in a tile's within-tile navigation history. Records
/// the URL visited and the timestamp it was loaded. The document
/// itself is cached in the tile's per-URL document map, not on the
/// entry.
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub url: String,
    pub loaded_at: SystemTime,
}

impl HistoryEntry {
    pub fn new(url: String) -> Self {
        Self {
            url,
            loaded_at: SystemTime::now(),
        }
    }
}

/// Per-tile mutable state. Each open tile has one of these, keyed
/// in the manager by the anchor `NodeKey`.
#[derive(Default)]
pub struct TileState {
    /// Within-tile navigation history. Index 0 = oldest; pushed when
    /// the user navigates within this tile. Reload re-fetches and
    /// replaces the document at the cursor.
    pub history: Vec<HistoryEntry>,
    /// Where the user is right now in the history list. Always in
    /// range when `history` is non-empty.
    pub history_cursor: usize,
    /// Document cache, keyed by URL. Pre-fetched documents are
    /// kept so back/forward within the tile is instant.
    pub documents: HashMap<String, EngineDocument>,
}

impl TileState {
    /// Construct a tile state seeded with the anchor's first
    /// document. The tile starts with cursor 0 pointing at that
    /// single history entry.
    pub fn seeded(url: String, document: EngineDocument) -> Self {
        let mut documents = HashMap::new();
        documents.insert(url.clone(), document);
        Self {
            history: vec![HistoryEntry::new(url)],
            history_cursor: 0,
            documents,
        }
    }

    /// Currently-pointed-at history entry (the URL the tile is
    /// displaying right now). `None` for tiles with empty history
    /// (shouldn't happen post-seed but defensively safe).
    pub fn current(&self) -> Option<&HistoryEntry> {
        self.history.get(self.history_cursor)
    }

    /// Document at the current history cursor, if cached. Cache
    /// lookups go through here so callers don't reach into
    /// `documents` directly.
    pub fn current_document(&self) -> Option<&EngineDocument> {
        let entry = self.current()?;
        self.documents.get(&entry.url)
    }

    /// Whether navigating back is possible from the current cursor.
    pub fn can_go_back(&self) -> bool {
        self.history_cursor > 0
    }

    /// Whether navigating forward is possible from the current
    /// cursor.
    pub fn can_go_forward(&self) -> bool {
        self.history_cursor + 1 < self.history.len()
    }

    /// Extend the history with a fresh navigation. If the cursor
    /// isn't at the end (the user went back then navigated
    /// somewhere new), the forward branch is truncated — same as a
    /// browser's "if you go back and then somewhere new, the
    /// original forward path is lost" semantics.
    ///
    /// If the URL already matches the current entry, this is a
    /// no-op against `history`, but the document is refreshed (used
    /// for in-place reload).
    pub fn push(&mut self, url: String, document: EngineDocument) {
        if self.current().map(|e| &e.url) == Some(&url) {
            // Same URL — treat as a refresh: replace document, keep
            // cursor + history shape unchanged.
            self.documents.insert(url, document);
            return;
        }
        if self.history_cursor + 1 < self.history.len() {
            self.history.truncate(self.history_cursor + 1);
        }
        self.history.push(HistoryEntry::new(url.clone()));
        self.history_cursor = self.history.len() - 1;
        self.documents.insert(url, document);
    }

    /// Move the cursor back one entry. Returns the new current
    /// entry, or `None` if already at the oldest.
    pub fn go_back(&mut self) -> Option<&HistoryEntry> {
        if !self.can_go_back() {
            return None;
        }
        self.history_cursor -= 1;
        self.current()
    }

    /// Move the cursor forward one entry. Returns the new current
    /// entry, or `None` if already at the newest.
    pub fn go_forward(&mut self) -> Option<&HistoryEntry> {
        if !self.can_go_forward() {
            return None;
        }
        self.history_cursor += 1;
        self.current()
    }

    /// Replace the document for the current history entry —
    /// supports reload semantics where the URL is unchanged but the
    /// content has been re-fetched.
    pub fn refresh_current(&mut self, document: EngineDocument) {
        if let Some(entry) = self.history.get(self.history_cursor) {
            let url = entry.url.clone();
            self.documents.insert(url, document);
        }
    }
}

/// Navigation mode — caller picks whether a submit should extend
/// the active tile's history (default) or mint a new tile (and, in
/// turn, a new anchor node in the graph).
///
/// Per the node-per-tile design committed 2026-05-11: the omnibar's
/// default Enter is [`Self::WithinTile`]; `Ctrl/Cmd-Enter` triggers
/// [`Self::NewTile`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigateMode {
    /// Extend the active tile's history. No new kernel node. Falls
    /// through to `NewTile` if no tile is currently active.
    WithinTile,
    /// Mint a new anchor node and open it as a new tile.
    NewTile,
}

impl Default for NavigateMode {
    fn default() -> Self {
        Self::WithinTile
    }
}

/// Workbench tile state. Tiles are an ordered list (matches the
/// strip the user sees); `active` indexes into it.
///
/// Each open tile is identified by its anchor `NodeKey`; per-tile
/// mutable state ([`TileState`]) lives in `state`, keyed by the
/// same anchor.
#[derive(Default)]
pub struct TileManager {
    open: Vec<NodeKey>,
    active: Option<usize>,
    state: HashMap<NodeKey, TileState>,
    /// Parallel state map for surface-engine tiles (e.g. `scrying.web`).
    /// A given anchor is always one flavor or the other — the routing
    /// decision picks which at open time (slice 5 of the scrying plan).
    /// Slice 3 just makes both flavors addressable from the same manager.
    surface: HashMap<NodeKey, SurfaceTileState>,
}

impl TileManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ordered anchors of the open tiles.
    pub fn open_tiles(&self) -> &[NodeKey] {
        &self.open
    }

    /// Index of the active tile in `open_tiles()`.
    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    /// Anchor `NodeKey` of the active tile.
    pub fn active_node(&self) -> Option<NodeKey> {
        self.active.and_then(|i| self.open.get(i).copied())
    }

    /// Document at the active tile's history cursor.
    pub fn active_document(&self) -> Option<&EngineDocument> {
        let node = self.active_node()?;
        self.state.get(&node)?.current_document()
    }

    /// URL the active tile is currently displaying. Use this to
    /// sync the omnibar / toolbar — distinct from the anchor's URL
    /// once within-tile navigation has happened.
    pub fn active_url(&self) -> Option<&str> {
        let node = self.active_node()?;
        self.state.get(&node)?.current().map(|e| e.url.as_str())
    }

    /// Whether the active tile can navigate back in its own
    /// history.
    pub fn active_can_go_back(&self) -> bool {
        self.active_node()
            .and_then(|n| self.state.get(&n))
            .map(|s| s.can_go_back())
            .unwrap_or(false)
    }

    /// Whether the active tile can navigate forward in its own
    /// history.
    pub fn active_can_go_forward(&self) -> bool {
        self.active_node()
            .and_then(|n| self.state.get(&n))
            .map(|s| s.can_go_forward())
            .unwrap_or(false)
    }

    /// Document the tile anchored at `node` is currently showing.
    /// Used by tile-strip rendering to label rows by their current
    /// document title.
    pub fn document_for(&self, node: NodeKey) -> Option<&EngineDocument> {
        self.state.get(&node)?.current_document()
    }

    /// Borrow the active tile's state (history + documents).
    pub fn active_state(&self) -> Option<&TileState> {
        let node = self.active_node()?;
        self.state.get(&node)
    }

    /// Mutably borrow the active tile's state.
    pub fn active_state_mut(&mut self) -> Option<&mut TileState> {
        let node = self.active.and_then(|i| self.open.get(i).copied())?;
        self.state.get_mut(&node)
    }

    /// Open a new tile anchored at `node` for `url`/`document`, or
    /// focus the existing tile if one already exists for that
    /// anchor. Returns `true` when a fresh tile was added; `false`
    /// when an existing one was re-focused (the URL is treated as
    /// a history push if it differs from the current entry, or a
    /// refresh if it matches).
    pub fn open_or_focus(&mut self, node: NodeKey, url: String, document: EngineDocument) -> bool {
        if let Some(i) = self.open.iter().position(|n| *n == node) {
            self.active = Some(i);
            if let Some(state) = self.state.get_mut(&node) {
                state.push(url, document);
            }
            false
        } else {
            self.open.push(node);
            self.state.insert(node, TileState::seeded(url, document));
            self.active = Some(self.open.len() - 1);
            true
        }
    }

    /// Push a within-tile navigation onto the active tile's
    /// history. Used by `NavigateMode::WithinTile`. No-op if no
    /// tile is active.
    pub fn push_history(&mut self, url: String, document: EngineDocument) {
        if let Some(state) = self.active_state_mut() {
            state.push(url, document);
        }
    }

    /// Replace the document at the active tile's current cursor —
    /// reload semantics. Returns `true` if a refresh happened.
    pub fn refresh_active(&mut self, document: EngineDocument) -> bool {
        if let Some(state) = self.active_state_mut() {
            state.refresh_current(document);
            true
        } else {
            false
        }
    }

    /// Move the active tile's cursor back one entry. Returns the
    /// new current URL (cloned) when navigation happened.
    pub fn active_go_back(&mut self) -> Option<String> {
        let state = self.active_state_mut()?;
        state.go_back().map(|e| e.url.clone())
    }

    /// Move the active tile's cursor forward one entry. Returns
    /// the new current URL (cloned) when navigation happened.
    pub fn active_go_forward(&mut self) -> Option<String> {
        let state = self.active_state_mut()?;
        state.go_forward().map(|e| e.url.clone())
    }

    /// Set the active tile to `index` if it is in range. Returns
    /// the resulting active `NodeKey` if any.
    pub fn focus_index(&mut self, index: usize) -> Option<NodeKey> {
        if index < self.open.len() {
            self.active = Some(index);
        }
        self.active_node()
    }

    // ── Surface-tile API ────────────────────────────────────────────────

    /// Open a new surface tile anchored at `node`, or focus the existing
    /// one if `node` already has a surface tile. The `producer` is a live
    /// [`SurfaceProducer`] (already spawned by the engine); `engine_id`
    /// identifies which engine produced it (e.g. `"scrying.web"`).
    ///
    /// Returns `true` when a fresh tile was added; `false` when the
    /// existing surface tile was re-focused (in which case `producer` is
    /// dropped — the existing producer is kept).
    pub fn open_or_focus_surface(
        &mut self,
        node: NodeKey,
        anchor_url: String,
        producer: Box<dyn SurfaceProducer>,
        engine_id: impl Into<String>,
    ) -> bool {
        if let Some(i) = self.open.iter().position(|n| *n == node) {
            self.active = Some(i);
            // Existing surface tile keeps its producer; the freshly-passed
            // one drops here. Existing document tile would coexist —
            // routing should not normally call this for a doc-tile
            // anchor, but if it does, we leave the doc state alone and
            // start tracking surface state too. Same-anchor flavor flip
            // is policy elsewhere.
            let _ = producer;
            false
        } else {
            self.open.push(node);
            self.surface.insert(
                node,
                SurfaceTileState::spawned(engine_id, producer, anchor_url),
            );
            self.active = Some(self.open.len() - 1);
            true
        }
    }

    /// Borrow the surface state for `node`, if it has one.
    pub fn surface_state(&self, node: NodeKey) -> Option<&SurfaceTileState> {
        self.surface.get(&node)
    }

    /// Mutably borrow the surface state for `node`, if it has one.
    pub fn surface_state_mut(&mut self, node: NodeKey) -> Option<&mut SurfaceTileState> {
        self.surface.get_mut(&node)
    }

    /// Borrow the active tile's surface state, if any.
    pub fn active_surface_state(&self) -> Option<&SurfaceTileState> {
        let node = self.active_node()?;
        self.surface.get(&node)
    }

    /// Mutably borrow the active tile's surface state, if any.
    pub fn active_surface_state_mut(&mut self) -> Option<&mut SurfaceTileState> {
        let node = self.active.and_then(|i| self.open.get(i).copied())?;
        self.surface.get_mut(&node)
    }

    /// Whether `node` is anchored by a surface-engine tile. False both for
    /// document-engine tiles and for unknown anchors.
    pub fn is_surface_tile(&self, node: NodeKey) -> bool {
        self.surface.contains_key(&node)
    }

    /// Iterator over all open surface-tile anchors. Useful for the host's
    /// per-tick `step()` driver, which needs to poll every active producer.
    pub fn surface_tile_anchors(&self) -> impl Iterator<Item = NodeKey> + '_ {
        self.surface.keys().copied()
    }

    /// Close the tile at `index`. The tile's per-anchor state (and
    /// its document cache, or surface producer) is dropped — the
    /// underlying graph node stays alive for other tiles / the orrery.
    /// Returns the new active `NodeKey` if any.
    pub fn close_index(&mut self, index: usize) -> Option<NodeKey> {
        if index >= self.open.len() {
            return self.active_node();
        }
        let removed = self.open.remove(index);
        self.state.remove(&removed);
        // Dropping the surface state drops the producer, which tears
        // down its underlying WebView control.
        self.surface.remove(&removed);
        if self.open.is_empty() {
            self.active = None;
        } else if let Some(active) = self.active {
            self.active = Some(if index < active {
                active - 1
            } else if index == active {
                active.saturating_sub(1)
            } else {
                active
            });
        }
        self.active_node()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker::{DocumentProvenance, DocumentTrustState};

    fn fixture_doc(addr: &str) -> EngineDocument {
        EngineDocument {
            address: addr.to_string(),
            title: Some(format!("Title for {addr}")),
            content_type: "text/plain".to_string(),
            lang: None,
            provenance: DocumentProvenance::for_engine("test", addr),
            trust: DocumentTrustState::Unknown,
            diagnostics: Vec::new(),
            blocks: Vec::new(),
        }
    }

    fn fixture_node(seed: u32) -> NodeKey {
        NodeKey::new(seed as usize)
    }

    #[test]
    fn open_or_focus_creates_seeded_tile() {
        let mut tm = TileManager::new();
        let node = fixture_node(1);
        assert!(tm.open_or_focus(node, "mere://a".to_string(), fixture_doc("mere://a")));
        assert_eq!(tm.open_tiles(), &[node]);
        assert_eq!(tm.active_index(), Some(0));
        assert_eq!(tm.active_url(), Some("mere://a"));
        assert!(!tm.active_can_go_back());
        assert!(!tm.active_can_go_forward());
    }

    #[test]
    fn within_tile_history_grows_on_push() {
        let mut tm = TileManager::new();
        let node = fixture_node(1);
        tm.open_or_focus(node, "mere://a".to_string(), fixture_doc("mere://a"));
        tm.push_history("mere://b".to_string(), fixture_doc("mere://b"));
        tm.push_history("mere://c".to_string(), fixture_doc("mere://c"));
        let state = tm.active_state().unwrap();
        assert_eq!(state.history.len(), 3);
        assert_eq!(state.history_cursor, 2);
        assert_eq!(tm.active_url(), Some("mere://c"));
    }

    #[test]
    fn back_forward_walk_history_without_refetching() {
        let mut tm = TileManager::new();
        let node = fixture_node(1);
        tm.open_or_focus(node, "a".to_string(), fixture_doc("a"));
        tm.push_history("b".to_string(), fixture_doc("b"));
        tm.push_history("c".to_string(), fixture_doc("c"));
        assert_eq!(tm.active_go_back(), Some("b".to_string()));
        assert_eq!(tm.active_url(), Some("b"));
        assert_eq!(tm.active_go_back(), Some("a".to_string()));
        assert_eq!(tm.active_go_back(), None);
        assert_eq!(tm.active_go_forward(), Some("b".to_string()));
    }

    #[test]
    fn navigate_after_back_truncates_forward_branch() {
        let mut tm = TileManager::new();
        let node = fixture_node(1);
        tm.open_or_focus(node, "a".to_string(), fixture_doc("a"));
        tm.push_history("b".to_string(), fixture_doc("b"));
        tm.push_history("c".to_string(), fixture_doc("c"));
        tm.active_go_back();
        tm.push_history("d".to_string(), fixture_doc("d"));
        let state = tm.active_state().unwrap();
        assert_eq!(
            state
                .history
                .iter()
                .map(|e| e.url.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "d"]
        );
        assert_eq!(state.history_cursor, 2);
    }

    #[test]
    fn refresh_active_replaces_current_document() {
        let mut tm = TileManager::new();
        let node = fixture_node(1);
        tm.open_or_focus(node, "a".to_string(), fixture_doc("a"));
        let new_doc = EngineDocument {
            title: Some("refreshed".to_string()),
            ..fixture_doc("a")
        };
        assert!(tm.refresh_active(new_doc));
        assert_eq!(
            tm.active_document().unwrap().title.as_deref(),
            Some("refreshed")
        );
    }

    #[test]
    fn close_index_shifts_active_correctly() {
        let mut tm = TileManager::new();
        for i in 1..=3 {
            tm.open_or_focus(
                fixture_node(i),
                format!("addr-{i}"),
                fixture_doc(&format!("addr-{i}")),
            );
        }
        let active_after = tm.close_index(2);
        assert_eq!(active_after, Some(fixture_node(2)));
        assert_eq!(tm.active_index(), Some(1));

        let active_after = tm.close_index(0);
        assert_eq!(active_after, Some(fixture_node(2)));
        assert_eq!(tm.active_index(), Some(0));
    }

    #[test]
    fn revisiting_existing_anchor_refreshes_document() {
        let mut tm = TileManager::new();
        let node = fixture_node(1);
        tm.open_or_focus(node, "a".to_string(), fixture_doc("a"));
        let revised = EngineDocument {
            title: Some("revised".to_string()),
            ..fixture_doc("a")
        };
        let created_new = tm.open_or_focus(node, "a".to_string(), revised);
        assert!(!created_new);
        assert_eq!(
            tm.active_document().unwrap().title.as_deref(),
            Some("revised")
        );
    }
}
