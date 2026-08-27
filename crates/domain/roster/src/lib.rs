// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Neutral roster snapshot vocabulary and pure helpers, shared by host data
//! builders and views.

use std::collections::{BTreeMap, BTreeSet};

use forme::{GraphMemberId, GraphletBinding, GraphletId, GraphletKind, GraphletRef};
use kernel::graph::{
    ContainmentSubKind, EdgeFamily, FieldDefinition, FieldExtent, FieldId, Graph, NodeKey,
    ProvenanceSubKind, RelationKind, RelationSelector, SemanticSubKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RosterTab {
    Nodes,
    Links,
    Graphlets,
    Fields,
}

impl RosterTab {
    pub const ALL: [RosterTab; 4] = [
        RosterTab::Nodes,
        RosterTab::Links,
        RosterTab::Graphlets,
        RosterTab::Fields,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RosterTab::Nodes => "Nodes",
            RosterTab::Links => "Links",
            RosterTab::Graphlets => "Graphlets",
            RosterTab::Fields => "Fields",
        }
    }

    pub fn empty_label(self) -> &'static str {
        match self {
            RosterTab::Nodes => "No nodes yet",
            RosterTab::Links => "No relations yet",
            RosterTab::Graphlets => "No graphlets yet",
            RosterTab::Fields => "No fields yet",
        }
    }
}

impl Default for RosterTab {
    fn default() -> Self {
        Self::Nodes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RosterSubject {
    Node(GraphMemberId),
    LinkBundle {
        from: GraphMemberId,
        to: GraphMemberId,
    },
    RelationCell {
        from: GraphMemberId,
        to: GraphMemberId,
        selector: RelationSelector,
    },
    Graphlet(GraphletId),
    Field(FieldId),
    Facet(FacetSubject),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FacetSubject {
    NodeContent(GraphMemberId),
    NodeTags(GraphMemberId),
    NodeRelations(GraphMemberId),
    NodeFields(GraphMemberId),
    LinkFamily {
        from: GraphMemberId,
        to: GraphMemberId,
        family: EdgeFamily,
    },
    FieldRule(FieldId),
    FieldExtent(FieldId),
    FieldVisibility(FieldId),
    FieldStrength(FieldId),
}

impl RosterSubject {
    pub fn natural_tab(&self) -> RosterTab {
        match self {
            RosterSubject::Node(_) => RosterTab::Nodes,
            RosterSubject::LinkBundle { .. } | RosterSubject::RelationCell { .. } => {
                RosterTab::Links
            }
            RosterSubject::Graphlet(_) => RosterTab::Graphlets,
            RosterSubject::Field(_) => RosterTab::Fields,
            RosterSubject::Facet(facet) => facet.natural_tab(),
        }
    }
}

impl FacetSubject {
    pub fn natural_tab(&self) -> RosterTab {
        match self {
            FacetSubject::NodeContent(_)
            | FacetSubject::NodeTags(_)
            | FacetSubject::NodeRelations(_)
            | FacetSubject::NodeFields(_) => RosterTab::Nodes,
            FacetSubject::LinkFamily { .. } => RosterTab::Links,
            FacetSubject::FieldRule(_)
            | FacetSubject::FieldExtent(_)
            | FacetSubject::FieldVisibility(_)
            | FacetSubject::FieldStrength(_) => RosterTab::Fields,
        }
    }
}

pub const RELATE_PICKER_KINDS: &[(SemanticSubKind, &str)] = &[
    (SemanticSubKind::Cites, "Cites"),
    (SemanticSubKind::Quotes, "Quotes"),
    (SemanticSubKind::Summarizes, "Summarizes"),
    (SemanticSubKind::Elaborates, "Elaborates"),
    (SemanticSubKind::ExampleOf, "Example of"),
    (SemanticSubKind::Supports, "Supports"),
    (SemanticSubKind::Contradicts, "Contradicts"),
    (SemanticSubKind::Questions, "Questions"),
    (SemanticSubKind::SameEntityAs, "Same entity as"),
    (SemanticSubKind::DuplicateOf, "Duplicate of"),
    (SemanticSubKind::Hyperlink, "Hyperlink"),
];

#[derive(Clone)]
pub struct RosterRow {
    pub member: GraphMemberId,
    pub title: String,
    pub url: String,
    pub content_type: Option<String>,
    pub tags: Vec<String>,
    pub selected: bool,
    pub open: bool,
    pub section_header: Option<String>,
}

#[derive(Clone)]
pub struct LinkRow {
    pub from: GraphMemberId,
    pub to: GraphMemberId,
    pub source_title: String,
    pub source_url: String,
    pub target_title: String,
    pub target_url: String,
    pub direction_label: String,
    pub family: EdgeFamily,
    pub family_label: String,
    pub kind_label: String,
    pub source_label: Option<String>,
    pub selector: RelationSelector,
    pub selected: bool,
    pub starts_bundle: bool,
}

#[derive(Clone)]
pub struct GraphletRow {
    pub id: GraphletId,
    pub kind_label: String,
    pub binding_label: String,
    pub member_count: usize,
    pub selectors_label: String,
    pub drift_label: String,
    pub selected: bool,
}

#[derive(Clone)]
pub struct FieldRow {
    pub id: FieldId,
    pub name: String,
    pub rule_label: String,
    pub extent_label: String,
    pub hidden: bool,
    pub selected: bool,
    pub strength: f32,
}

#[derive(Clone)]
pub struct NodeDetail {
    pub member: GraphMemberId,
    pub title: String,
    pub url: String,
    pub content_type: Option<String>,
    pub tags: Vec<String>,
    pub relation_count: usize,
    pub open: bool,
    pub facets: Vec<FacetEntry>,
}

#[derive(Clone)]
pub struct LinkRelationRow {
    pub from: GraphMemberId,
    pub to: GraphMemberId,
    pub family: EdgeFamily,
    pub family_label: String,
    pub kind_label: String,
    pub label: Option<String>,
    pub selector: RelationSelector,
    pub editable: bool,
    pub selected: bool,
    pub hidden: bool,
}

#[derive(Clone)]
pub struct LinkCard {
    pub from: GraphMemberId,
    pub to: GraphMemberId,
    pub source_title: String,
    pub source_url: String,
    pub target_title: String,
    pub target_url: String,
    pub hidden: bool,
    pub relations: Vec<LinkRelationRow>,
    pub facets: Vec<FacetEntry>,
}

#[derive(Clone)]
pub struct GraphletCard {
    pub id: GraphletId,
    pub kind_label: String,
    pub binding_label: String,
    pub members: Vec<String>,
    pub selectors_label: String,
    pub family_selectors: Option<Vec<(EdgeFamily, bool)>>,
    pub drift_tracking: bool,
    pub drift_summary: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Clone)]
pub struct FieldDetail {
    pub id: FieldId,
    pub name: String,
    pub rule_label: String,
    pub extent_label: String,
    pub hidden: bool,
    pub strength: f32,
    pub facets: Vec<FacetEntry>,
}

#[derive(Clone)]
pub struct FacetEntry {
    pub label: String,
    pub value: String,
    pub subject: RosterSubject,
}

#[derive(Clone)]
pub struct FacetInfoRow {
    pub label: String,
    pub value: String,
}

#[derive(Clone)]
pub struct FacetAction {
    pub label: String,
    pub intent: FacetActionIntent,
}

#[derive(Clone)]
pub enum FacetActionIntent {
    SelectNode(GraphMemberId),
    SelectField(FieldId),
    ToggleFieldVisibility(FieldId),
    AdjustFieldStrength(FieldId, f32),
    OpenLinkBundle {
        from: GraphMemberId,
        to: GraphMemberId,
    },
}

#[derive(Clone)]
pub struct FacetCard {
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<FacetInfoRow>,
    pub actions: Vec<FacetAction>,
}

#[derive(Clone)]
pub enum RosterDetail {
    Node(NodeDetail),
    Link(LinkCard),
    Graphlet(GraphletCard),
    Field(FieldDetail),
    Facet(FacetCard),
}

#[derive(Clone, Default)]
pub struct RosterSnapshot {
    pub node_rows: Vec<RosterRow>,
    pub link_rows: Vec<LinkRow>,
    pub graphlet_rows: Vec<GraphletRow>,
    pub field_rows: Vec<FieldRow>,
    pub detail: Option<RosterDetail>,
}

#[derive(Clone)]
pub struct GraphletRowInput {
    pub id: GraphletId,
    pub kind: Option<GraphletKind>,
    pub binding: GraphletBinding,
    pub member_count: usize,
    pub added_count: usize,
    pub removed_count: usize,
    pub selected: bool,
}

#[derive(Clone)]
pub struct GraphletCardInput {
    pub id: GraphletId,
    pub kind: Option<GraphletKind>,
    pub binding: GraphletBinding,
    pub members: Vec<String>,
    pub family_selectors: Option<Vec<(EdgeFamily, bool)>>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Clone)]
pub struct NodeRowInput {
    pub member: GraphMemberId,
    pub title: String,
    pub url: String,
    pub content_type: Option<String>,
    pub tags: Vec<String>,
    pub selected: bool,
    pub open: bool,
}

#[derive(Clone)]
pub struct LinkRowInput {
    pub from: GraphMemberId,
    pub to: GraphMemberId,
    pub source_title: String,
    pub source_url: String,
    pub target_title: String,
    pub target_url: String,
    pub kind: RelationKind,
    pub source_label: Option<String>,
    pub selected: bool,
}

#[derive(Clone)]
pub struct FieldRowInput {
    pub id: FieldId,
    pub name: Option<String>,
    pub definition: FieldDefinition,
    pub extent: FieldExtent,
    pub hidden: bool,
    pub selected: bool,
    pub strength: f32,
}

#[derive(Clone)]
pub struct NodeDetailInput {
    pub member: GraphMemberId,
    pub title: String,
    pub url: String,
    pub content_type: Option<String>,
    pub tags: Vec<String>,
    pub relation_count: usize,
    pub field_count: usize,
    pub open: bool,
}

#[derive(Clone)]
pub struct LinkRelationInput {
    pub from: GraphMemberId,
    pub to: GraphMemberId,
    pub kind: RelationKind,
    pub label: Option<String>,
    pub selected: bool,
    pub hidden: bool,
}

#[derive(Clone)]
pub struct LinkCardInput {
    pub from: GraphMemberId,
    pub to: GraphMemberId,
    pub source_title: String,
    pub source_url: String,
    pub target_title: String,
    pub target_url: String,
    pub hidden: bool,
    pub relations: Vec<LinkRelationInput>,
}

#[derive(Clone)]
pub struct FieldDetailInput {
    pub id: FieldId,
    pub name: Option<String>,
    pub definition: FieldDefinition,
    pub extent: FieldExtent,
    pub hidden: bool,
    pub strength: f32,
}

#[derive(Clone)]
pub struct NodeRelationsFacetInput {
    pub member: GraphMemberId,
    pub title: String,
    pub counts_by_family: Vec<(EdgeFamily, usize)>,
}

pub fn build_node_rows(inputs: Vec<NodeRowInput>) -> Vec<RosterRow> {
    let mut rows: Vec<RosterRow> = inputs
        .into_iter()
        .map(|input| RosterRow {
            member: input.member,
            title: input.title,
            url: input.url,
            content_type: input.content_type,
            tags: input.tags,
            selected: input.selected,
            open: input.open,
            section_header: None,
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

pub fn build_link_rows(inputs: Vec<LinkRowInput>) -> Vec<LinkRow> {
    let mut rows: Vec<LinkRow> = inputs
        .into_iter()
        .map(|input| {
            let family = input.kind.family();
            let selector = relation_selector(input.kind);
            LinkRow {
                from: input.from,
                to: input.to,
                source_title: input.source_title,
                source_url: input.source_url,
                target_title: input.target_title,
                target_url: input.target_url,
                direction_label: "->".to_string(),
                family,
                family_label: edge_family_label(family).to_string(),
                kind_label: relation_kind_label(input.kind).to_string(),
                source_label: input.source_label,
                selector,
                selected: input.selected,
                starts_bundle: false,
            }
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

pub fn build_graphlet_rows(inputs: Vec<GraphletRowInput>) -> Vec<GraphletRow> {
    inputs
        .into_iter()
        .map(|input| GraphletRow {
            id: input.id,
            kind_label: graphlet_kind_label(input.kind.as_ref()),
            binding_label: graphlet_binding_label(&input.binding).to_string(),
            member_count: input.member_count,
            selectors_label: graphlet_binding_selectors_label(&input.binding),
            drift_label: graphlet_drift_label(
                &input.binding,
                input.added_count,
                input.removed_count,
            ),
            selected: input.selected,
        })
        .collect()
}

pub fn build_field_rows(inputs: Vec<FieldRowInput>) -> Vec<FieldRow> {
    let mut rows: Vec<FieldRow> = inputs
        .into_iter()
        .map(|input| FieldRow {
            id: input.id,
            name: display_field_name(input.name.as_deref(), input.id),
            rule_label: field_definition_label(&input.definition).to_string(),
            extent_label: field_extent_label(&input.extent),
            hidden: input.hidden,
            selected: input.selected,
            strength: input.strength,
        })
        .collect();
    rows.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.as_uuid().cmp(&b.id.as_uuid()))
    });
    if rows.iter().any(|row| row.selected) {
        rows.sort_by_key(|row| !row.selected);
    }
    rows
}

pub fn build_node_detail(input: NodeDetailInput) -> NodeDetail {
    let tag_count = input.tags.len();
    NodeDetail {
        member: input.member,
        title: input.title,
        url: input.url,
        content_type: input.content_type.clone(),
        tags: input.tags,
        relation_count: input.relation_count,
        open: input.open,
        facets: node_facets(
            input.member,
            input.content_type.as_deref(),
            tag_count,
            input.relation_count,
            input.field_count,
        ),
    }
}

pub fn build_link_card(input: LinkCardInput) -> LinkCard {
    let mut relations: Vec<LinkRelationRow> = input
        .relations
        .into_iter()
        .map(|input| {
            let family = input.kind.family();
            let selector = relation_selector(input.kind);
            let editable = matches!(selector, RelationSelector::Semantic(_));
            LinkRelationRow {
                from: input.from,
                to: input.to,
                family,
                family_label: edge_family_label(family).to_string(),
                kind_label: relation_kind_label(input.kind).to_string(),
                label: input.label,
                selector,
                editable,
                selected: input.selected,
                hidden: input.hidden,
            }
        })
        .collect();
    relations.sort_by(|a, b| {
        a.family
            .cmp(&b.family)
            .then_with(|| a.kind_label.cmp(&b.kind_label))
    });
    let facets = link_facets(input.from, input.to, &relations);
    LinkCard {
        from: input.from,
        to: input.to,
        source_title: input.source_title,
        source_url: input.source_url,
        target_title: input.target_title,
        target_url: input.target_url,
        hidden: input.hidden,
        relations,
        facets,
    }
}

pub fn build_graphlet_card(input: GraphletCardInput) -> GraphletCard {
    let drift_tracking = matches!(input.binding, GraphletBinding::Linked { .. });
    let drift_summary = graphlet_drift_summary(&input.binding, &input.added, &input.removed);
    GraphletCard {
        id: input.id,
        kind_label: graphlet_kind_label(input.kind.as_ref()),
        binding_label: graphlet_binding_label(&input.binding).to_string(),
        members: input.members,
        selectors_label: graphlet_binding_selectors_label(&input.binding),
        family_selectors: input.family_selectors,
        drift_tracking,
        drift_summary,
        added: input.added,
        removed: input.removed,
    }
}

pub fn build_field_detail(input: FieldDetailInput) -> FieldDetail {
    FieldDetail {
        id: input.id,
        name: display_field_name(input.name.as_deref(), input.id),
        rule_label: field_definition_label(&input.definition).to_string(),
        extent_label: field_extent_label(&input.extent),
        hidden: input.hidden,
        strength: input.strength,
        facets: field_facets(input.id),
    }
}

pub fn selected_field_id(subject: Option<&RosterSubject>) -> Option<FieldId> {
    match subject {
        Some(RosterSubject::Field(id)) => Some(*id),
        Some(RosterSubject::Facet(
            FacetSubject::FieldRule(id)
            | FacetSubject::FieldExtent(id)
            | FacetSubject::FieldVisibility(id)
            | FacetSubject::FieldStrength(id),
        )) => Some(*id),
        _ => None,
    }
}

pub fn content_bucket(content_type: Option<&str>) -> (u8, &'static str) {
    let Some(content_type) = content_type else {
        return (3, "Unknown");
    };
    let base = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "application/rss+xml" | "application/atom+xml" | "application/feed+json" => (1, "Feeds"),
        "application/gopher-menu"
        | "application/x-nex"
        | "application/x-guppy"
        | "text/x-finger" => (2, "Menus"),
        _ => (0, "Documents"),
    }
}

pub fn relation_label(graph: &Graph, from: NodeKey, to: NodeKey) -> Option<String> {
    graph
        .find_edge_key(from, to)
        .and_then(|key| graph.get_edge(key))
        .and_then(|payload| payload.label().map(str::to_string))
}

pub fn relation_selector(kind: RelationKind) -> RelationSelector {
    match kind {
        RelationKind::Semantic(sub) => RelationSelector::Semantic(sub),
        RelationKind::Traversal => RelationSelector::Family(EdgeFamily::Traversal),
        RelationKind::Containment(sub) => RelationSelector::Containment(sub),
        RelationKind::Arrangement(sub) => RelationSelector::Arrangement(sub),
        RelationKind::Imported(sub) => RelationSelector::Imported(sub),
        RelationKind::Provenance(sub) => RelationSelector::Provenance(sub),
    }
}

pub fn edge_family_label(family: EdgeFamily) -> &'static str {
    match family {
        EdgeFamily::Semantic => "Semantic",
        EdgeFamily::Traversal => "Traversal",
        EdgeFamily::Containment => "Containment",
        EdgeFamily::Arrangement => "Arrangement",
        EdgeFamily::Imported => "Imported",
        EdgeFamily::Provenance => "Provenance",
    }
}

pub fn graphlet_kind_label(kind: Option<&GraphletKind>) -> String {
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

pub fn graphlet_binding_label(binding: &GraphletBinding) -> &'static str {
    match binding {
        GraphletBinding::UnlinkedSession => "Session",
        GraphletBinding::Linked { .. } => "Linked",
        GraphletBinding::Branched { .. } => "Branched",
    }
}

pub fn graphlet_selectors_label(graphlet: &GraphletRef<GraphMemberId>) -> String {
    graphlet_binding_selectors_label(&graphlet.binding)
}

pub fn graphlet_binding_selectors_label(binding: &GraphletBinding) -> String {
    let selectors = match binding {
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

pub fn field_definition_label(definition: &FieldDefinition) -> &'static str {
    match definition {
        FieldDefinition::Scalar(_) => "Scalar",
        FieldDefinition::Vector(_) => "Vector",
    }
}

pub fn field_extent_label(extent: &FieldExtent) -> String {
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
        FieldExtent::Polygon { points } => format!("Polygon ({} pts)", points.len()),
    }
}

pub fn member_labels(graph: &Graph, members: &[GraphMemberId]) -> Vec<String> {
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

pub fn short_id(id: impl ToString) -> String {
    id.to_string().chars().take(8).collect()
}

pub fn display_field_name(name: Option<&str>, id: FieldId) -> String {
    match name {
        Some(name) => name.to_string(),
        None => format!("Field {}", short_id(id.as_uuid())),
    }
}

pub fn relation_kind_label(kind: RelationKind) -> &'static str {
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

pub fn node_facets(
    member: GraphMemberId,
    content_type: Option<&str>,
    tag_count: usize,
    relation_count: usize,
    field_count: usize,
) -> Vec<FacetEntry> {
    vec![
        facet_entry(
            "Content",
            content_type.unwrap_or("unknown"),
            FacetSubject::NodeContent(member),
        ),
        facet_entry(
            "Tags",
            tag_count.to_string(),
            FacetSubject::NodeTags(member),
        ),
        facet_entry(
            "Relations",
            relation_count.to_string(),
            FacetSubject::NodeRelations(member),
        ),
        facet_entry(
            "Fields",
            field_count.to_string(),
            FacetSubject::NodeFields(member),
        ),
    ]
}

pub fn link_facets(
    from: GraphMemberId,
    to: GraphMemberId,
    relations: &[LinkRelationRow],
) -> Vec<FacetEntry> {
    let mut by_family: BTreeMap<EdgeFamily, (usize, BTreeSet<String>)> = BTreeMap::new();
    for rel in relations {
        let (count, kinds) = by_family.entry(rel.family).or_default();
        *count += 1;
        kinds.insert(rel.kind_label.clone());
    }
    by_family
        .into_iter()
        .map(|(family, (count, kinds))| {
            let kinds = kinds.into_iter().collect::<Vec<_>>().join(", ");
            facet_entry(
                edge_family_label(family),
                format!("{count}: {kinds}"),
                FacetSubject::LinkFamily { from, to, family },
            )
        })
        .collect()
}

pub fn field_facets(id: FieldId) -> Vec<FacetEntry> {
    vec![
        facet_entry("Rule", "inspect", FacetSubject::FieldRule(id)),
        facet_entry("Extent", "inspect", FacetSubject::FieldExtent(id)),
        facet_entry("Visibility", "toggle", FacetSubject::FieldVisibility(id)),
        facet_entry("Strength", "tune", FacetSubject::FieldStrength(id)),
    ]
}

pub fn build_node_content_facet_card(detail: &NodeDetail) -> FacetCard {
    let content = detail.content_type.as_deref().unwrap_or("unknown");
    let bucket = detail
        .content_type
        .as_deref()
        .map(|ct| content_bucket(Some(ct)).1)
        .unwrap_or("Unknown");
    facet_card(
        "Content",
        detail.title.clone(),
        vec![
            info("content type", content),
            info("bucket", bucket),
            info("url", detail.url.clone()),
        ],
        vec![select_node_action(detail.member)],
    )
}

pub fn build_node_tags_facet_card(detail: &NodeDetail) -> FacetCard {
    facet_card(
        "Tags",
        detail.title.clone(),
        vec![
            info("count", detail.tags.len().to_string()),
            info("tags", nonempty_join(&detail.tags)),
        ],
        vec![select_node_action(detail.member)],
    )
}

pub fn build_node_relations_facet_card(input: NodeRelationsFacetInput) -> FacetCard {
    let mut counts = input.counts_by_family;
    counts.sort_by_key(|(family, _)| *family);
    let mut rows = vec![info(
        "total",
        counts
            .iter()
            .map(|(_, count)| *count)
            .sum::<usize>()
            .to_string(),
    )];
    rows.extend(
        counts
            .into_iter()
            .map(|(family, count)| info(edge_family_label(family), count.to_string())),
    );
    facet_card(
        "Relations",
        input.title,
        rows,
        vec![select_node_action(input.member)],
    )
}

pub fn build_node_fields_facet_card(detail: &NodeDetail, fields: &[String]) -> FacetCard {
    facet_card(
        "Fields",
        detail.title.clone(),
        vec![
            info("attached", fields.len().to_string()),
            info("fields", nonempty_join(fields)),
        ],
        vec![select_node_action(detail.member)],
    )
}

pub fn build_link_family_facet_card(link: &LinkCard, family: EdgeFamily) -> FacetCard {
    let mut rows = Vec::new();
    for rel in link.relations.iter().filter(|rel| rel.family == family) {
        rows.push(info(
            &rel.kind_label,
            rel.label.as_deref().unwrap_or("relation cell"),
        ));
    }
    if rows.is_empty() {
        rows.push(info("relations", "none"));
    }
    facet_card(
        edge_family_label(family),
        format!("{} -> {}", link.source_title, link.target_title),
        rows,
        vec![FacetAction {
            label: "open link".to_string(),
            intent: FacetActionIntent::OpenLinkBundle {
                from: link.from,
                to: link.to,
            },
        }],
    )
}

pub fn build_field_rule_facet_card(detail: &FieldDetail) -> FacetCard {
    facet_card(
        "Field rule",
        detail.name.clone(),
        vec![
            info("rule", detail.rule_label.clone()),
            info("script", "not configured"),
            info("template", "not configured"),
        ],
        vec![select_field_action(detail.id)],
    )
}

pub fn build_field_extent_facet_card(detail: &FieldDetail) -> FacetCard {
    facet_card(
        "Field extent",
        detail.name.clone(),
        vec![info("extent", detail.extent_label.clone())],
        vec![select_field_action(detail.id)],
    )
}

pub fn build_field_visibility_facet_card(detail: &FieldDetail) -> FacetCard {
    facet_card(
        "Field visibility",
        detail.name.clone(),
        vec![info(
            "visibility",
            if detail.hidden { "hidden" } else { "visible" },
        )],
        vec![
            select_field_action(detail.id),
            FacetAction {
                label: if detail.hidden { "show" } else { "hide" }.to_string(),
                intent: FacetActionIntent::ToggleFieldVisibility(detail.id),
            },
        ],
    )
}

pub fn build_field_strength_facet_card(detail: &FieldDetail) -> FacetCard {
    facet_card(
        "Field strength",
        detail.name.clone(),
        vec![info("strength", format!("{:.0}", detail.strength / 1000.0))],
        vec![
            FacetAction {
                label: "weaker".to_string(),
                intent: FacetActionIntent::AdjustFieldStrength(detail.id, -1000.0),
            },
            FacetAction {
                label: "stronger".to_string(),
                intent: FacetActionIntent::AdjustFieldStrength(detail.id, 1000.0),
            },
        ],
    )
}

pub fn facet_card(
    title: impl Into<String>,
    subtitle: impl Into<String>,
    rows: Vec<FacetInfoRow>,
    actions: Vec<FacetAction>,
) -> FacetCard {
    FacetCard {
        title: title.into(),
        subtitle: subtitle.into(),
        rows,
        actions,
    }
}

pub fn info(label: impl Into<String>, value: impl Into<String>) -> FacetInfoRow {
    FacetInfoRow {
        label: label.into(),
        value: value.into(),
    }
}

pub fn select_node_action(member: GraphMemberId) -> FacetAction {
    FacetAction {
        label: "select node".to_string(),
        intent: FacetActionIntent::SelectNode(member),
    }
}

pub fn select_field_action(id: FieldId) -> FacetAction {
    FacetAction {
        label: "select field".to_string(),
        intent: FacetActionIntent::SelectField(id),
    }
}

pub fn nonempty_join(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn facet_entry(
    label: impl Into<String>,
    value: impl Into<String>,
    subject: FacetSubject,
) -> FacetEntry {
    FacetEntry {
        label: label.into(),
        value: value.into(),
        subject: RosterSubject::Facet(subject),
    }
}

fn graphlet_drift_label(
    binding: &GraphletBinding,
    added_count: usize,
    removed_count: usize,
) -> String {
    if added_count > 0 || removed_count > 0 {
        format!("+{added_count} -{removed_count}")
    } else if matches!(binding, GraphletBinding::Linked { .. }) {
        "clean".to_string()
    } else {
        "manual".to_string()
    }
}

fn graphlet_drift_summary(
    binding: &GraphletBinding,
    added: &[String],
    removed: &[String],
) -> String {
    if !added.is_empty() || !removed.is_empty() {
        format!("drift proposal: +{} -{}", added.len(), removed.len())
    } else if matches!(binding, GraphletBinding::Linked { .. }) {
        "drift proposal: clean".to_string()
    } else {
        "drift proposal: not tracked".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_bucket_groups_known_content_shapes() {
        assert_eq!(content_bucket(None), (3, "Unknown"));
        assert_eq!(content_bucket(Some("application/rss+xml")), (1, "Feeds"));
        assert_eq!(
            content_bucket(Some("application/gopher-menu")),
            (2, "Menus")
        );
        assert_eq!(
            content_bucket(Some("text/html; charset=utf-8")),
            (0, "Documents")
        );
    }

    #[test]
    fn selected_field_id_reads_field_subjects_and_field_facets() {
        let field = FieldId::new();
        assert_eq!(
            selected_field_id(Some(&RosterSubject::Field(field))),
            Some(field)
        );
        assert_eq!(
            selected_field_id(Some(&RosterSubject::Facet(FacetSubject::FieldStrength(
                field,
            )))),
            Some(field)
        );
        assert_eq!(
            selected_field_id(Some(&RosterSubject::Node(GraphMemberId::from_u128(1)))),
            None
        );
    }

    #[test]
    fn link_facets_group_relations_by_family() {
        let from = GraphMemberId::from_u128(1);
        let to = GraphMemberId::from_u128(2);
        let facets = link_facets(
            from,
            to,
            &[
                LinkRelationRow {
                    from,
                    to,
                    family: EdgeFamily::Semantic,
                    family_label: "Semantic".to_string(),
                    kind_label: "Cites".to_string(),
                    label: None,
                    selector: RelationSelector::Semantic(SemanticSubKind::Cites),
                    editable: true,
                    selected: false,
                    hidden: false,
                },
                LinkRelationRow {
                    from,
                    to,
                    family: EdgeFamily::Semantic,
                    family_label: "Semantic".to_string(),
                    kind_label: "Quotes".to_string(),
                    label: None,
                    selector: RelationSelector::Semantic(SemanticSubKind::Quotes),
                    editable: true,
                    selected: false,
                    hidden: false,
                },
            ],
        );
        assert_eq!(facets.len(), 1);
        assert_eq!(facets[0].label, "Semantic");
        assert_eq!(facets[0].value, "2: Cites, Quotes");
        assert_eq!(
            facets[0].subject,
            RosterSubject::Facet(FacetSubject::LinkFamily {
                from,
                to,
                family: EdgeFamily::Semantic,
            })
        );
    }

    #[test]
    fn build_node_rows_sorts_and_marks_section_headers() {
        let rows = build_node_rows(vec![
            NodeRowInput {
                member: GraphMemberId::from_u128(1),
                title: "Zeta".to_string(),
                url: "https://zeta.test".to_string(),
                content_type: Some("text/html".to_string()),
                tags: Vec::new(),
                selected: false,
                open: false,
            },
            NodeRowInput {
                member: GraphMemberId::from_u128(2),
                title: "Alpha".to_string(),
                url: "https://alpha.test".to_string(),
                content_type: Some("application/rss+xml".to_string()),
                tags: Vec::new(),
                selected: false,
                open: false,
            },
        ]);
        assert_eq!(rows[0].title, "Zeta");
        assert_eq!(rows[0].section_header.as_deref(), Some("Documents"));
        assert_eq!(rows[1].title, "Alpha");
        assert_eq!(rows[1].section_header.as_deref(), Some("Feeds"));
    }

    #[test]
    fn build_link_card_sorts_relations_and_uses_selector_intents() {
        let from = GraphMemberId::from_u128(1);
        let to = GraphMemberId::from_u128(2);
        let card = build_link_card(LinkCardInput {
            from,
            to,
            source_title: "From".to_string(),
            source_url: "https://from.test".to_string(),
            target_title: "To".to_string(),
            target_url: "https://to.test".to_string(),
            hidden: false,
            relations: vec![
                LinkRelationInput {
                    from,
                    to,
                    kind: RelationKind::Semantic(SemanticSubKind::Quotes),
                    label: None,
                    selected: false,
                    hidden: false,
                },
                LinkRelationInput {
                    from,
                    to,
                    kind: RelationKind::Semantic(SemanticSubKind::Cites),
                    label: None,
                    selected: true,
                    hidden: false,
                },
            ],
        });
        assert_eq!(card.relations.len(), 2);
        assert_eq!(card.relations[0].kind_label, "Cites");
        assert_eq!(
            card.relations[0].selector,
            RelationSelector::Semantic(SemanticSubKind::Cites)
        );
        assert_eq!(card.facets[0].label, "Semantic");
    }

    #[test]
    fn build_graphlet_card_derives_drift_and_selector_labels() {
        let card = build_graphlet_card(GraphletCardInput {
            id: 7,
            kind: Some(GraphletKind::Facet),
            binding: GraphletBinding::Linked {
                spec: forme::GraphletSpec {
                    kind: GraphletKind::Facet,
                    anchors: Vec::new(),
                    primary_anchor: None,
                    selectors: vec!["semantic".to_string()],
                },
            },
            members: vec!["A".to_string()],
            family_selectors: Some(vec![(EdgeFamily::Semantic, true)]),
            added: vec!["B".to_string()],
            removed: vec!["C".to_string()],
        });
        assert_eq!(card.kind_label, "Facet");
        assert_eq!(card.selectors_label, "semantic");
        assert!(card.drift_tracking);
        assert_eq!(card.drift_summary, "drift proposal: +1 -1");
    }
}
