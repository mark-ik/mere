/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Data builders for the graph-object Roster: node/link/graphlet/field tables
//! plus the selected subject's in-roster detail card.

use forme::{GraphMemberId, GraphletBinding, GraphletId};
use kernel::graph::RelationSelector;

use super::{WindowCtx, fetch};
use crate::pane_input_snapshot::PaneInputSnapshot;
use crate::roster;

impl WindowCtx<'_> {
    pub(super) fn roster_snapshot(
        &self,
        subject: Option<&roster::RosterSubject>,
    ) -> roster::RosterSnapshot {
        let input = self.pane_input_snapshot();
        roster::RosterSnapshot {
            node_rows: self.roster_node_rows(&input),
            link_rows: self.roster_link_rows(subject),
            graphlet_rows: self.roster_graphlet_rows(subject),
            field_rows: self.roster_field_rows(subject),
            detail: subject.and_then(|s| self.roster_detail(s, &input)),
        }
    }

    pub(super) fn roster_rows(&self) -> Vec<roster::RosterRow> {
        let input = self.pane_input_snapshot();
        self.roster_node_rows(&input)
    }

    fn roster_node_rows(&self, input: &PaneInputSnapshot) -> Vec<roster::RosterRow> {
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
                    selected: input.is_selected(node.id),
                    open: input.is_open(node.id),
                    section_header: None,
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            let ba = roster::content_bucket(a.content_type.as_deref());
            let bb = roster::content_bucket(b.content_type.as_deref());
            ba.0.cmp(&bb.0)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        let mut current: Option<u8> = None;
        for row in &mut rows {
            let (ord, label) = roster::content_bucket(row.content_type.as_deref());
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
                let selector = roster::relation_selector(relation.kind);
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
                    family_label: roster::edge_family_label(relation.kind.family()).to_string(),
                    kind_label: roster::relation_kind_label(relation.kind).to_string(),
                    source_label: roster::relation_label(graph, relation.from, relation.to),
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
                    kind_label: roster::graphlet_kind_label(g.kind.as_ref()),
                    binding_label: roster::graphlet_binding_label(&g.binding).to_string(),
                    member_count: g.anchors.len(),
                    selectors_label: roster::graphlet_selectors_label(g),
                    drift_label,
                    selected: matches!(subject, Some(roster::RosterSubject::Graphlet(id)) if *id == g.id),
                }
            })
            .collect()
    }

    fn roster_field_rows(&self, subject: Option<&roster::RosterSubject>) -> Vec<roster::FieldRow> {
        let mut out = Vec::new();
        let selected_field = roster::selected_field_id(subject);
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
                rule_label: roster::field_definition_label(&field.definition).to_string(),
                extent_label: roster::field_extent_label(&field.extent),
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

    fn roster_detail(
        &self,
        subject: &roster::RosterSubject,
        input: &PaneInputSnapshot,
    ) -> Option<roster::RosterDetail> {
        match subject {
            roster::RosterSubject::Node(member) => self
                .node_detail_with_input(*member, input)
                .map(roster::RosterDetail::Node),
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
        let input = self.pane_input_snapshot();
        self.node_detail_with_input(member, &input)
    }

    fn node_detail_with_input(
        &self,
        member: GraphMemberId,
        input: &PaneInputSnapshot,
    ) -> Option<roster::NodeDetail> {
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
            open: input.is_open(member),
            facets: roster::node_facets(
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
                let selector = roster::relation_selector(r.kind);
                roster::LinkRelationRow {
                    from,
                    to,
                    family: r.kind.family(),
                    family_label: roster::edge_family_label(r.kind.family()).to_string(),
                    kind_label: roster::relation_kind_label(r.kind).to_string(),
                    label: roster::relation_label(graph, r.from, r.to),
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
        let facets = roster::link_facets(from, to, &relations);
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
        let family_selectors = match &graphlet.binding {
            GraphletBinding::Linked { spec } => Some(
                crate::graphlets::EDGE_FAMILIES
                    .iter()
                    .map(|&family| (family, crate::graphlets::spec_has_family(spec, family)))
                    .collect(),
            ),
            _ => None,
        };
        Some(roster::GraphletCard {
            id,
            kind_label: roster::graphlet_kind_label(graphlet.kind.as_ref()),
            binding_label: roster::graphlet_binding_label(&graphlet.binding).to_string(),
            members: roster::member_labels(graph, &graphlet.anchors),
            selectors_label: roster::graphlet_selectors_label(graphlet),
            family_selectors,
            drift_tracking,
            drift_summary,
            added: delta
                .as_ref()
                .map(|d| roster::member_labels(graph, &d.added))
                .unwrap_or_default(),
            removed: delta
                .as_ref()
                .map(|d| roster::member_labels(graph, &d.removed))
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
            rule_label: roster::field_definition_label(&field.definition).to_string(),
            extent_label: roster::field_extent_label(&field.extent),
            hidden: !self.orrery().field_visible(id),
            strength: self.orrery().field_strength(id).unwrap_or(0.0),
            facets: roster::field_facets(id),
        })
    }
}
