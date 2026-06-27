/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Graph-structural **signal producer** — the graph-signals-layer plan's `intel/signals`.
//!
//! Computes per-node / per-pair signals from the kernel [`Graph`] and hands them to
//! cartography's narrow [`IntelligenceSignals`] contract. This crate OWNS the production
//! lifecycle; cartography keeps only the contract (so cartography never depends on a producer).
//!
//! First signal: **degree-based importance** — a cheap, synchronous signal computed inline. The
//! generation + per-signal dirty-bit **cache** (that gates recomputation and backgrounds the
//! expensive signals: betweenness / communities / affinity) and those richer signals land in
//! later slices; this slice is the spine (producer -> snapshot -> `project_orrery_strategy`).
//! (Graph signals — P1.)

use std::collections::{HashMap, VecDeque};

use cartography::{ImportanceWeights, IntelligenceSignals};
use kernel::graph::{Graph, NodeKey};

pub use cartography::{AffinityScores, BridgeNodes, Cluster, ClusterSet, Overlay};

mod affinity;
mod bridges;
mod community;
mod importance;

pub use affinity::*;
pub use bridges::*;
pub use community::*;
pub use importance::*;

#[cfg(test)]
mod tests;

/// Produce the cheap, synchronous signal snapshot for `graph`: degree-based importance. The
/// other contract fields (clusters / affinity / bridges / embeddings) stay `None` until their
/// producers land. Recomputed on call — degree is cheap enough to run inline; the cache that
/// gates recomputation is a later slice. (Graph signals — P1, the spine.)
pub fn produce_cheap_signals(graph: &Graph) -> IntelligenceSignals {
    IntelligenceSignals {
        importance: Some(degree_importance(graph)),
        ..IntelligenceSignals::default()
    }
}
