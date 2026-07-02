/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The off-thread **community-detection lane**: an [`armillary`] actor that runs Louvain on a
//! `Send` [`CommunitySnapshot`] and emits a revision-stamped [`ClusterSet`]. Mirrors the physics
//! actor (the same off-thread discipline). Used only when the host has supplied an off-thread wake
//! (native + offloaded); on wasm and in tests the orrery computes community inline instead, so this
//! lane is never spun up there. (Graph signals — the background lane, P3.)
//!
//! The `Graph` never crosses the thread: the owner extracts a `Send` [`CommunitySnapshot`] on its
//! own thread, sends it, and the worker computes the partition. A result computed against a
//! since-superseded graph is rejected by **revision** on arrival (stale-rejection), so a layout or
//! ring never paints a partition that no longer matches the graph.

use std::sync::mpsc::Receiver;

use armillary::{ActorHandle, Emitter, Wake, spawn};
use signals::{ClusterSet, CommunitySnapshot, community_louvain_on_snapshot};

/// A recompute request: the structure to partition, stamped with the graph revision it was taken at.
pub(crate) struct CommunityRequest {
    snapshot: CommunitySnapshot,
    revision: u64,
}

/// A finished partition, carrying the revision it was computed against. (Graph signals.)
pub(crate) struct CommunityUpdate {
    pub clusters: ClusterSet,
    pub revision: u64,
}

/// The host side of the community worker: dispatch snapshots, drain results. Dropping it ends the
/// worker thread (its command channel closes). (Graph signals — the background lane.)
pub(crate) struct CommunityActor {
    handle: ActorHandle<CommunityRequest>,
    updates: Receiver<CommunityUpdate>,
    /// The revision of an in-flight request, so a stable graph is not re-dispatched while a result
    /// is pending. Cleared when that revision's result lands.
    inflight: Option<u64>,
}

impl CommunityActor {
    /// Spawn the worker on its own thread. `wake` pokes the host loop when a result lands, so an
    /// on-demand renderer redraws to pick it up via [`drain`](Self::drain).
    pub fn spawn(wake: Wake) -> Self {
        let (handle, updates) = spawn(wake, |commands, out: Emitter<CommunityUpdate>| {
            while let Ok(CommunityRequest { snapshot, revision }) = commands.recv() {
                let clusters = community_louvain_on_snapshot(&snapshot);
                out.emit(CommunityUpdate { clusters, revision });
            }
        });
        Self {
            handle,
            updates,
            inflight: None,
        }
    }

    /// Dispatch a recompute for `revision`, unless one is already in flight for that same revision
    /// (so a stable graph is not recomputed repeatedly while a result is pending).
    pub fn request(&mut self, snapshot: CommunitySnapshot, revision: u64) {
        if self.inflight == Some(revision) {
            return;
        }
        if self.handle.command(CommunityRequest { snapshot, revision }) {
            self.inflight = Some(revision);
        }
    }

    /// Drain to the freshest result (older queued ones are superseded — only the latest partition
    /// matters). Clears the in-flight marker when its revision lands. Returns the freshest update,
    /// if any; the caller accepts it only if its revision still matches the live graph.
    pub fn drain(&mut self) -> Option<CommunityUpdate> {
        let mut latest = None;
        while let Ok(update) = self.updates.try_recv() {
            latest = Some(update);
        }
        if let Some(update) = &latest {
            if self.inflight == Some(update.revision) {
                self.inflight = None;
            }
        }
        latest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::fixtures::GraphFixtures;
    use std::sync::Arc;

    use kernel::geometry::PortablePoint;
    use kernel::graph::Graph;

    #[test]
    fn actor_round_trips_a_partition_with_its_revision() {
        // Two triangles {0,1,2} and {3,4,5} joined by a bridge => two communities.
        let mut graph = Graph::new();
        let n: Vec<_> = (0..6)
            .map(|i| {
                graph.add_node(
                    format!("https://{i}.example"),
                    PortablePoint::new(i as f32, 0.0),
                )
            })
            .collect();
        for &(a, b) in &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)] {
            graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
        }

        let wake: Wake = Arc::new(|| {});
        let mut actor = CommunityActor::spawn(wake);
        actor.request(CommunitySnapshot::from_graph(&graph), 42);

        // Spin until the worker emits (sub-millisecond; bounded so a regression fails rather than
        // hangs). The result carries the revision it was requested for, and the right partition.
        let mut update = None;
        for _ in 0..2_000_000 {
            if let Some(u) = actor.drain() {
                update = Some(u);
                break;
            }
            std::thread::yield_now();
        }
        let update = update.expect("the worker emitted a partition");
        assert_eq!(
            update.revision, 42,
            "the result carries its request revision"
        );
        assert_eq!(
            update.clusters.clusters.len(),
            2,
            "two triangles => two communities"
        );
    }
}
