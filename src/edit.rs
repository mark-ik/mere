/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Graph edits: the entries of the edit spine.
//!
//! A [`GraphEdit`] is one mutation. A [`Codicil`](codicil::Codicil) of them is the
//! graph's history; replaying it materializes the graph. Edits reference nodes by
//! their **stable identity** ([`Identified::Id`]), never by an ephemeral graph key,
//! so replay into a fresh graph reconstructs the same result. Edges get a stable
//! [`EdgeId`] at connect time (assigned by the [`GraphLog`](crate::GraphLog) and
//! carried in the edit) so a specific edge can be retracted across replay.

use codicil::LogId;
use serde::{Deserialize, Serialize};

use crate::caps::Identified;

/// A stable edge identity, assigned when the edge is first connected and carried
/// in the [`GraphEdit::Connect`] entry, so it survives replay and can address the
/// edge for retraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EdgeId(pub u64);

/// How a node was derived from another graph's node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DerivationKind {
    /// A verbatim copy (tear-out): the content is unchanged, only the identity and
    /// host graph differ.
    CopiedFrom,
    /// An excerpt or clip of the source.
    ClippedFrom,
    /// Generated from the source (a summary, an answer).
    GeneratedFrom,
    /// A translation of the source.
    TranslatedFrom,
}

/// A record that a node in this graph derives from a node in another graph. The
/// node-level provenance that tracks duplicates across graphs, rather than
/// deduplicating them (the whole-graph fork provenance lives on the log; see
/// [`codicil::Provenance`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivationRecord<Id> {
    /// The identity of the graph (log) the source node lives in.
    pub source_log: LogId,
    /// The source node's identity within that graph.
    pub source_node: Id,
    /// How this node relates to the source.
    pub kind: DerivationKind,
}

/// One mutation of a graph. The unit stored in the edit spine.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "N: Serialize, N::Id: Serialize, E: Serialize",
    deserialize = "N: Deserialize<'de>, N::Id: Deserialize<'de>, E: Deserialize<'de>"
))]
pub enum GraphEdit<N: Identified, E> {
    /// Insert (or upsert, by identity) a node.
    InsertNode(N),
    /// Remove the node with this identity, and its incident edges.
    RemoveNode(N::Id),
    /// Connect two nodes (by identity) with an edge payload, under a stable id.
    Connect {
        /// The stable id assigned to this edge.
        id: EdgeId,
        /// The source node's identity.
        from: N::Id,
        /// The target node's identity.
        to: N::Id,
        /// The edge payload.
        edge: E,
    },
    /// Retract the edge with this stable id.
    Disconnect(EdgeId),
    /// Record that a node derives from a node in another graph.
    Derive {
        /// The node in this graph.
        node: N::Id,
        /// Where it came from.
        from: DerivationRecord<N::Id>,
    },
}
