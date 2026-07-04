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
        let rows: Vec<roster::NodeRowInput> = graph
            .nodes()
            .map(|(key, node)| {
                let url = node.url().to_string();
                let content_type = match self.shared.content.pages.get(&url) {
                    Some(fetch::ContentState::Ready(fetched)) => fetched.content_type.clone(),
                    _ => node.mime_hint.clone(),
                };
                let mut tags: Vec<String> = node.tags.iter().cloned().collect();
                tags.sort();
                roster::NodeRowInput {
                    member: node.id,
                    title: graph.node_display_label(key),
                    url,
                    content_type,
                    tags,
                    selected: input.is_selected(node.id),
                    open: input.is_open(node.id),
                }
            })
            .collect();
        roster::build_node_rows(rows)
    }

    fn roster_link_rows(&self, subject: Option<&roster::RosterSubject>) -> Vec<roster::LinkRow> {
        let graph = self.orrery().graph();
        let rows: Vec<roster::LinkRowInput> = graph
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
                Some(roster::LinkRowInput {
                    from: source.id,
                    to: target.id,
                    source_title: graph.node_display_label(relation.from),
                    source_url: source.url().to_string(),
                    target_title: graph.node_display_label(relation.to),
                    target_url: target.url().to_string(),
                    kind: relation.kind,
                    source_label: roster::relation_label(graph, relation.from, relation.to),
                    selected,
                })
            })
            .collect();
        roster::build_link_rows(rows)
    }

    fn roster_graphlet_rows(
        &self,
        subject: Option<&roster::RosterSubject>,
    ) -> Vec<roster::GraphletRow> {
        let Some(index) = self.graphlets.get(&self.view.focused_graph) else {
            return Vec::new();
        };
        let graph = self.orrery().graph();
        let rows: Vec<roster::GraphletRowInput> = index
            .graphlets()
            .iter()
            .map(|g| {
                let delta = index.preview_reconcile(graph, g.id);
                roster::GraphletRowInput {
                    id: g.id,
                    kind: g.kind.clone(),
                    binding: g.binding.clone(),
                    member_count: g.anchors.len(),
                    added_count: delta.as_ref().map(|delta| delta.added.len()).unwrap_or(0),
                    removed_count: delta.as_ref().map(|delta| delta.removed.len()).unwrap_or(0),
                    selected: matches!(subject, Some(roster::RosterSubject::Graphlet(id)) if *id == g.id),
                }
            })
            .collect();
        roster::build_graphlet_rows(rows)
    }

    fn roster_field_rows(&self, subject: Option<&roster::RosterSubject>) -> Vec<roster::FieldRow> {
        let selected_field = roster::selected_field_id(subject);
        let rows: Vec<roster::FieldRowInput> = self
            .orrery()
            .graph()
            .fields()
            .filter(|field| field.is_active())
            .map(|field| roster::FieldRowInput {
                id: field.id,
                name: field.name.clone(),
                definition: field.definition.clone(),
                extent: field.extent.clone(),
                hidden: !self.orrery().field_visible(field.id),
                selected: selected_field == Some(field.id),
                strength: self.orrery().field_strength(field.id).unwrap_or(0.0),
            })
            .collect();
        roster::build_field_rows(rows)
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
        Some(roster::build_node_detail(roster::NodeDetailInput {
            member,
            title: graph.node_display_label(key),
            url,
            content_type,
            tags,
            relation_count,
            field_count,
            open: input.is_open(member),
        }))
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
        let relations: Vec<roster::LinkRelationInput> = graph
            .relations()
            .filter(|r| r.from == from_key && r.to == to_key)
            .map(|r| {
                let selector = roster::relation_selector(r.kind);
                roster::LinkRelationInput {
                    from,
                    to,
                    kind: r.kind,
                    label: roster::relation_label(graph, r.from, r.to),
                    selected: selected_selector == Some(selector),
                    hidden: self
                        .orrery()
                        .relation_between_members_hidden(from, to, selector),
                }
            })
            .collect();
        Some(roster::build_link_card(roster::LinkCardInput {
            from,
            to,
            source_title: graph.node_display_label(from_key),
            source_url: source.url().to_string(),
            target_title: graph.node_display_label(to_key),
            target_url: target.url().to_string(),
            hidden: self.orrery().edge_between_members_hidden(from, to),
            relations,
        }))
    }

    fn graphlet_card(&self, id: GraphletId) -> Option<roster::GraphletCard> {
        let graph = self.orrery().graph();
        let index = self.graphlets.get(&self.view.focused_graph)?;
        let graphlet = index.get(id)?;
        let delta = index.preview_reconcile(graph, id);
        let family_selectors = match &graphlet.binding {
            GraphletBinding::Linked { spec } => Some(
                crate::graphlets::EDGE_FAMILIES
                    .iter()
                    .map(|&family| (family, crate::graphlets::spec_has_family(spec, family)))
                    .collect(),
            ),
            _ => None,
        };
        Some(roster::build_graphlet_card(roster::GraphletCardInput {
            id,
            kind: graphlet.kind.clone(),
            binding: graphlet.binding.clone(),
            members: roster::member_labels(graph, &graphlet.anchors),
            family_selectors,
            added: delta
                .as_ref()
                .map(|d| roster::member_labels(graph, &d.added))
                .unwrap_or_default(),
            removed: delta
                .as_ref()
                .map(|d| roster::member_labels(graph, &d.removed))
                .unwrap_or_default(),
        }))
    }

    pub(super) fn field_detail(&self, id: kernel::graph::FieldId) -> Option<roster::FieldDetail> {
        let field = self
            .orrery()
            .graph()
            .fields()
            .find(|field| field.id == id)?;
        Some(roster::build_field_detail(roster::FieldDetailInput {
            id,
            name: field.name.clone(),
            definition: field.definition.clone(),
            extent: field.extent.clone(),
            hidden: !self.orrery().field_visible(id),
            strength: self.orrery().field_strength(id).unwrap_or(0.0),
        }))
    }
}
