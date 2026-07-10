/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! View-intent bridges for display state the kernel graph does not own.

use std::collections::BTreeSet;

use mere::kernel::graph::{EdgeFamily, Graph, RelationKind, RelationSelector};
use session_runtime::{HiddenRelationRecord, ViewIntent};

pub(crate) fn restore_hidden_relations(orrery: &mut mere::canvas::Canvas, intent: Option<&ViewIntent>) {
    let Some(intent) = intent else {
        return;
    };
    let records: Vec<(uuid::Uuid, uuid::Uuid, RelationSelector)> = intent
        .hidden_relations
        .iter()
        .filter_map(|record| {
            let kind = RelationKind::from_tag(record.relation_tag)?;
            relation_record_exists(orrery.graph(), record).then_some((
                record.from_id,
                record.to_id,
                selector_for_relation_kind(kind),
            ))
        })
        .collect();
    for (from, to, selector) in records {
        orrery.hide_relation_between_members(from, to, selector);
    }
}

pub(crate) fn hidden_relation_records(orrery: &mere::canvas::Canvas) -> BTreeSet<HiddenRelationRecord> {
    let graph = orrery.graph();
    let mut out = BTreeSet::new();
    for relation in graph.relations() {
        let Some(from) = graph.get_node(relation.from).map(|node| node.id) else {
            continue;
        };
        let Some(to) = graph.get_node(relation.to).map(|node| node.id) else {
            continue;
        };
        let selector = selector_for_relation_kind(relation.kind);
        if orrery.relation_between_members_hidden(from, to, selector) {
            out.insert(HiddenRelationRecord::new(from, to, relation.kind.tag()));
        }
    }
    out
}

fn relation_record_exists(graph: &Graph, record: &HiddenRelationRecord) -> bool {
    let Some(kind) = RelationKind::from_tag(record.relation_tag) else {
        return false;
    };
    let (Some(from), Some(to)) = (
        graph.get_node_key_by_id(record.from_id),
        graph.get_node_key_by_id(record.to_id),
    ) else {
        return false;
    };
    graph
        .relations()
        .any(|relation| relation.from == from && relation.to == to && relation.kind == kind)
}

fn selector_for_relation_kind(kind: RelationKind) -> RelationSelector {
    match kind {
        RelationKind::Semantic(sub) => RelationSelector::Semantic(sub),
        RelationKind::Traversal => RelationSelector::Family(EdgeFamily::Traversal),
        RelationKind::Containment(sub) => RelationSelector::Containment(sub),
        RelationKind::Arrangement(sub) => RelationSelector::Arrangement(sub),
        RelationKind::Imported(sub) => RelationSelector::Imported(sub),
        RelationKind::Provenance(sub) => RelationSelector::Provenance(sub),
    }
}

#[cfg(test)]
mod tests {
    use mere::kernel::graph::SemanticSubKind;

    use super::*;

    #[test]
    fn hidden_relation_records_round_trip_relation_cell_visibility() {
        let mut orrery = mere::canvas::Canvas::new();
        let a_key = orrery.visit("https://a.example");
        let a = orrery.graph().get_node(a_key).unwrap().id;
        let b_key = orrery.visit("https://b.example");
        let b = orrery.graph().get_node(b_key).unwrap().id;
        assert!(orrery.assert_relation_between_members(a, b, SemanticSubKind::Cites));
        assert!(orrery.assert_relation_between_members(a, b, SemanticSubKind::Quotes));
        assert!(orrery.hide_relation_between_members(
            a,
            b,
            RelationSelector::Semantic(SemanticSubKind::Cites)
        ));

        let records = hidden_relation_records(&orrery);
        assert!(records.iter().any(|record| {
            record.from_id == a
                && record.to_id == b
                && record.relation_tag
                    == mere::kernel::graph::RelationKind::Semantic(SemanticSubKind::Cites).tag()
        }));
        assert!(!records.iter().any(|record| {
            record.from_id == a
                && record.to_id == b
                && record.relation_tag
                    == mere::kernel::graph::RelationKind::Semantic(SemanticSubKind::Quotes).tag()
        }));

        let mut restored = mere::canvas::Canvas::with_graph(orrery.graph().clone());
        let intent = ViewIntent {
            hidden_relations: records,
            ..Default::default()
        };
        restore_hidden_relations(&mut restored, Some(&intent));
        assert!(restored.relation_between_members_hidden(
            a,
            b,
            RelationSelector::Semantic(SemanticSubKind::Cites)
        ));
        assert!(!restored.relation_between_members_hidden(
            a,
            b,
            RelationSelector::Semantic(SemanticSubKind::Quotes)
        ));
    }
}
