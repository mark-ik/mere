//! chartulary (aka **chart**) — the generic content-addressed container graph.
//!
//! A chartulary is the register a house kept its charters and muniments in. This
//! crate is that register for an app's nodes and their relations: a
//! [`Graph<N, E>`] where nodes are content-addressed containers and edges are
//! typed relations, over one shared, app-agnostic model.
//!
//! The design is fully generic. A node needs exactly one capability,
//! [`Identified`], to live in the graph; everything else is an opt-in trait that
//! unlocks a feature ([`Addressed`], [`ContentBearing`], [`Labeled`] on nodes,
//! [`Classified`] and [`Predicated`] on edges). The provided [`Container`] and
//! [`Relation`] payloads implement them all, so an app can start immediately;
//! mere's web node and isometry's entity implement the traits on their own types
//! instead.
//!
//! chartulary sits above muniment (a node's content is a muniment blob, referenced
//! by hash) and codicil (graph edits are a codicil log; the graph is the replay).
//! Relations come in two rings: a shared [`Semantic`] ring that projects to RDF,
//! and app-private families that do not (see [`taxonomy`]).
//!
//! G0 is the generic core, the capability traits, the default payloads, and the
//! two-ring taxonomy. **G1** adds the edit spine ([`GraphLog`]): graph mutations
//! are [`GraphEdit`] entries in a codicil, the graph is the replay, and muniment
//! snapshots give checkpoint-plus-tail loading. Lineage (stemma) and the RDF
//! projection (scholia) are later phases. The canonical plan is mere's
//! `design_docs/.../2026-07-08_generic_graph_substrate_plan.md`.

pub mod caps;
pub mod container;
pub mod edit;
pub mod graph;
pub mod spine;
pub mod taxonomy;

pub use caps::{
    Address, Addressed, Classified, ContentBearing, Identified, Labeled, Predicated,
};
pub use container::{Container, Relation};
pub use edit::{DerivationKind, DerivationRecord, EdgeId, GraphEdit};
pub use graph::{EdgeKey, Graph, NodeKey};
pub use spine::GraphLog;
pub use taxonomy::{Recognized, RelationClass, Semantic, REL_NS};

// Re-exported so a consumer can fork and inspect provenance without depending on
// codicil directly.
pub use codicil::{LogId, Provenance};
