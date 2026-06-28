/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pane row/data builders for a window: Inspector utility rows, the Apparatus
//! at-rest sync record, the Alembic engram list, and their small label helpers.
//! The Steward pane's builders live in `steward.rs`; the graph-object Roster's
//! builders live in `roster_data.rs`.
//! These project graph + content state into the `(String, String)` / row shapes
//! the list panes render. Factored out of `frame_ops.rs` to keep files under the
//! 600-LOC ceiling.

use frame::PaneContent;

use super::WindowCtx;

impl WindowCtx<'_> {
    pub(super) fn utility_pane_rows(&self, content: &PaneContent) -> Vec<(String, String)> {
        match content {
            PaneContent::Inspector => {
                let focused = self.focused_member();
                let node = focused
                    .and_then(|member| self.orrery().graph().get_node_by_id(member))
                    .map(|(_, node)| node);
                let state = node.and_then(|node| self.shared.content.pages.get(node.url()));
                super::inspector::inspector_rows(node, state)
            }
            PaneContent::Steward => self.steward_rows(),
            _ => Vec::new(),
        }
    }

    /// The Trail pane's item list: the graph-wide recently-visited nodes, the
    /// focused node's own url history, and the eidetic deleted-nodes log — three
    /// titled sections of inert rows. (Lineage + eidetic pane.)
    pub(super) fn trail_items(&mut self) -> Vec<crate::list_pane::PaneItem> {
        use crate::list_pane::PaneItem;
        // Strip the scheme and cap the length so a row reads cleanly.
        let short = |url: &str| -> String {
            url.strip_prefix("https://")
                .or_else(|| url.strip_prefix("http://"))
                .unwrap_or(url)
                .chars()
                .take(56)
                .collect()
        };

        let mut items = vec![PaneItem::text("utility-title", "Recent")];
        let recent = self.orrery().graph().recent_visited(8);
        if recent.is_empty() {
            items.push(PaneItem::text("utility-row-muted", "nothing visited yet"));
        } else {
            for rv in &recent {
                items.push(PaneItem::text("utility-row", short(&rv.url)));
            }
        }

        // The focused node's own url history (shown only when it has gone somewhere).
        let history = self
            .focused_member()
            .and_then(|member| {
                self.orrery()
                    .graph()
                    .get_node_by_id(member)
                    .map(|(key, _)| key)
            })
            .map(|key| self.orrery().graph().node_history_projection(key).entries)
            .unwrap_or_default();
        if history.len() > 1 {
            items.push(PaneItem::text("utility-title", "This node"));
            for url in &history {
                items.push(PaneItem::text("utility-row", short(url)));
            }
        }

        // Removed: the eidetic deleted-nodes log (newest first; fjall resolves
        // synchronously, so `block_on` does not stall the UI).
        let removed = self
            .shared
            .content
            .store
            .as_mut()
            .map(|store| pollster::block_on(eidetic::list_deleted(store)).unwrap_or_default())
            .unwrap_or_default();
        if !removed.is_empty() {
            items.push(PaneItem::text("utility-title", "Removed"));
            for tomb in removed.iter().take(12) {
                // A clickable row: a click queues `recover:<node_id>`, which the host
                // re-mints back into the graph. (Recover-deleted-node, Lane 0.)
                items.push(PaneItem::button(
                    "utility-row",
                    short(&tomb.url),
                    format!("recover:{}", tomb.node_id),
                ));
            }
        }
        items
    }

    /// The Alembic memory pane's item list: **Recent** (short-term working set),
    /// **Saved** (long-term — pinned or tagged nodes; tagging retains into long-term
    /// per the promotion model), and **Engrams** (the content-addressed graph engrams
    /// in private memory). Three titled sections of rows. Recent / Saved are a grounded
    /// subset until slice C (explicit promote / evict); B1 lists engrams, B2 makes a row
    /// clickable to thaw into the orrery. (Alembic memory pane.)
    pub(super) fn alembic_items(&mut self) -> Vec<crate::list_pane::PaneItem> {
        use crate::list_pane::PaneItem;
        let clip = |s: &str| -> String { s.chars().take(56).collect() };
        let strip = |url: &str| -> String {
            url.strip_prefix("https://")
                .or_else(|| url.strip_prefix("http://"))
                .unwrap_or(url)
                .chars()
                .take(56)
                .collect()
        };

        // Recent (short-term): the working set — recently-visited *untagged* nodes, with the
        // eviction policy shown (the visible, never-silent policy). The untagged = short-term
        // rule mirrors `memory_levels::level_of`, the canonical model (decision #2: a tag is
        // the promotion act; `is_pinned` is a physics position-pin, not a memory-keep). Slice C.
        let mut items = vec![PaneItem::text("utility-title", "Recent")];
        // The eviction policy is editable: this row cycles it (7d -> 30d -> 90d -> forever) and
        // persists; the next forgetting pass uses the new policy. (Editable eviction policy, B4.)
        items.push(PaneItem::button(
            "utility-row-muted",
            format!(
                "{} \u{21bb}",
                self.shared.presentation.eviction_policy.describe()
            ),
            "alembic:eviction:cycle",
        ));
        // A real "forget now" affordance: runs Athanor's forgetting pass, dropping stale
        // short-term cached content (the count surfaces in Steward). No placebo. (Slice C/D.)
        items.push(PaneItem::button(
            "utility-row-muted",
            "\u{232b} forget stale recent now",
            "alembic:forget",
        ));
        let recent: Vec<(String, String)> = {
            let graph = self.orrery().graph();
            graph
                .recent_visited(8)
                .into_iter()
                // A tagged (long-term) node lives under Saved, not here.
                .filter(|rv| {
                    graph
                        .get_node_by_url(&rv.url)
                        .is_none_or(|(_, n)| n.tags.is_empty())
                })
                .map(|rv| (rv.url.clone(), strip(&rv.url)))
                .collect()
        };
        if recent.is_empty() {
            items.push(PaneItem::text(
                "utility-row-muted",
                "nothing in recent working memory",
            ));
        } else {
            // Each row is a one-click "keep": ☆ promotes it to Saved (adds the reserved `saved`
            // tag). (Alembic one-click promote, B3.)
            for (url, shown) in &recent {
                items.push(PaneItem::button(
                    "utility-row",
                    format!("\u{2606} {shown}"),
                    format!("alembic:keep:{url}"),
                ));
            }
        }

        // Saved (long-term): tagged nodes — tagging is the promotion act (decision #2). A scoped
        // block so the graph borrow ends before the store borrow below.
        let saved: Vec<(String, String)> = {
            let graph = self.orrery().graph();
            let mut s: Vec<(String, String)> = graph
                .nodes()
                .filter(|(_, node)| !node.tags.is_empty())
                .map(|(key, node)| (node.url().to_string(), graph.node_display_label(key)))
                .collect();
            s.sort_by(|a, b| a.1.cmp(&b.1));
            s
        };
        items.push(PaneItem::text(
            "utility-title",
            format!("Saved ({})", saved.len()),
        ));
        if saved.is_empty() {
            items.push(PaneItem::text(
                "utility-row-muted",
                "tag a node to keep it long-term",
            ));
        } else {
            // Each row is a one-click "release": ★ removes the reserved `saved` tag (a node still
            // tagged otherwise stays Saved). (Alembic one-click demote, B3.)
            for (url, label) in saved.iter().take(12) {
                items.push(PaneItem::button(
                    "utility-row",
                    format!("\u{2605} {}", clip(label)),
                    format!("alembic:release:{url}"),
                ));
            }
        }

        // Engrams (distillation): the content-addressed graph engrams in private memory.
        // fjall resolves synchronously, so `block_on` does not stall the UI (the Trail /
        // deleted-log pattern). B2 makes these rows clickable to thaw into the orrery.
        items.push(PaneItem::text("utility-title", "Engrams"));
        let engrams = self
            .shared
            .content
            .store
            .as_mut()
            .map(|store| {
                pollster::block_on(session_runtime::graph_engram::list_graph_engrams(store))
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        if engrams.is_empty() {
            items.push(PaneItem::text(
                "utility-row-muted",
                "save a graph as an engram (>save_graph_engram)",
            ));
        } else {
            for manifest in engrams.iter().take(20) {
                // A clickable row: the key carries the full manifest id; a click queues
                // `engram:open:<id>`, which the host thaws into an Orrery pane beside. The
                // label shows a short id + size. (Alembic — open an engram, B2.)
                let id_short: String = manifest.id.to_string().chars().take(20).collect();
                items.push(PaneItem::button(
                    "utility-row",
                    format!("{id_short} · {} B", manifest.byte_size),
                    format!("engram:open:{}", manifest.id),
                ));
            }
        }
        items
    }

    /// The at-rest sync record for Apparatus — the record half of the static-vs-live
    /// split (Steward owns the live, actionable rows in `steward::steward_sync_rows`;
    /// Apparatus owns the record). The lane's on/off state, the caught-up op total, and how
    /// long since the last sync activity. Drawn from the same `SyncIndicator` the
    /// host folds each frame, so it stays honest (no placebo). (Chrome bar P1.)
    pub(super) fn apparatus_sync_rows(&self) -> Vec<(String, String)> {
        let indicator = &self.view.chrome().sync;
        if !indicator.active {
            return vec![("Lane".to_string(), "off".to_string())];
        }
        let mut rows = vec![
            ("Lane".to_string(), indicator.label.clone()),
            ("Caught-up ops".to_string(), indicator.ops.to_string()),
        ];
        if let Some(then_ms) = indicator.last_activity_ms {
            rows.push(("Last activity".to_string(), unix_age(then_ms)));
        }
        rows
    }
}

/// A coarse "N ago" readout from a Unix-epoch-ms timestamp, for the Apparatus
/// at-rest sync record (`SyncIndicator::last_activity_ms` is Unix-epoch, not the
/// monotonic `Instant` the observability `age` helper takes). (Chrome bar P1.)
fn unix_age(then_ms: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let secs = now_ms.saturating_sub(then_ms) / 1_000;
    if secs < 1 {
        "now".to_string()
    } else if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3_600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3_600)
    }
}
