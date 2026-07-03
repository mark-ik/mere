/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

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
}
