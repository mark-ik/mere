/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Data builders for Roster Facet Cards.

use std::collections::BTreeMap;

use forme::GraphMemberId;
use kernel::graph::{EdgeFamily, FieldExtent};

use super::WindowCtx;
use crate::roster;

impl WindowCtx<'_> {
    pub(super) fn facet_card(&self, facet: &roster::FacetSubject) -> Option<roster::FacetCard> {
        match *facet {
            roster::FacetSubject::NodeContent(member) => self.node_content_facet(member),
            roster::FacetSubject::NodeTags(member) => self.node_tags_facet(member),
            roster::FacetSubject::NodeRelations(member) => self.node_relations_facet(member),
            roster::FacetSubject::NodeFields(member) => self.node_fields_facet(member),
            roster::FacetSubject::LinkFamily { from, to, family } => {
                self.link_family_facet(from, to, family)
            }
            roster::FacetSubject::FieldRule(id) => self.field_rule_facet(id),
            roster::FacetSubject::FieldExtent(id) => self.field_extent_facet(id),
            roster::FacetSubject::FieldVisibility(id) => self.field_visibility_facet(id),
            roster::FacetSubject::FieldStrength(id) => self.field_strength_facet(id),
        }
    }

    pub(super) fn attached_field_names(&self, member: GraphMemberId) -> Vec<String> {
        let mut names = Vec::new();
        for field in self.orrery().graph().fields() {
            if field.is_active()
                && matches!(&field.extent, FieldExtent::AttachedToNode(id) if *id == member)
            {
                names.push(roster::display_field_name(field.name.as_deref(), field.id));
            }
        }
        names.sort();
        names
    }

    fn node_content_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let detail = self.node_detail(member)?;
        Some(roster::build_node_content_facet_card(&detail))
    }

    fn node_tags_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let detail = self.node_detail(member)?;
        Some(roster::build_node_tags_facet_card(&detail))
    }

    fn node_relations_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let graph = self.orrery().graph();
        let (key, _) = graph.get_node_by_id(member)?;
        let mut counts: BTreeMap<EdgeFamily, usize> = BTreeMap::new();
        for rel in graph.relations().filter(|r| r.from == key || r.to == key) {
            *counts.entry(rel.kind.family()).or_default() += 1;
        }
        Some(roster::build_node_relations_facet_card(
            roster::NodeRelationsFacetInput {
                member,
                title: graph.node_display_label(key),
                counts_by_family: counts.into_iter().collect(),
            },
        ))
    }

    fn node_fields_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let detail = self.node_detail(member)?;
        let fields = self.attached_field_names(member);
        Some(roster::build_node_fields_facet_card(&detail, &fields))
    }

    fn link_family_facet(
        &self,
        from: GraphMemberId,
        to: GraphMemberId,
        family: EdgeFamily,
    ) -> Option<roster::FacetCard> {
        let link = self.link_card(from, to, None)?;
        Some(roster::build_link_family_facet_card(&link, family))
    }

    fn field_rule_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(roster::build_field_rule_facet_card(&detail))
    }

    fn field_extent_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(roster::build_field_extent_facet_card(&detail))
    }

    fn field_visibility_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(roster::build_field_visibility_facet_card(&detail))
    }

    fn field_strength_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(roster::build_field_strength_facet_card(&detail))
    }
}
