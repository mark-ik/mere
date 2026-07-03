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

    /// Union two graph engrams (by id string) into a new one by URL identity, retaining
    /// per-member provenance — the Alembic memory spine's "compose" (decision #1; B7-P3).
    /// Reachable directly as the `>compose_engrams("<id-a>", "<id-b>")` omnibar verb, and from
    /// the Engrams two-select gesture via [`toggle_compose_selection`](Self::toggle_compose_selection).
    /// No Athanor propose/apply here, unlike forgetting: the user already named the two ids, so
    /// there is no automatic-discovery judgment for Athanor to propose — this runs on confirm,
    /// the same shape as `save_graph_engram`. Returns a one-line note (new engram id, or the
    /// error) for the caller to surface.
    pub(super) fn compose_engrams(&mut self, id_a: &str, id_b: &str) -> String {
        use session_runtime::graph_engram::{RedactionPolicy, compose_graph_engrams};

        let Some(a) = eidetic::Hash::parse(id_a)
            .ok()
            .map(eidetic::ManifestId::from_hash)
        else {
            return format!("Compose failed: not a valid engram id: {id_a}");
        };
        let Some(b) = eidetic::Hash::parse(id_b)
            .ok()
            .map(eidetic::ManifestId::from_hash)
        else {
            return format!("Compose failed: not a valid engram id: {id_b}");
        };
        let created_at = eidetic::Timestamp(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        );

        let Some(store) = self.shared.content.store.as_mut() else {
            return "Compose failed: no private memory store is open".to_string();
        };
        match pollster::block_on(compose_graph_engrams(
            store,
            &[a, b],
            RedactionPolicy::default(),
            created_at,
        )) {
            Ok(Some(id)) => format!("Composed engram {id} from {id_a} + {id_b}"),
            Ok(None) => "Compose failed: one or both engram ids were not found".to_string(),
            Err(err) => format!("Compose failed: {err}"),
        }
    }

    /// The Alembic Engrams two-select gesture: click one engram's ⊕ to mark it pending, click a
    /// second (different) engram's ⊕ to compose them, or click the same engram again to
    /// deselect. One slot of state (`Presentation::pending_compose_engram`), not a multi-select
    /// list — the thinnest gesture that still reads as "pick two". A composed pair's result is
    /// recorded as an `alembic.compose` diagnostic (the Apparatus log), the same surface
    /// `run_forgetting_pass` uses, since this path has no omnibar to echo into. (B7-P3.)
    pub(super) fn toggle_compose_selection(&mut self, id: &str) {
        match self.shared.presentation.pending_compose_engram.take() {
            Some(pending) if pending == id => {} // same row again: deselect (already taken above)
            Some(pending) => {
                let note = self.compose_engrams(&pending, id);
                self.shared.observability.record_diagnostic(
                    "alembic.compose",
                    super::observability::Severity::Info,
                    note,
                );
            }
            None => self.shared.presentation.pending_compose_engram = Some(id.to_string()),
        }
        self.view.request_redraw();
    }

    /// Run Athanor's consolidation pass (Alembic B1-P2) over the private memory
    /// store: relate graph engrams that are successive versions of the same
    /// material (significant url overlap) but carry no lineage link yet, by
    /// composing each such pair — the only linking mechanism eidetic offers, since
    /// manifests are immutable once saved. Content-addressed, so re-linking an
    /// already-consolidated pair on a later pass is a safe no-op, not a duplicate.
    /// Records the count as an `alembic.consolidate` diagnostic, the same surface
    /// `run_forgetting_pass` uses — driven by the same idle cadence (B1-P1).
    pub(super) fn run_consolidation_pass(&mut self) {
        use session_runtime::athanor;
        use session_runtime::graph_engram::RedactionPolicy;

        let Some(store) = self.shared.content.store.as_mut() else {
            return;
        };
        let proposal = match pollster::block_on(athanor::propose_consolidation(store)) {
            Ok(proposal) => proposal,
            Err(err) => {
                self.shared.observability.record_diagnostic(
                    "alembic.consolidate",
                    super::observability::Severity::Warn,
                    format!("Consolidation propose failed: {err}"),
                );
                return;
            }
        };
        if proposal.is_empty() {
            self.shared.observability.record_diagnostic(
                "alembic.consolidate",
                super::observability::Severity::Info,
                "Consolidation: no unlinked version chains found".to_string(),
            );
            return;
        }
        let created_at = eidetic::Timestamp(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        );
        let Some(store) = self.shared.content.store.as_mut() else {
            return;
        };
        let linked = pollster::block_on(athanor::apply_consolidation(
            store,
            &proposal,
            RedactionPolicy::default(),
            created_at,
        ))
        .unwrap_or(0);
        self.shared.observability.record_diagnostic(
            "alembic.consolidate",
            super::observability::Severity::Info,
            format!("Consolidation: linked {linked} engram pair(s)"),
        );
    }
}
