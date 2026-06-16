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
}
