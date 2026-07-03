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
                let uuid = field.id.as_uuid().to_string();
                names.push(
                    field
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Field {}", &uuid[..8.min(uuid.len())])),
                );
            }
        }
        names.sort();
        names
    }

    fn node_content_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let detail = self.node_detail(member)?;
        let content = detail.content_type.as_deref().unwrap_or("unknown");
        let bucket = detail
            .content_type
            .as_deref()
            .map(|ct| roster::content_bucket(Some(ct)).1)
            .unwrap_or("Unknown");
        Some(roster::facet_card(
            "Content",
            detail.title.clone(),
            vec![
                roster::info("content type", content),
                roster::info("bucket", bucket),
                roster::info("url", detail.url),
            ],
            vec![roster::select_node_action(member)],
        ))
    }

    fn node_tags_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let detail = self.node_detail(member)?;
        Some(roster::facet_card(
            "Tags",
            detail.title,
            vec![
                roster::info("count", detail.tags.len().to_string()),
                roster::info("tags", roster::nonempty_join(&detail.tags)),
            ],
            vec![roster::select_node_action(member)],
        ))
    }

    fn node_relations_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let graph = self.orrery().graph();
        let (key, _) = graph.get_node_by_id(member)?;
        let mut counts: BTreeMap<EdgeFamily, usize> = BTreeMap::new();
        for rel in graph.relations().filter(|r| r.from == key || r.to == key) {
            *counts.entry(rel.kind.family()).or_default() += 1;
        }
        let mut rows = vec![roster::info(
            "total",
            counts.values().sum::<usize>().to_string(),
        )];
        rows.extend(counts.into_iter().map(|(family, count)| {
            roster::info(roster::edge_family_label(family), count.to_string())
        }));
        Some(roster::facet_card(
            "Relations",
            graph.node_display_label(key),
            rows,
            vec![roster::select_node_action(member)],
        ))
    }

    fn node_fields_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let detail = self.node_detail(member)?;
        let fields = self.attached_field_names(member);
        Some(roster::facet_card(
            "Fields",
            detail.title,
            vec![
                roster::info("attached", fields.len().to_string()),
                roster::info("fields", roster::nonempty_join(&fields)),
            ],
            vec![roster::select_node_action(member)],
        ))
    }

    fn link_family_facet(
        &self,
        from: GraphMemberId,
        to: GraphMemberId,
        family: EdgeFamily,
    ) -> Option<roster::FacetCard> {
        let link = self.link_card(from, to, None)?;
        let mut rows = Vec::new();
        for rel in link.relations.iter().filter(|rel| rel.family == family) {
            rows.push(roster::info(
                &rel.kind_label,
                rel.label.as_deref().unwrap_or("relation cell"),
            ));
        }
        if rows.is_empty() {
            rows.push(roster::info("relations", "none"));
        }
        Some(roster::facet_card(
            roster::edge_family_label(family),
            format!("{} -> {}", link.source_title, link.target_title),
            rows,
            vec![roster::FacetAction {
                label: "open link".to_string(),
                intent: roster::FacetActionIntent::OpenLinkBundle { from, to },
            }],
        ))
    }

    fn field_rule_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(roster::facet_card(
            "Field rule",
            detail.name,
            vec![
                roster::info("rule", detail.rule_label),
                roster::info("script", "not configured"),
                roster::info("template", "not configured"),
            ],
            vec![roster::select_field_action(id)],
        ))
    }

    fn field_extent_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(roster::facet_card(
            "Field extent",
            detail.name,
            vec![roster::info("extent", detail.extent_label)],
            vec![roster::select_field_action(id)],
        ))
    }

    fn field_visibility_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(roster::facet_card(
            "Field visibility",
            detail.name,
            vec![roster::info(
                "visibility",
                if detail.hidden { "hidden" } else { "visible" },
            )],
            vec![
                roster::select_field_action(id),
                roster::FacetAction {
                    label: if detail.hidden { "show" } else { "hide" }.to_string(),
                    intent: roster::FacetActionIntent::ToggleFieldVisibility(id),
                },
            ],
        ))
    }

    fn field_strength_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(roster::facet_card(
            "Field strength",
            detail.name,
            vec![roster::info(
                "strength",
                format!("{:.0}", detail.strength / 1000.0),
            )],
            vec![
                roster::FacetAction {
                    label: "weaker".to_string(),
                    intent: roster::FacetActionIntent::AdjustFieldStrength(id, -1000.0),
                },
                roster::FacetAction {
                    label: "stronger".to_string(),
                    intent: roster::FacetActionIntent::AdjustFieldStrength(id, 1000.0),
                },
            ],
        ))
    }
}
