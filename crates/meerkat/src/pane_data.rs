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

use eidetic::{BlobManifest, DeletedNode};
use frame::PaneContent;
use kernel::graph::Node;

use super::WindowCtx;

struct InspectorPaneInput<'a> {
    node: Option<&'a Node>,
    state: Option<&'a crate::fetch::ContentState>,
}

struct TrailRemovedInput {
    node_id: String,
    url: String,
}

struct TrailPaneInput {
    recent_urls: Vec<String>,
    history_urls: Vec<String>,
    removed: Vec<TrailRemovedInput>,
}

struct AlembicRecentInput {
    url: String,
    shown: String,
}

struct AlembicSavedInput {
    url: String,
    label: String,
}

struct AlembicEngramInput {
    id: String,
    byte_size: u64,
    pending_compose: bool,
}

struct AlembicPaneInput {
    eviction_policy_label: String,
    recent: Vec<AlembicRecentInput>,
    saved: Vec<AlembicSavedInput>,
    engrams: Vec<AlembicEngramInput>,
}

struct ApparatusSyncInput {
    active: bool,
    label: String,
    ops_label: String,
    last_activity_ms: Option<u64>,
}

impl WindowCtx<'_> {
    pub(super) fn utility_pane_rows(&self, content: &PaneContent) -> Vec<(String, String)> {
        match content {
            PaneContent::Inspector => {
                let input = self.inspector_pane_input();
                super::inspector::inspector_rows(input.node, input.state)
            }
            PaneContent::Steward => self.steward_rows(),
            _ => Vec::new(),
        }
    }

    /// The Trail pane's item list: the graph-wide recently-visited nodes, the
    /// focused node's own url history, and the eidetic deleted-nodes log — three
    /// titled sections of inert rows. (Lineage + eidetic pane.)
    pub(super) fn trail_items(&mut self) -> Vec<crate::list_pane::PaneItem> {
        let input = self.pane_input_snapshot();
        let recent_urls = self
            .orrery()
            .graph()
            .recent_visited(8)
            .into_iter()
            .map(|rv| rv.url)
            .collect();

        // The focused node's own url history (shown only when it has gone somewhere).
        let history_urls = input
            .focused_member()
            .and_then(|member| {
                self.orrery()
                    .graph()
                    .get_node_by_id(member)
                    .map(|(key, _)| key)
            })
            .map(|key| self.orrery().graph().node_history_projection(key).entries)
            .unwrap_or_default();

        // Removed: the eidetic deleted-nodes log (newest first; fjall resolves
        // synchronously, so `block_on` does not stall the UI).
        let removed = self
            .deleted_node_log()
            .into_iter()
            .take(12)
            .map(|tomb| TrailRemovedInput {
                node_id: tomb.node_id.to_string(),
                url: tomb.url,
            })
            .collect();
        build_trail_items(TrailPaneInput {
            recent_urls,
            history_urls,
            removed,
        })
    }

    /// The Alembic memory pane's item list: **Recent** (short-term working set),
    /// **Saved** (long-term — pinned or tagged nodes; tagging retains into long-term
    /// per the promotion model), and **Engrams** (the content-addressed graph engrams
    /// in private memory). Three titled sections of rows. Recent / Saved are a grounded
    /// subset until slice C (explicit promote / evict); B1 lists engrams, B2 makes a row
    /// clickable to thaw into the orrery. (Alembic memory pane.)
    pub(super) fn alembic_items(&mut self) -> Vec<crate::list_pane::PaneItem> {
        let input = self.pane_input_snapshot();
        let recent = {
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
                .map(|rv| AlembicRecentInput {
                    shown: short_url(&rv.url),
                    url: rv.url.clone(),
                })
                .collect()
        };

        // Saved (long-term): tagged nodes — tagging is the promotion act (decision #2). A scoped
        // block so the graph borrow ends before the store borrow below.
        let saved = {
            let graph = self.orrery().graph();
            let mut s: Vec<AlembicSavedInput> = graph
                .nodes()
                .filter(|(_, node)| !node.tags.is_empty())
                .map(|(key, node)| AlembicSavedInput {
                    url: node.url().to_string(),
                    label: graph.node_display_label(key),
                })
                .collect();
            s.sort_by(|a, b| a.label.cmp(&b.label));
            s
        };

        // Engrams (distillation): the content-addressed graph engrams in private memory.
        // fjall resolves synchronously, so `block_on` does not stall the UI (the Trail /
        // deleted-log pattern). B2 makes these rows clickable to thaw into the orrery.
        let engrams = self
            .graph_engram_manifests()
            .into_iter()
            .take(20)
            .map(|manifest| {
                let id = manifest.id.to_string();
                AlembicEngramInput {
                    pending_compose: input.pending_compose_engram() == Some(id.as_str()),
                    id,
                    byte_size: manifest.byte_size,
                }
            })
            .collect();
        build_alembic_items(AlembicPaneInput {
            eviction_policy_label: input.eviction_policy().describe().to_string(),
            recent,
            saved,
            engrams,
        })
    }

    /// The at-rest sync record for Apparatus — the record half of the static-vs-live
    /// split (Steward owns the live, actionable rows in `steward::steward_sync_rows`;
    /// Apparatus owns the record). The lane's on/off state, the caught-up op total, and how
    /// long since the last sync activity. Drawn from the same `SyncIndicator` the
    /// host folds each frame, so it stays honest (no placebo). (Chrome bar P1.)
    pub(super) fn apparatus_sync_rows(&self) -> Vec<(String, String)> {
        let indicator = &self.multi.state().shared.sync;
        build_apparatus_sync_rows(ApparatusSyncInput {
            active: indicator.active,
            label: indicator.label.clone(),
            ops_label: indicator.ops.to_string(),
            last_activity_ms: indicator.last_activity_ms,
        })
    }

    fn inspector_pane_input(&self) -> InspectorPaneInput<'_> {
        let input = self.pane_input_snapshot();
        let node = input
            .focused_member()
            .and_then(|member| self.orrery().graph().get_node_by_id(member))
            .map(|(_, node)| node);
        let state = node.and_then(|node| self.shared.content.pages.get(node.url()));
        InspectorPaneInput { node, state }
    }

    fn deleted_node_log(&mut self) -> Vec<DeletedNode> {
        self.shared
            .content
            .store
            .as_mut()
            .map(|store| pollster::block_on(eidetic::list_deleted(store)).unwrap_or_default())
            .unwrap_or_default()
    }

    fn graph_engram_manifests(&mut self) -> Vec<BlobManifest> {
        self.shared
            .content
            .store
            .as_mut()
            .map(|store| {
                pollster::block_on(session_runtime::graph_engram::list_graph_engrams(store))
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }
}

fn short_url(url: &str) -> String {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
        .chars()
        .take(56)
        .collect()
}

fn short_label(label: &str) -> String {
    label.chars().take(56).collect()
}

fn build_trail_items(input: TrailPaneInput) -> Vec<crate::list_pane::PaneItem> {
    use crate::list_pane::PaneItem;

    let mut items = vec![PaneItem::text("utility-title", "Recent")];
    if input.recent_urls.is_empty() {
        items.push(PaneItem::text("utility-row-muted", "nothing visited yet"));
    } else {
        for url in &input.recent_urls {
            items.push(PaneItem::text("utility-row", short_url(url)));
        }
    }

    if input.history_urls.len() > 1 {
        items.push(PaneItem::text("utility-title", "This node"));
        for url in &input.history_urls {
            items.push(PaneItem::text("utility-row", short_url(url)));
        }
    }

    if !input.removed.is_empty() {
        items.push(PaneItem::text("utility-title", "Removed"));
        for tomb in &input.removed {
            items.push(PaneItem::button(
                "utility-row",
                short_url(&tomb.url),
                format!("recover:{}", tomb.node_id),
            ));
        }
    }
    items
}

fn build_alembic_items(input: AlembicPaneInput) -> Vec<crate::list_pane::PaneItem> {
    use crate::list_pane::PaneItem;

    let mut items = vec![PaneItem::text("utility-title", "Recent")];
    items.push(PaneItem::button(
        "utility-row-muted",
        format!("{} \u{21bb}", input.eviction_policy_label),
        "alembic:eviction:cycle",
    ));
    items.push(PaneItem::button(
        "utility-row-muted",
        "\u{232b} forget stale recent now",
        "alembic:forget",
    ));
    if input.recent.is_empty() {
        items.push(PaneItem::text(
            "utility-row-muted",
            "nothing in recent working memory",
        ));
    } else {
        for row in &input.recent {
            items.push(PaneItem::button(
                "utility-row",
                format!("\u{2606} {}", row.shown),
                format!("alembic:keep:{}", row.url),
            ));
        }
    }

    items.push(PaneItem::text(
        "utility-title",
        format!("Saved ({})", input.saved.len()),
    ));
    if input.saved.is_empty() {
        items.push(PaneItem::text(
            "utility-row-muted",
            "tag a node to keep it long-term",
        ));
    } else {
        for row in input.saved.iter().take(12) {
            items.push(PaneItem::button(
                "utility-row",
                format!("\u{2605} {}", short_label(&row.label)),
                format!("alembic:release:{}", row.url),
            ));
        }
    }

    items.push(PaneItem::text("utility-title", "Engrams"));
    if input.engrams.is_empty() {
        items.push(PaneItem::text(
            "utility-row-muted",
            "save a graph as an engram (>save_graph_engram)",
        ));
    } else {
        for engram in &input.engrams {
            let id_short: String = engram.id.chars().take(20).collect();
            items.push(PaneItem::button(
                "utility-row",
                format!("{id_short} · {} B", engram.byte_size),
                format!("engram:open:{}", engram.id),
            ));
            items.push(PaneItem::button(
                "utility-row",
                if engram.pending_compose {
                    "⊗ selected for compose"
                } else {
                    "⊕ compose"
                },
                format!("engram:compose:{}", engram.id),
            ));
        }
    }
    items
}

fn build_apparatus_sync_rows(input: ApparatusSyncInput) -> Vec<(String, String)> {
    if !input.active {
        return vec![("Lane".to_string(), "off".to_string())];
    }
    let mut rows = vec![
        ("Lane".to_string(), input.label),
        ("Caught-up ops".to_string(), input.ops_label),
    ];
    if let Some(then_ms) = input.last_activity_ms {
        rows.push(("Last activity".to_string(), unix_age(then_ms)));
    }
    rows
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_apparatus_sync_rows_reports_off_lane() {
        let rows = build_apparatus_sync_rows(ApparatusSyncInput {
            active: false,
            label: "mesh".to_string(),
            ops_label: "7".to_string(),
            last_activity_ms: Some(1),
        });
        assert_eq!(rows, vec![("Lane".to_string(), "off".to_string())]);
    }

    #[test]
    fn build_trail_items_includes_removed_recovery_buttons() {
        let items = build_trail_items(TrailPaneInput {
            recent_urls: vec!["https://example.com/a".to_string()],
            history_urls: vec![
                "https://example.com/a".to_string(),
                "https://example.com/b".to_string(),
            ],
            removed: vec![TrailRemovedInput {
                node_id: "123".to_string(),
                url: "https://example.com/z".to_string(),
            }],
        });
        assert!(items.iter().any(|item| item.text == "Recent"));
        assert!(
            items
                .iter()
                .any(|item| item.key.as_deref() == Some("recover:123"))
        );
    }
}
