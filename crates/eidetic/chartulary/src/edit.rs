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
use serde_json::Value;

use crate::caps::Identified;
use crate::facet::FacetId;

/// Which replica minted an [`EdgeId`].
///
/// Opaque 32 bytes. Chartulary never interprets them; it only needs distinct
/// replicas of one container to differ. **Bind this to the replication layer's
/// identity** (the p2panda verifying key), not to [`Author`], which is a
/// caller-chosen display label such as `"ui"` and carries no uniqueness.
///
/// [`Author`]: crate::commit::Author
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct WriterId(pub [u8; 32]);

impl WriterId {
    /// The single-writer identity: all zeros.
    ///
    /// What [`GraphLog::new`](crate::GraphLog::new) uses, and what a legacy
    /// `EdgeId` deserializes to. Correct for any graph with one writer, which
    /// is every graph predating multi-writer containers.
    pub const LOCAL: Self = Self([0u8; 32]);

    /// Whether this is the single-writer identity.
    pub fn is_local(&self) -> bool {
        *self == Self::LOCAL
    }
}

/// A stable edge identity, assigned when the edge is first connected and carried
/// in the [`GraphEdit::Connect`] entry, so it survives replay and can address the
/// edge for retraction.
///
/// The identity is `(writer, counter)`: each replica mints from its own
/// monotonic counter, and uniqueness across replicas comes from the writer half.
/// A bare counter (what this was through 0.1.x) collides the moment one
/// container has two writers, because two partitioned replicas both mint from
/// zero; no merge ordering can repair that, since the collision is in the
/// addressing space rather than the operation order. This is the same shape
/// p2panda uses for operation identity, author plus sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct EdgeId {
    /// The replica that minted this id.
    pub writer: WriterId,
    /// That replica's monotonic counter at mint time.
    pub counter: u64,
}

impl EdgeId {
    /// An id minted by `writer` at `counter`.
    pub fn new(writer: WriterId, counter: u64) -> Self {
        Self { writer, counter }
    }

    /// A single-writer id, for graphs with one writer and for tests.
    pub fn local(counter: u64) -> Self {
        Self::new(WriterId::LOCAL, counter)
    }
}

/// Reads both the current `{writer, counter}` form and the bare integer 0.1.x
/// wrote, so existing single-writer journals and snapshots keep loading; a
/// legacy id becomes [`WriterId::LOCAL`], which is what it always meant.
///
/// Requires a self-describing format. Every chartulary store in the tree is
/// JSON today; a non-self-describing codec (postcard) would need the legacy
/// data converted rather than sniffed.
impl<'de> Deserialize<'de> for EdgeId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EdgeIdVisitor;

        impl<'de> serde::de::Visitor<'de> for EdgeIdVisitor {
            type Value = EdgeId;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an edge id: {writer, counter}, or a bare 0.1.x counter")
            }

            fn visit_u64<E: serde::de::Error>(self, counter: u64) -> Result<EdgeId, E> {
                Ok(EdgeId::local(counter))
            }

            fn visit_i64<E: serde::de::Error>(self, counter: i64) -> Result<EdgeId, E> {
                u64::try_from(counter)
                    .map(EdgeId::local)
                    .map_err(|_| E::custom("negative edge counter"))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<EdgeId, A::Error> {
                let mut writer = None;
                let mut counter = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "writer" => writer = Some(map.next_value()?),
                        "counter" => counter = Some(map.next_value()?),
                        _ => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EdgeId {
                    writer: writer.unwrap_or(WriterId::LOCAL),
                    counter: counter.ok_or_else(|| serde::de::Error::missing_field("counter"))?,
                })
            }
        }

        deserializer.deserialize_any(EdgeIdVisitor)
    }
}

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
    /// Set one independent facet value on a live node.
    SetFacet {
        /// The node carrying the facet.
        node: N::Id,
        /// The stable, namespaced facet identity.
        facet: FacetId,
        /// The schema-agnostic value accepted by the host.
        value: Value,
    },
    /// Remove one independent facet from a live node.
    RemoveFacet {
        /// The node carrying the facet.
        node: N::Id,
        /// The stable, namespaced facet identity.
        facet: FacetId,
    },
}
