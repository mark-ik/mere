/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! JSON-LD graph export (Lane 0 sidequest #1).
//!
//! The dormant inverse of the wired linked-data *ingest* (`apply_contribution`
//! off the fetch drain): `linked_data::to_jsonld_string` over the focused graph,
//! written to a file under `<mere_root>/exports/`. Reached as `Command::ExportGraph`
//! (palette + `>export_graph`); the host drains it and echoes the path. A first cut
//! writes a timestamped file; a native Save-As dialog is the follow-up.

use super::WindowCtx;

impl WindowCtx<'_> {
    /// Export the focused graph as JSON-LD to
    /// `<mere_root>/exports/graph-<unix-secs>.jsonld`, returning a one-line result
    /// (node count + path, or the error) for the omnibar to echo. Read-only over
    /// the graph; the only side effect is the written file.
    pub(super) fn export_graph_jsonld(&self) -> String {
        let graph = self.orrery().graph();
        let node_count = graph.nodes().count();
        let document = linked_data::to_jsonld_string(graph);

        let dir = self.shared.session.mere_root.join("exports");
        if let Err(err) = std::fs::create_dir_all(&dir) {
            return format!("Export failed (could not create {}): {err}", dir.display());
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = dir.join(format!("graph-{stamp}.jsonld"));
        match std::fs::write(&path, document) {
            Ok(()) => format!("Exported {node_count} node(s) → {}", path.display()),
            Err(err) => format!("Export failed: {err}"),
        }
    }

    /// Freeze the focused graph into a content-addressed graph engram in the
    /// private eidetic store — the Alembic memory spine's "Save as graph engram".
    /// Redacts private / heavy fields by default (thumbnails, favicons, session
    /// state); returns a one-line note (engram id + node count, or the error) for
    /// the omnibar. fjall resolves synchronously, so the `block_on` does not stall
    /// the UI — the same shape as the deleted-node tombstone path.
    pub(super) fn save_graph_engram(&mut self) -> String {
        use session_runtime::graph_engram::{RedactionPolicy, save_graph_snapshot_engram};

        // Snapshot first (ends the borrow of the live graph) so the store borrow
        // that follows does not conflict.
        let snapshot = self.orrery().graph().to_snapshot();
        let node_count = snapshot.nodes.len();
        let created_at = eidetic::Timestamp(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        );

        let Some(store) = self.shared.content.store.as_mut() else {
            return "Save engram failed: no private memory store is open".to_string();
        };
        match pollster::block_on(save_graph_snapshot_engram(
            store,
            snapshot,
            RedactionPolicy::default(),
            created_at,
        )) {
            Ok(id) => format!("Saved graph engram {id} ({node_count} node(s))"),
            Err(err) => format!("Save engram failed: {err}"),
        }
    }
}
