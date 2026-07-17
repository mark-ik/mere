//! Nested graphs: a graph contained within a node.
//!
//! A node that implements [`GraphBearing`] carries the log identity of a
//! nested graph. The nested graph is an ordinary [`GraphLog`] persisted under
//! the slot convention below, so bearing is a reference, never an embedding:
//! the parent's payload stays one `LogId` wide, and the nested graph loads on
//! demand through the same [`load_full`](GraphLog::load_full) /
//! [`load_checkpointed`](GraphLog::load_checkpointed) paths as any graph.
//!
//! Lifecycle is owned by the bearing node. Removing it through
//! [`GraphLog::remove_bearing_node`] archives the nested graph's slots (one
//! atomic move to the `archive/` prefix) *before* the node leaves the parent,
//! so a nested graph is archived, never orphaned: `keys("nested/")` lists the
//! live ones, `keys("archive/nested/")` the archived ones, and a failed
//! archive leaves the bearing node in place.

use codicil::LogId;
use muniment::{Backend, Codec, SlotStore, StoreError, WriteOp};

use crate::caps::{GraphBearing, Identified};
use crate::spine::GraphLog;

/// The live slot for a nested graph's edit log.
pub fn log_slot(id: &LogId) -> String {
    format!("nested/{}/log", id.as_str())
}

/// The live slot for a nested graph's snapshot.
pub fn snap_slot(id: &LogId) -> String {
    format!("nested/{}/snap", id.as_str())
}

/// The archive slot for a nested graph's edit log.
pub fn archived_log_slot(id: &LogId) -> String {
    format!("archive/nested/{}/log", id.as_str())
}

/// The archive slot for a nested graph's snapshot.
pub fn archived_snap_slot(id: &LogId) -> String {
    format!("archive/nested/{}/snap", id.as_str())
}

/// Move a nested graph's slots (the log, and the snapshot if present) to the
/// archive prefix in one atomic batch. Returns whether anything was live to
/// move. Codec-agnostic: the bytes travel untouched through the raw
/// [`Backend`].
pub async fn archive_nested<B: Backend>(backend: &B, id: &LogId) -> Result<bool, StoreError> {
    let Some(log_bytes) = backend.get(&log_slot(id)).await? else {
        return Ok(false);
    };
    let mut ops = vec![
        WriteOp::Put {
            key: archived_log_slot(id),
            value: log_bytes,
        },
        WriteOp::Delete { key: log_slot(id) },
    ];
    if let Some(snap_bytes) = backend.get(&snap_slot(id)).await? {
        ops.push(WriteOp::Put {
            key: archived_snap_slot(id),
            value: snap_bytes,
        });
        ops.push(WriteOp::Delete { key: snap_slot(id) });
    }
    backend.apply(&ops).await?;
    Ok(true)
}

impl<N: Identified + GraphBearing, E> GraphLog<N, E> {
    /// The nested graph borne by the node with `id`, if the node is present
    /// and bears one.
    pub fn nested_of(&self, id: &N::Id) -> Option<LogId> {
        let key = self.graph().key_of(id)?;
        self.graph().node(key)?.nested().cloned()
    }

    /// Every nested graph borne by a live node, in unspecified order.
    pub fn live_nested(&self) -> Vec<LogId> {
        self.graph()
            .nodes()
            .filter_map(|(_, node)| node.nested().cloned())
            .collect()
    }
}

impl<N, E> GraphLog<N, E>
where
    N: Identified + GraphBearing + Clone,
    E: Clone,
{
    /// Remove the node with `id`, first archiving the nested graph it bears,
    /// so removal never orphans one. The archive lands before the node leaves
    /// the graph: if archiving fails, the node stays. Returns the archived
    /// graph's identity, or `None` if the node bore none (the node is removed
    /// either way).
    pub async fn remove_bearing_node<B: Backend, C: Codec>(
        &mut self,
        slots: &SlotStore<B, C>,
        id: &N::Id,
    ) -> Result<Option<LogId>, StoreError> {
        let nested = self.nested_of(id);
        if let Some(nested_id) = &nested {
            archive_nested(slots.backend(), nested_id).await?;
        }
        self.remove_node(id);
        Ok(nested)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::{Container, Relation};
    use crate::graph::{Graph, NodeKey};
    use crate::taxonomy::{Recognized, RelationClass};
    use muniment::{JsonSlots, MemoryBackend};

    /// A canonical, key-independent view: sorted node ids and sorted
    /// (from, to) pairs (the spine test's fingerprint, minus relation detail).
    fn fingerprint(graph: &Graph<Container, Relation>) -> (Vec<String>, Vec<(String, String)>) {
        let mut nodes: Vec<String> = graph.nodes().map(|(_, n)| n.id.clone()).collect();
        nodes.sort();
        let keys: Vec<NodeKey> = graph.nodes().map(|(k, _)| k).collect();
        let mut edges = Vec::new();
        for key in keys {
            let from = graph.node(key).unwrap().id.clone();
            for (_, target, _) in graph.out_edges(key) {
                edges.push((from.clone(), graph.node(target).unwrap().id.clone()));
            }
        }
        edges.sort();
        (nodes, edges)
    }

    fn cites() -> Relation {
        Relation::new(RelationClass::recognized(Recognized::Cites))
    }

    #[test]
    fn bearing_node_round_trips_persist_and_replay() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());

            // The nested graph: a servitor's own small world.
            let nid = LogId::new("servitor.trail");
            let mut nested = GraphLog::<Container, Relation>::with_id(nid.clone());
            nested.insert_node(Container::new("grant"));
            nested.insert_node(Container::new("cursor"));
            nested.connect(&"grant".to_string(), &"cursor".to_string(), cites());
            nested.save_log(&slots, &log_slot(&nid)).await.unwrap();

            // The parent bears it by identity only.
            let mut parent = GraphLog::<Container, Relation>::new();
            parent.insert_node(Container::new("servitor").with_nested(nid.clone()));
            parent.save_log(&slots, "parent/log").await.unwrap();

            // Reload the parent, follow the bearing, reload the nested graph.
            let parent_back = GraphLog::<Container, Relation>::load_full(&slots, "parent/log")
                .await
                .unwrap();
            let followed = parent_back
                .nested_of(&"servitor".to_string())
                .expect("the bearing survived replay");
            assert_eq!(followed, nid);
            let nested_back =
                GraphLog::<Container, Relation>::load_full(&slots, &log_slot(&followed))
                    .await
                    .unwrap();
            assert_eq!(fingerprint(nested_back.graph()), fingerprint(nested.graph()));
            assert_eq!(nested_back.id(), Some(&nid), "identity survives the trip");
            assert_eq!(parent_back.live_nested(), vec![nid]);
        });
    }

    #[test]
    fn removing_a_bearing_node_archives_never_orphans() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());

            let nid = LogId::new("servitor.trail");
            let mut nested = GraphLog::<Container, Relation>::with_id(nid.clone());
            nested.insert_node(Container::new("grant"));
            nested.save_log(&slots, &log_slot(&nid)).await.unwrap();
            nested.snapshot(&slots, &snap_slot(&nid)).await.unwrap();

            let mut parent = GraphLog::<Container, Relation>::new();
            parent.insert_node(Container::new("servitor").with_nested(nid.clone()));
            parent.insert_node(Container::new("plain"));

            let archived = parent
                .remove_bearing_node(&slots, &"servitor".to_string())
                .await
                .unwrap();
            assert_eq!(archived, Some(nid.clone()));

            // Gone from the live prefix, both slots present under the archive.
            assert!(
                slots.keys("nested/").await.unwrap().is_empty(),
                "no live slots remain"
            );
            let mut moved = slots.keys("archive/nested/").await.unwrap();
            moved.sort();
            assert_eq!(
                moved,
                vec![archived_log_slot(&nid), archived_snap_slot(&nid)]
            );

            // The archived log still replays: archived, not destroyed.
            let archived_back =
                GraphLog::<Container, Relation>::load_full(&slots, &archived_log_slot(&nid))
                    .await
                    .unwrap();
            assert_eq!(archived_back.graph().node_count(), 1);

            // The parent's own replay agrees the node (and the bearing) is gone.
            let replayed = GraphLog::replay(parent.log().clone());
            assert_eq!(replayed.graph().node_count(), 1);
            assert!(replayed.live_nested().is_empty());

            // A node bearing nothing removes through the same API as a no-op archive.
            let none = parent
                .remove_bearing_node(&slots, &"plain".to_string())
                .await
                .unwrap();
            assert_eq!(none, None);
            assert_eq!(parent.graph().node_count(), 0);
        });
    }

    #[test]
    fn pre_nesting_containers_still_deserialize() {
        let old = r#"{"id":"a","addresses":[],"content":null,"media_type":null,"title":null,"tags":[]}"#;
        let container: Container = serde_json::from_str(old).unwrap();
        assert!(container.nested.is_none(), "absent field defaults to None");
    }
}
