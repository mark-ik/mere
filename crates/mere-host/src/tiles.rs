/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Workbench tile state.
//!
//! Per Mark's framing: **every tile is a facet of a graph node**.
//! Opening a tile finds (or creates) the node for the address; the
//! tile becomes one way of viewing that node. Closing the tile drops
//! the visual binding — the node itself stays in the graph for the
//! orrery to show.
//!
//! `TileManager` owns the open tile list, the active index, and a
//! cache of rendered documents keyed by `NodeKey` so flipping between
//! tiles doesn't refetch.

use std::collections::HashMap;

use inker::EngineDocument;
use mere_kernel::graph::NodeKey;

/// Workbench tile state. Tiles are an ordered list (matches the
/// strip the user sees); `active` indexes into it.
#[derive(Default)]
pub struct TileManager {
    open: Vec<NodeKey>,
    active: Option<usize>,
    documents: HashMap<NodeKey, EngineDocument>,
}

impl TileManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_tiles(&self) -> &[NodeKey] {
        &self.open
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn active_node(&self) -> Option<NodeKey> {
        self.active.and_then(|i| self.open.get(i).copied())
    }

    pub fn active_document(&self) -> Option<&EngineDocument> {
        self.active_node().and_then(|n| self.documents.get(&n))
    }

    pub fn document_for(&self, node: NodeKey) -> Option<&EngineDocument> {
        self.documents.get(&node)
    }

    /// Open `node` as a new tile or focus the existing tile for it,
    /// in either case setting `active` to that tile. Returns `true`
    /// when a new tile was added, `false` when an existing one was
    /// re-focused.
    pub fn open_or_focus(&mut self, node: NodeKey, document: EngineDocument) -> bool {
        self.documents.insert(node, document);
        if let Some(i) = self.open.iter().position(|n| *n == node) {
            self.active = Some(i);
            false
        } else {
            self.open.push(node);
            self.active = Some(self.open.len() - 1);
            true
        }
    }

    /// Replace the document for an already-open tile (e.g. on
    /// reload). No-op if `node` isn't currently open.
    pub fn refresh_document(&mut self, node: NodeKey, document: EngineDocument) {
        if self.open.contains(&node) {
            self.documents.insert(node, document);
        }
    }

    /// Set the active tile to `index` if it is in range. Returns the
    /// resulting active `NodeKey` if any.
    pub fn focus_index(&mut self, index: usize) -> Option<NodeKey> {
        if index < self.open.len() {
            self.active = Some(index);
        }
        self.active_node()
    }

    /// Close the tile at `index`. The document cache is dropped only
    /// when the closed tile is the last reference to that node — but
    /// since tiles can only carry one entry per node today, this is
    /// always the case. Returns the new active node, if any.
    pub fn close_index(&mut self, index: usize) -> Option<NodeKey> {
        if index >= self.open.len() {
            return self.active_node();
        }
        let removed = self.open.remove(index);
        self.documents.remove(&removed);
        if self.open.is_empty() {
            self.active = None;
        } else if let Some(active) = self.active {
            // Preserve the active tile across the removal:
            //  - closing before the active one shifts left
            //  - closing the active one falls back to the previous
            //    entry (or 0 if there is nothing before)
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
