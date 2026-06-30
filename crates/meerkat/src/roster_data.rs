/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Data builders for the graph-object Roster: node/link/graphlet/field tables
//! plus the selected subject's in-roster detail card.

use forme::{GraphMemberId, GraphletBinding, GraphletId, GraphletKind, GraphletRef};
use kernel::graph::{
    ContainmentSubKind, EdgeFamily, FieldDefinition, FieldExtent, ProvenanceSubKind, RelationKind,
    RelationSelector, SemanticSubKind,
};
use orrery::NodeShape;

use super::node_ops::content_shape;
use super::{WindowCtx, fetch, roster};
use crate::roster_facet_data::{field_facets, link_facets, node_facets};

impl WindowCtx<'_> {
    pub(super) fn roster_snapshot(
        &self,
        subject: Option<&roster::RosterSubject>,
    ) -> roster::RosterSnapshot {
        roster::RosterSnapshot {
            node_rows: self.roster_node_rows(),
            link_rows: self.roster_link_rows(subject),
            graphlet_rows: self.roster_graphlet_rows(subject),
            field_rows: self.roster_field_rows(subject),
            detail: subject.and_then(|s| self.roster_detail(s)),
        }
    }

    pub(super) fn roster_rows(&self) -> Vec<roster::RosterRow> {
        self.roster_node_rows()
    }

    fn roster_node_rows(&self) -> Vec<roster::RosterRow> {
        let selected_members: std::collections::HashSet<GraphMemberId> =
            self.orrery().selected_members().into_iter().collect();
        let open_members: std::collections::HashSet<GraphMemberId> =
            self.view.workbench.open_members().into_iter().collect();
        let graph = self.orrery().graph();
        let mut rows: Vec<roster::RosterRow> = graph
            .nodes()
            .map(|(key, node)| {
                let url = node.url().to_string();
                let content_type = match self.shared.content.pages.get(&url) {
                    Some(fetch::ContentState::Ready(fetched)) => fetched.content_type.clone(),
                    _ => node.mime_hint.clone(),
                };
                let mut tags: Vec<String> = node.tags.iter().cloned().collect();
                tags.sort();
                roster::RosterRow {
                    member: node.id,
                    title: graph.node_display_label(key),
                    url,
                    content_type,
                    tags,
                    selected: selected_members.contains(&node.id),
                    open: open_members.contains(&node.id),
                    section_header: None,
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            let ba = content_bucket(a.content_type.as_deref());
            let bb = content_bucket(b.content_type.as_deref());
            ba.0.cmp(&bb.0)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
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

    fn roster_link_rows(&self, subject: Option<&roster::RosterSubject>) -> Vec<roster::LinkRow> {
        let graph = self.orrery().graph();
        let mut rows: Vec<roster::LinkRow> = graph
            .relations()
            .filter_map(|relation| {
                let source = graph.get_node(relation.from)?;
                let target = graph.get_node(relation.to)?;
                let selector = relation_selector(relation.kind);
                let selected = matches!(
                    subject,
                    Some(roster::RosterSubject::LinkBundle { from, to })
                        if *from == source.id && *to == target.id
                ) || matches!(
                    subject,
                    Some(roster::RosterSubject::RelationCell { from, to, selector: s })
                        if *from == source.id && *to == target.id && *s == selector
                ) || matches!(
                    subject,
                    Some(roster::RosterSubject::Facet(roster::FacetSubject::LinkFamily {
                        from,
                        to,
                        family,
                    })) if *from == source.id
                        && *to == target.id
                        && *family == relation.kind.family()
                );
                Some(roster::LinkRow {
                    from: source.id,
                    to: target.id,
                    source_title: graph.node_display_label(relation.from),
                    source_url: source.url().to_string(),
                    target_title: graph.node_display_label(relation.to),
                    target_url: target.url().to_string(),
                    direction_label: "->".to_string(),
                    family: relation.kind.family(),
                    family_label: edge_family_label(relation.kind.family()).to_string(),
                    kind_label: relation_kind_label(relation.kind).to_string(),
                    source_label: relation_label(graph, relation.from, relation.to),
                    selector,
                    selected,
                    starts_bundle: false,
                })
            })
            .collect();
        rows.sort_by(|a, b| {
            a.source_title
                .to_lowercase()
                .cmp(&b.source_title.to_lowercase())
                .then_with(|| {
                    a.target_title
                        .to_lowercase()
                        .cmp(&b.target_title.to_lowercase())
                })
                .then_with(|| a.family.cmp(&b.family))
                .then_with(|| a.kind_label.cmp(&b.kind_label))
        });
        let mut last: Option<(GraphMemberId, GraphMemberId)> = None;
        for row in &mut rows {
            let bundle = (row.from, row.to);
            row.starts_bundle = last != Some(bundle);
            last = Some(bundle);
        }
        rows
    }

    fn roster_graphlet_rows(
        &self,
        subject: Option<&roster::RosterSubject>,
    ) -> Vec<roster::GraphletRow> {
        let Some(index) = self.graphlets.get(&self.view.focused_graph) else {
            return Vec::new();
        };
        let graph = self.orrery().graph();
        index
            .graphlets()
            .iter()
            .map(|g| {
                let delta = index.preview_reconcile(graph, g.id);
                let drift_label = match delta {
                    Some(delta) => format!("+{} -{}", delta.added.len(), delta.removed.len()),
                    None if matches!(g.binding, GraphletBinding::Linked { .. }) => "clean".to_string(),
                    None => "manual".to_string(),
                };
                roster::GraphletRow {
                    id: g.id,
                    kind_label: graphlet_kind_label(g.kind.as_ref()),
                    binding_label: graphlet_binding_label(&g.binding).to_string(),
                    member_count: g.anchors.len(),
                    selectors_label: graphlet_selectors_label(g),
                    drift_label,
                    selected: matches!(subject, Some(roster::RosterSubject::Graphlet(id)) if *id == g.id),
                }
            })
            .collect()
    }

    fn roster_field_rows(&self, subject: Option<&roster::RosterSubject>) -> Vec<roster::FieldRow> {
        let mut out = Vec::new();
        let selected_field = selected_field_id(subject);
        for field in self.orrery().graph().fields() {
            if !field.is_active() {
                continue;
            }
            let id = field.id;
            let uuid = id.as_uuid().to_string();
            out.push(roster::FieldRow {
                id,
                name: field
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Field {}", &uuid[..8.min(uuid.len())])),
                rule_label: field_definition_label(&field.definition).to_string(),
                extent_label: field_extent_label(&field.extent),
                hidden: !self.orrery().field_visible(id),
                selected: selected_field == Some(id),
                strength: self.orrery().field_strength(id).unwrap_or(0.0),
            });
        }
        out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.as_uuid().cmp(&b.id.as_uuid()))
        });
        if let Some(selected) = selected_field {
            out.sort_by_key(|row| row.id != selected);
        }
        out
    }

    fn roster_detail(&self, subject: &roster::RosterSubject) -> Option<roster::RosterDetail> {
        match subject {
            roster::RosterSubject::Node(member) => {
                self.node_detail(*member).map(roster::RosterDetail::Node)
            }
            roster::RosterSubject::LinkBundle { from, to } => self
                .link_card(*from, *to, None)
                .map(roster::RosterDetail::Link),
            roster::RosterSubject::RelationCell { from, to, selector } => self
                .link_card(*from, *to, Some(*selector))
                .map(roster::RosterDetail::Link),
            roster::RosterSubject::Graphlet(id) => {
                self.graphlet_card(*id).map(roster::RosterDetail::Graphlet)
            }
            roster::RosterSubject::Field(id) => {
                self.field_detail(*id).map(roster::RosterDetail::Field)
            }
            roster::RosterSubject::Facet(facet) => {
                self.facet_card(facet).map(roster::RosterDetail::Facet)
            }
        }
    }

    pub(super) fn node_detail(&self, member: GraphMemberId) -> Option<roster::NodeDetail> {
        let graph = self.orrery().graph();
        let (key, node) = graph.get_node_by_id(member)?;
        let url = node.url().to_string();
        let content_type = match self.shared.content.pages.get(&url) {
            Some(fetch::ContentState::Ready(fetched)) => fetched.content_type.clone(),
            _ => node.mime_hint.clone(),
        };
        let mut tags: Vec<String> = node.tags.iter().cloned().collect();
        tags.sort();
        let tag_count = tags.len();
        let relation_count = graph
            .relations()
            .filter(|r| r.from == key || r.to == key)
            .count();
        let field_count = self.attached_field_names(member).len();
        Some(roster::NodeDetail {
            member,
            title: graph.node_display_label(key),
            url,
            content_type: content_type.clone(),
            tags,
            relation_count,
            open: self.view.workbench.open_members().contains(&member),
            facets: node_facets(
                member,
                content_type.as_deref(),
                tag_count,
                relation_count,
                field_count,
            ),
        })
    }

    pub(super) fn link_card(
        &self,
        from: GraphMemberId,
        to: GraphMemberId,
        selected_selector: Option<RelationSelector>,
    ) -> Option<roster::LinkCard> {
        let graph = self.orrery().graph();
        let (from_key, source) = graph.get_node_by_id(from)?;
        let (to_key, target) = graph.get_node_by_id(to)?;
        let mut relations: Vec<roster::LinkRelationRow> = graph
            .relations()
            .filter(|r| r.from == from_key && r.to == to_key)
            .map(|r| {
                let selector = relation_selector(r.kind);
                roster::LinkRelationRow {
                    from,
                    to,
                    family: r.kind.family(),
                    family_label: edge_family_label(r.kind.family()).to_string(),
                    kind_label: relation_kind_label(r.kind).to_string(),
                    label: relation_label(graph, r.from, r.to),
                    selector,
                    editable: matches!(selector, RelationSelector::Semantic(_)),
                    selected: selected_selector == Some(selector),
                    hidden: self
                        .orrery()
                        .relation_between_members_hidden(from, to, selector),
                }
            })
            .collect();
        relations.sort_by(|a, b| {
            a.family
                .cmp(&b.family)
                .then_with(|| a.kind_label.cmp(&b.kind_label))
        });
        let facets = link_facets(from, to, &relations);
        Some(roster::LinkCard {
            from,
            to,
            source_title: graph.node_display_label(from_key),
            source_url: source.url().to_string(),
            target_title: graph.node_display_label(to_key),
            target_url: target.url().to_string(),
            hidden: self.orrery().edge_between_members_hidden(from, to),
            relations,
            facets,
        })
    }

    fn graphlet_card(&self, id: GraphletId) -> Option<roster::GraphletCard> {
        let graph = self.orrery().graph();
        let index = self.graphlets.get(&self.view.focused_graph)?;
        let graphlet = index.get(id)?;
        let delta = index.preview_reconcile(graph, id);
        let drift_tracking = matches!(graphlet.binding, GraphletBinding::Linked { .. });
        let drift_summary = match delta.as_ref() {
            Some(delta) => format!(
                "drift proposal: +{} -{}",
                delta.added.len(),
                delta.removed.len()
            ),
            None if drift_tracking => "drift proposal: clean".to_string(),
            None => "drift proposal: not tracked".to_string(),
        };
        Some(roster::GraphletCard {
            id,
            kind_label: graphlet_kind_label(graphlet.kind.as_ref()),
            binding_label: graphlet_binding_label(&graphlet.binding).to_string(),
            members: member_labels(graph, &graphlet.anchors),
            selectors_label: graphlet_selectors_label(graphlet),
            drift_tracking,
            drift_summary,
            added: delta
                .as_ref()
                .map(|d| member_labels(graph, &d.added))
                .unwrap_or_default(),
            removed: delta
                .as_ref()
                .map(|d| member_labels(graph, &d.removed))
                .unwrap_or_default(),
        })
    }

    pub(super) fn field_detail(&self, id: kernel::graph::FieldId) -> Option<roster::FieldDetail> {
        let field = self
            .orrery()
            .graph()
            .fields()
            .find(|field| field.id == id)?;
        let uuid = id.as_uuid().to_string();
        Some(roster::FieldDetail {
            id,
            name: field
                .name
                .clone()
                .unwrap_or_else(|| format!("Field {}", &uuid[..8.min(uuid.len())])),
            rule_label: field_definition_label(&field.definition).to_string(),
            extent_label: field_extent_label(&field.extent),
            hidden: !self.orrery().field_visible(id),
            strength: self.orrery().field_strength(id).unwrap_or(0.0),
            facets: field_facets(id),
        })
    }
}

fn selected_field_id(subject: Option<&roster::RosterSubject>) -> Option<kernel::graph::FieldId> {
    match subject {
        Some(roster::RosterSubject::Field(id)) => Some(*id),
        Some(roster::RosterSubject::Facet(
            roster::FacetSubject::FieldRule(id)
            | roster::FacetSubject::FieldExtent(id)
            | roster::FacetSubject::FieldVisibility(id)
            | roster::FacetSubject::FieldStrength(id),
        )) => Some(*id),
        _ => None,
    }
}

pub(super) fn content_bucket(content_type: Option<&str>) -> (u8, &'static str) {
    match content_type {
        None => (3, "Unknown"),
        Some(ct) => match content_shape(Some(ct)) {
            NodeShape::Circle => (1, "Feeds"),
            NodeShape::Rounded => (2, "Menus"),
            NodeShape::Square => (0, "Documents"),
        },
    }
}

fn relation_label(
    graph: &kernel::graph::Graph,
    from: kernel::graph::NodeKey,
    to: kernel::graph::NodeKey,
) -> Option<String> {
    graph
        .find_edge_key(from, to)
        .and_then(|key| graph.get_edge(key))
        .and_then(|payload| payload.label().map(str::to_string))
}

fn relation_selector(kind: RelationKind) -> RelationSelector {
    match kind {
        RelationKind::Semantic(sub) => RelationSelector::Semantic(sub),
        RelationKind::Traversal => RelationSelector::Family(EdgeFamily::Traversal),
        RelationKind::Containment(sub) => RelationSelector::Containment(sub),
        RelationKind::Arrangement(sub) => RelationSelector::Arrangement(sub),
        RelationKind::Imported(sub) => RelationSelector::Imported(sub),
        RelationKind::Provenance(sub) => RelationSelector::Provenance(sub),
    }
}

pub(super) fn edge_family_label(family: EdgeFamily) -> &'static str {
    match family {
        EdgeFamily::Semantic => "Semantic",
        EdgeFamily::Traversal => "Traversal",
        EdgeFamily::Containment => "Containment",
        EdgeFamily::Arrangement => "Arrangement",
        EdgeFamily::Imported => "Imported",
        EdgeFamily::Provenance => "Provenance",
    }
}

fn graphlet_kind_label(kind: Option<&GraphletKind>) -> String {
    match kind {
        Some(GraphletKind::Ego { radius }) => format!("Ego r{radius}"),
        Some(GraphletKind::Corridor) => "Corridor".to_string(),
        Some(GraphletKind::Component) => "Component".to_string(),
        Some(GraphletKind::Loop) => "Loop".to_string(),
        Some(GraphletKind::Frontier) => "Frontier".to_string(),
        Some(GraphletKind::Facet) => "Facet".to_string(),
        Some(GraphletKind::Session) => "Session".to_string(),
        Some(GraphletKind::Bridge) => "Bridge".to_string(),
        Some(GraphletKind::WorkbenchCorrespondence) => "Workbench".to_string(),
        None => "Graphlet".to_string(),
    }
}

fn graphlet_binding_label(binding: &GraphletBinding) -> &'static str {
    match binding {
        GraphletBinding::UnlinkedSession => "Session",
        GraphletBinding::Linked { .. } => "Linked",
        GraphletBinding::Branched { .. } => "Branched",
    }
}

fn graphlet_selectors_label(graphlet: &GraphletRef<GraphMemberId>) -> String {
    let selectors = match &graphlet.binding {
        GraphletBinding::Linked { spec } => &spec.selectors,
        GraphletBinding::Branched { parent_spec, .. } => &parent_spec.selectors,
        GraphletBinding::UnlinkedSession => return "all relations".to_string(),
    };
    if selectors.is_empty() {
        "all relations".to_string()
    } else {
        selectors.join(", ")
    }
}

fn field_definition_label(definition: &FieldDefinition) -> &'static str {
    match definition {
        FieldDefinition::Scalar(_) => "Scalar",
        FieldDefinition::Vector(_) => "Vector",
    }
}

fn field_extent_label(extent: &FieldExtent) -> String {
    match extent {
        FieldExtent::Global => "Global".to_string(),
        FieldExtent::Region {
            min_x,
            min_y,
            max_x,
            max_y,
        } => format!(
            "Region {:.0},{:.0} - {:.0},{:.0}",
            min_x, min_y, max_x, max_y
        ),
        FieldExtent::AttachedToNode(id) => format!("Attached {}", short_id(*id)),
    }
}

fn member_labels(graph: &kernel::graph::Graph, members: &[GraphMemberId]) -> Vec<String> {
    members
        .iter()
        .map(|member| {
            graph
                .get_node_by_id(*member)
                .map(|(key, _)| graph.node_display_label(key))
                .unwrap_or_else(|| short_id(*member))
        })
        .collect()
}

fn short_id(id: uuid::Uuid) -> String {
    id.to_string().chars().take(8).collect()
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
