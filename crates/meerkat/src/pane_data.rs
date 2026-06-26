/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pane row/data builders for a window: the roster's node rows (grouped by
//! content bucket, with edge detail for a sole selection) and field rows, the
//! Inspector / Steward utility rows, and the small label helpers they share.
//! These project graph + content state into the `(String, String)` / row shapes
//! the list panes render. Factored out of `frame_ops.rs` to keep files under the
//! 600-LOC ceiling.

use forme::GraphMemberId;
use frame::PaneContent;
use kernel::graph::{ContainmentSubKind, ProvenanceSubKind, RelationKind, SemanticSubKind};
use orrery::NodeShape;

use super::node_ops::content_shape;
use super::{WindowCtx, fetch, roster};

impl WindowCtx<'_> {
    /// The roster rows: every graph node as a row (url as title, content type as
    /// subtitle, focused node marked selected). (Frame tree, F1 roster.)
    pub(super) fn roster_rows(&self) -> Vec<roster::RosterRow> {
        // Highlight by member id against the live selection set (not by focused_url,
        // which collapses to None for a multi-selection and aliases duplicate URLs
        // — both common now that Add node / New tile mint same-URL nodes).
        let selected_members: std::collections::HashSet<GraphMemberId> =
            self.orrery().selected_members().into_iter().collect();
        let graph = self.orrery().graph();
        let mut rows: Vec<roster::RosterRow> = self
            .orrery()
            .graph()
            .nodes()
            .map(|(key, node)| {
                let url = node.url().to_string();
                // A human-meaningful label: a real title, else an ingested entity's
                // role / @type ("publisher" / "Organization"), else the host, never a
                // raw `urn:mere:bnode:` string. (Node legibility.)
                let title = graph.node_display_label(key);
                let content_type = match self.shared.content.pages.get(&url) {
                    Some(fetch::ContentState::Ready(fetched)) => fetched.content_type.clone(),
                    _ => node.mime_hint.clone(),
                };
                let mut tags: Vec<String> = node.tags.iter().cloned().collect();
                tags.sort();
                let selected = selected_members.contains(&node.id);
                // Edge detail only for a sole focused node (not every row of a
                // multi-selection), keeping it O(n) once, never per selected row.
                let edges = if selected && selected_members.len() == 1 {
                    let node_key = graph.get_node_by_id(node.id).map(|(k, _)| k);
                    if let Some(key) = node_key {
                        graph
                            .relations()
                            .filter(|r| r.from == key || r.to == key)
                            .filter_map(|r| {
                                let (direction, other_key) = if r.from == key {
                                    (roster::EdgeDir::Out, r.to)
                                } else {
                                    (roster::EdgeDir::In, r.from)
                                };
                                let other = graph.get_node(other_key)?;
                                let other_title = graph.node_display_label(other_key);
                                Some(roster::EdgeRow {
                                    direction,
                                    kind_label: relation_kind_label(r.kind).to_string(),
                                    other_title,
                                    other_url: other.url().to_string(),
                                    other_member: other.id,
                                })
                            })
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };
                roster::RosterRow {
                    member: node.id,
                    title,
                    url,
                    content_type,
                    tags,
                    edges,
                    selected,
                    section_header: None,
                }
            })
            .collect();
        // Sort by (content-type bucket, title) so nodes group by kind.
        rows.sort_by(|a, b| {
            let ba = content_bucket(a.content_type.as_deref());
            let bb = content_bucket(b.content_type.as_deref());
            ba.0.cmp(&bb.0)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        // Stamp section headers on the first row of each new bucket.
        let mut current: Option<u8> = None;
        for row in &mut rows {
            let (ord, label) = content_bucket(row.content_type.as_deref());
            if current != Some(ord) {
                current = Some(ord);
                row.section_header = Some(label.to_string());
            }
        }
        rows
    }

    /// Field-region rows for the roster: one per active field, its display name (the
    /// authoring name, else a short id) and hidden state. (Field regions — roster.)
    pub(super) fn roster_field_rows(&self) -> Vec<roster::FieldRow> {
        let mut out = Vec::new();
        for field in self.orrery().graph().fields() {
            if !field.is_active() {
                continue;
            }
            let id = field.id;
            let uuid = id.as_uuid().to_string();
            let name = field
                .name
                .clone()
                .unwrap_or_else(|| format!("Field {}", &uuid[..8.min(uuid.len())]));
            out.push(roster::FieldRow {
                id,
                name,
                hidden: !self.orrery().field_visible(id),
                strength: self.orrery().field_strength(id).unwrap_or(0.0),
            });
        }
        // Deterministic order (graph.fields() is HashMap-unordered): by name, then id
        // — matching the node rows' explicit sort. (Field regions — roster.)
        out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.as_uuid().cmp(&b.id.as_uuid()))
        });
        out
    }

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
            .and_then(|member| self.orrery().graph().get_node_by_id(member).map(|(key, _)| key))
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
            format!("{} \u{21bb}", self.shared.presentation.eviction_policy.describe()),
            "alembic:eviction:cycle",
        ));
        // A real "forget now" affordance: runs Athanor's forgetting pass, dropping stale
        // short-term cached content (the count surfaces in Steward). No placebo. (Slice C/D.)
        items.push(PaneItem::button(
            "utility-row-muted",
            "\u{232b} forget stale recent now",
            "alembic:forget",
        ));
        let recent: Vec<String> = {
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
                .map(|rv| strip(&rv.url))
                .collect()
        };
        if recent.is_empty() {
            items.push(PaneItem::text("utility-row-muted", "nothing in recent working memory"));
        } else {
            for row in &recent {
                items.push(PaneItem::text("utility-row", row.clone()));
            }
        }

        // Saved (long-term): tagged nodes — tagging is the promotion act (decision #2). A scoped
        // block so the graph borrow ends before the store borrow below.
        let saved: Vec<String> = {
            let graph = self.orrery().graph();
            let mut s: Vec<String> = graph
                .nodes()
                .filter(|(_, node)| !node.tags.is_empty())
                .map(|(key, _)| graph.node_display_label(key))
                .collect();
            s.sort();
            s
        };
        items.push(PaneItem::text(
            "utility-title",
            format!("Saved ({})", saved.len()),
        ));
        if saved.is_empty() {
            items.push(PaneItem::text("utility-row-muted", "tag a node to keep it long-term"));
        } else {
            for label in saved.iter().take(12) {
                items.push(PaneItem::text("utility-row", clip(label)));
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

    pub(super) fn steward_rows(&self) -> Vec<(String, String)> {
        let operations = self.shared.content.constellation.active_operations();
        let mut rows = vec![
            (
                "Active operations".to_string(),
                operations.len().to_string(),
            ),
            ("Tab cap".to_string(), self.shared.presentation.saved_tab_cap.to_string()),
            (
                "Live graphs".to_string(),
                format!("{} / {}", self.orrery_pool_count, super::MAX_POOLED_ORRERIES),
            ),
            (
                "Loading fetches".to_string(),
                self.fetch_state_count(1).to_string(),
            ),
            (
                "Failed fetches".to_string(),
                self.fetch_state_count(3).to_string(),
            ),
            ("Sync".to_string(), self.sync_summary()),
            (
                // Athanor's last forgetting pass, surfaced live here (not only in the
                // Apparatus diagnostics log). A real tracked op, no placebo. (Alembic B2.)
                "Last forgetting".to_string(),
                match self.shared.observability.last_forgetting() {
                    Some(p) => format!(
                        "dropped {} page(s) \u{b7} {}",
                        p.dropped,
                        crate::observability::age(p.at)
                    ),
                    None => "not run yet".to_string(),
                },
            ),
            (
                // The dialable ticket, readable + shareable here instead of only in the
                // logs. (Tessera ticket surface.)
                "Tessera ticket".to_string(),
                self.view
                    .chrome()
                    .sync
                    .ticket()
                    .map(str::to_string)
                    .unwrap_or_else(|| "\u{2014}".to_string()),
            ),
        ];
        let focused = self.focused_member();
        rows.push((
            "Focused operation".to_string(),
            match focused {
                Some(member) if self.shared.content.constellation.is_active(member) => format!(
                    "active background={} recovering={}",
                    self.shared.content.constellation.is_background(member),
                    self.shared.content.constellation.is_recovering(member)
                ),
                Some(_) => "dormant".to_string(),
                None => "none".to_string(),
            },
        ));
        for operation in operations.into_iter().take(6) {
            rows.push((
                format!("Operation {}", short_member(operation.member)),
                format!(
                    "{} background={} recovering={} scene={} height={}",
                    operation.url.as_deref().unwrap_or("not shown yet"),
                    operation.background,
                    operation.recovering,
                    operation.scene_version,
                    operation.content_height
                ),
            ));
        }
        rows
    }

    /// The Steward pane as a clickable item list: the live-ops status rows (from
    /// [`steward_rows`](Self::steward_rows)) followed by real action buttons for
    /// the focused operation — retry / stop / background-pin. Each button queues a
    /// `steward:*` key that `drain_list_pane_activations` routes to the existing
    /// node-ops verb, so the actions are reachable by click, not only by typing
    /// `>retry.focused`. Mirrors the Alembic pane's bespoke item builder rather
    /// than the inert `utility_pane_items` path. (Audit A2.)
    pub(super) fn steward_items(&self) -> Vec<crate::list_pane::PaneItem> {
        use crate::list_pane::PaneItem;
        let mut items = vec![PaneItem::text(
            "utility-title",
            crate::utility_panes::pane_title(&PaneContent::Steward),
        )];
        for (label, value) in self.steward_rows() {
            items.push(PaneItem::text("utility-row", format!("{label}: {value}")));
        }
        // Real verbs on the focused operation (no placebo): the drain maps each key
        // to its node-ops method.
        items.push(PaneItem::button("utility-row", "\u{21bb} retry focused", "steward:retry"));
        items.push(PaneItem::button("utility-row", "\u{23f9} stop focused", "steward:stop"));
        items.push(PaneItem::button(
            "utility-row",
            "\u{2693} pin focused (background)",
            "steward:pin",
        ));
        items.push(PaneItem::text(
            "utility-row-muted",
            crate::utility_panes::pane_status(&PaneContent::Steward),
        ));
        items
    }

    fn fetch_state_count(&self, tag: u8) -> usize {
        self.shared.content.pages
            .values()
            .filter(|state| fetch::ContentState::tag(Some(*state)) == tag)
            .count()
    }

    fn sync_summary(&self) -> String {
        let indicator = &self.view.chrome().sync;
        if !indicator.active {
            return "off".to_string();
        }
        // The earned standing is the headline; ops is the raw catch-up plumbing behind
        // it. (Tessera ledger fold.)
        let standing = indicator
            .standing
            .map(|s| format!(" standing={s:+}"))
            .unwrap_or_default();
        format!(
            "{} syncing={} ops={}{}",
            indicator.label, indicator.syncing, indicator.ops, standing
        )
    }
}

fn short_member(member: GraphMemberId) -> String {
    member.to_string().chars().take(8).collect()
}

fn content_bucket(content_type: Option<&str>) -> (u8, &'static str) {
    match content_type {
        None => (3, "Unknown"),
        Some(ct) => match content_shape(Some(ct)) {
            NodeShape::Circle => (1, "Feeds"),
            NodeShape::Rounded => (2, "Menus"),
            NodeShape::Square => (0, "Documents"),
        },
    }
}

fn relation_kind_label(kind: RelationKind) -> &'static str {
    use ContainmentSubKind::*;
    use ProvenanceSubKind::*;
    use SemanticSubKind::*;
    match kind {
        RelationKind::Traversal => "Traversal",
        RelationKind::Semantic(Hyperlink) => "Hyperlink",
        RelationKind::Semantic(UserGrouped) => "Grouped",
        RelationKind::Semantic(AgentDerived) => "Agent",
        RelationKind::Semantic(Cites) => "Cites",
        RelationKind::Semantic(Quotes) => "Quotes",
        RelationKind::Semantic(Summarizes) => "Summarizes",
        RelationKind::Semantic(Elaborates) => "Elaborates",
        RelationKind::Semantic(ExampleOf) => "Example",
        RelationKind::Semantic(Supports) => "Supports",
        RelationKind::Semantic(Contradicts) => "Contradicts",
        RelationKind::Semantic(Questions) => "Questions",
        RelationKind::Semantic(SameEntityAs) => "Same As",
        RelationKind::Semantic(DuplicateOf) => "Duplicate",
        RelationKind::Semantic(CanonicalMirrorOf) => "Mirror",
        RelationKind::Semantic(DependsOn) => "Depends",
        RelationKind::Semantic(Blocks) => "Blocks",
        RelationKind::Semantic(NextStep) => "Next",
        RelationKind::Containment(UrlPath) => "Path",
        RelationKind::Containment(Domain) => "Domain",
        RelationKind::Containment(FileSystem) => "Filesystem",
        RelationKind::Containment(UserFolder) => "Folder",
        RelationKind::Containment(ClipSource) => "Clip",
        RelationKind::Containment(NotebookSection) => "Section",
        RelationKind::Containment(CollectionMember) => "Collection",
        RelationKind::Arrangement(_) => "Arrangement",
        RelationKind::Imported(_) => "Imported",
        RelationKind::Provenance(ClippedFrom) => "Clipped",
        RelationKind::Provenance(ExcerptedFrom) => "Excerpt",
        RelationKind::Provenance(SummarizedFrom) => "Summary",
        RelationKind::Provenance(TranslatedFrom) => "Translation",
        RelationKind::Provenance(RewrittenFrom) => "Rewritten",
        RelationKind::Provenance(GeneratedFrom) => "Generated",
        RelationKind::Provenance(ExtractedFrom) => "Extracted",
        RelationKind::Provenance(ImportedFromSource) => "Imported",
        RelationKind::Provenance(CopiedFrom) => "Copied",
    }
}
