/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Data builders for Roster Facet Cards.

use std::collections::{BTreeMap, BTreeSet};

use forme::GraphMemberId;
use kernel::graph::{EdgeFamily, FieldExtent};

use super::{WindowCtx, roster};
use crate::roster_data::{content_bucket, edge_family_label};

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
            .map(|ct| content_bucket(Some(ct)).1)
            .unwrap_or("Unknown");
        Some(facet_card(
            "Content",
            detail.title.clone(),
            vec![
                info("content type", content),
                info("bucket", bucket),
                info("url", detail.url),
            ],
            vec![select_node_action(member)],
        ))
    }

    fn node_tags_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let detail = self.node_detail(member)?;
        Some(facet_card(
            "Tags",
            detail.title,
            vec![
                info("count", detail.tags.len().to_string()),
                info("tags", nonempty_join(&detail.tags)),
            ],
            vec![select_node_action(member)],
        ))
    }

    fn node_relations_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let graph = self.orrery().graph();
        let (key, _) = graph.get_node_by_id(member)?;
        let mut counts: BTreeMap<EdgeFamily, usize> = BTreeMap::new();
        for rel in graph.relations().filter(|r| r.from == key || r.to == key) {
            *counts.entry(rel.kind.family()).or_default() += 1;
        }
        let mut rows = vec![info("total", counts.values().sum::<usize>().to_string())];
        rows.extend(
            counts
                .into_iter()
                .map(|(family, count)| info(edge_family_label(family), count.to_string())),
        );
        Some(facet_card(
            "Relations",
            graph.node_display_label(key),
            rows,
            vec![select_node_action(member)],
        ))
    }

    fn node_fields_facet(&self, member: GraphMemberId) -> Option<roster::FacetCard> {
        let detail = self.node_detail(member)?;
        let fields = self.attached_field_names(member);
        Some(facet_card(
            "Fields",
            detail.title,
            vec![
                info("attached", fields.len().to_string()),
                info("fields", nonempty_join(&fields)),
            ],
            vec![select_node_action(member)],
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
            rows.push(info(
                &rel.kind_label,
                rel.label.as_deref().unwrap_or("relation cell"),
            ));
        }
        if rows.is_empty() {
            rows.push(info("relations", "none"));
        }
        Some(facet_card(
            edge_family_label(family),
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
        Some(facet_card(
            "Field rule",
            detail.name,
            vec![
                info("rule", detail.rule_label),
                info("script", "not configured"),
                info("template", "not configured"),
            ],
            vec![select_field_action(id)],
        ))
    }

    fn field_extent_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(facet_card(
            "Field extent",
            detail.name,
            vec![info("extent", detail.extent_label)],
            vec![select_field_action(id)],
        ))
    }

    fn field_visibility_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(facet_card(
            "Field visibility",
            detail.name,
            vec![info(
                "visibility",
                if detail.hidden { "hidden" } else { "visible" },
            )],
            vec![
                select_field_action(id),
                roster::FacetAction {
                    label: if detail.hidden { "show" } else { "hide" }.to_string(),
                    intent: roster::FacetActionIntent::ToggleFieldVisibility(id),
                },
            ],
        ))
    }

    fn field_strength_facet(&self, id: kernel::graph::FieldId) -> Option<roster::FacetCard> {
        let detail = self.field_detail(id)?;
        Some(facet_card(
            "Field strength",
            detail.name,
            vec![info("strength", format!("{:.0}", detail.strength / 1000.0))],
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

pub(super) fn node_facets(
    member: GraphMemberId,
    content_type: Option<&str>,
    tag_count: usize,
    relation_count: usize,
    field_count: usize,
) -> Vec<roster::FacetEntry> {
    vec![
        facet_entry(
            "Content",
            content_type.unwrap_or("unknown"),
            roster::FacetSubject::NodeContent(member),
        ),
        facet_entry(
            "Tags",
            tag_count.to_string(),
            roster::FacetSubject::NodeTags(member),
        ),
        facet_entry(
            "Relations",
            relation_count.to_string(),
            roster::FacetSubject::NodeRelations(member),
        ),
        facet_entry(
            "Fields",
            field_count.to_string(),
            roster::FacetSubject::NodeFields(member),
        ),
    ]
}

pub(super) fn link_facets(
    from: GraphMemberId,
    to: GraphMemberId,
    relations: &[roster::LinkRelationRow],
) -> Vec<roster::FacetEntry> {
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
                roster::FacetSubject::LinkFamily { from, to, family },
            )
        })
        .collect()
}

pub(super) fn field_facets(id: kernel::graph::FieldId) -> Vec<roster::FacetEntry> {
    vec![
        facet_entry("Rule", "inspect", roster::FacetSubject::FieldRule(id)),
        facet_entry("Extent", "inspect", roster::FacetSubject::FieldExtent(id)),
        facet_entry(
            "Visibility",
            "toggle",
            roster::FacetSubject::FieldVisibility(id),
        ),
        facet_entry("Strength", "tune", roster::FacetSubject::FieldStrength(id)),
    ]
}

fn facet_entry(
    label: impl Into<String>,
    value: impl Into<String>,
    subject: roster::FacetSubject,
) -> roster::FacetEntry {
    roster::FacetEntry {
        label: label.into(),
        value: value.into(),
        subject: roster::RosterSubject::Facet(subject),
    }
}

fn facet_card(
    title: impl Into<String>,
    subtitle: impl Into<String>,
    rows: Vec<roster::FacetInfoRow>,
    actions: Vec<roster::FacetAction>,
) -> roster::FacetCard {
    roster::FacetCard {
        title: title.into(),
        subtitle: subtitle.into(),
        rows,
        actions,
    }
}

fn info(label: impl Into<String>, value: impl Into<String>) -> roster::FacetInfoRow {
    roster::FacetInfoRow {
        label: label.into(),
        value: value.into(),
    }
}

fn select_node_action(member: GraphMemberId) -> roster::FacetAction {
    roster::FacetAction {
        label: "select node".to_string(),
        intent: roster::FacetActionIntent::SelectNode(member),
    }
}

fn select_field_action(id: kernel::graph::FieldId) -> roster::FacetAction {
    roster::FacetAction {
        label: "select field".to_string(),
        intent: roster::FacetActionIntent::SelectField(id),
    }
}

fn nonempty_join(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}
