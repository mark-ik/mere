/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! View-intent bridges for display state the kernel graph does not own.

use std::collections::BTreeSet;

use kernel::graph::{Graph, RelationKind};
use session_runtime::{HiddenRelationRecord, ViewIntent};

pub(crate) fn restore_hidden_relations(orrery: &mut orrery::Orrery, intent: Option<&ViewIntent>) {
    let Some(intent) = intent else {
        return;
    };
    let pairs: Vec<(uuid::Uuid, uuid::Uuid)> = intent
        .hidden_relations
        .iter()
        .filter(|record| relation_record_exists(orrery.graph(), record))
        .map(|record| (record.from_id, record.to_id))
        .collect();
    for (from, to) in pairs {
        orrery.hide_edge_between_members(from, to);
    }
}

pub(crate) fn hidden_relation_records(orrery: &orrery::Orrery) -> BTreeSet<HiddenRelationRecord> {
    let graph = orrery.graph();
    let mut out = BTreeSet::new();
    for (a, b) in orrery.hidden_edge_member_pairs() {
        let (Some(ak), Some(bk)) = (graph.get_node_key_by_id(a), graph.get_node_key_by_id(b))
        else {
            continue;
        };
        for relation in graph.relations() {
            if (relation.from == ak && relation.to == bk)
                || (relation.from == bk && relation.to == ak)
            {
                let Some(from) = graph.get_node(relation.from).map(|node| node.id) else {
                    continue;
                };
                let Some(to) = graph.get_node(relation.to).map(|node| node.id) else {
                    continue;
                };
                out.insert(HiddenRelationRecord::new(from, to, relation.kind.tag()));
            }
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

#[cfg(test)]
mod tests {
    use kernel::graph::SemanticSubKind;

    use super::*;

    #[test]
    fn hidden_relation_records_round_trip_endpoint_visibility() {
        let mut orrery = orrery::Orrery::new();
        let a_key = orrery.visit("https://a.example");
        let a = orrery.graph().get_node(a_key).unwrap().id;
        let b_key = orrery.visit("https://b.example");
        let b = orrery.graph().get_node(b_key).unwrap().id;
        assert!(orrery.assert_relation_between_members(a, b, SemanticSubKind::Cites));
        assert!(orrery.hide_edge_between_members(a, b));

        let records = hidden_relation_records(&orrery);
        assert!(records.iter().any(|record| {
            record.from_id == a
                && record.to_id == b
                && record.relation_tag
                    == kernel::graph::RelationKind::Semantic(SemanticSubKind::Cites).tag()
        }));

        let mut restored = orrery::Orrery::with_graph(orrery.graph().clone());
        let intent = ViewIntent {
            hidden_relations: records,
            ..Default::default()
        };
        restore_hidden_relations(&mut restored, Some(&intent));
        assert!(restored.edge_between_members_hidden(a, b));
    }
}
