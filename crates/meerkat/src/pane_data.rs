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
            self.orrery.selected_members().into_iter().collect();
        let graph = self.orrery.graph();
        let mut rows: Vec<roster::RosterRow> = self
            .orrery
            .graph()
            .nodes()
            .map(|(_key, node)| {
                let url = node.url().to_string();
                let title = if !node.title.is_empty() {
                    node.title.clone()
                } else if let Some(host) = &node.cached_host {
                    host.clone()
                } else {
                    url.clone()
                };
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
                                let other_title = if !other.title.is_empty() {
                                    other.title.clone()
                                } else {
                                    other.cached_host.clone().unwrap_or_else(|| other.url().to_string())
                                };
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
        for field in self.orrery.graph().fields() {
            if !field.is_active() {
                continue;
            }
            let id = field.id;
            let uuid = id.as_uuid().to_string();
            let name = field
                .name
                .clone()
                .unwrap_or_else(|| format!("Field {}", &uuid[..8.min(uuid.len())]));
            out.push(roster::FieldRow { id, name, hidden: !self.orrery.field_visible(id) });
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
                    .and_then(|member| self.orrery.graph().get_node_by_id(member))
                    .map(|(_, node)| node);
                let state = node.and_then(|node| self.shared.content.pages.get(node.url()));
                super::inspector::inspector_rows(node, state)
            }
            PaneContent::Steward => self.steward_rows(),
            _ => Vec::new(),
        }
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
                "Actions".to_string(),
                "retry.focused / stop.focused / pin.focused".to_string(),
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

    fn fetch_state_count(&self, tag: u8) -> usize {
        self.shared.content.pages
            .values()
            .filter(|state| fetch::ContentState::tag(Some(*state)) == tag)
            .count()
    }

    fn sync_summary(&self) -> String {
        let indicator = &self.view.runner.state().sync;
        if indicator.active {
            format!(
                "{} syncing={} ops={}",
                indicator.label, indicator.syncing, indicator.ops
            )
        } else {
            "off".to_string()
        }
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
    }
}
