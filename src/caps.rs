//! Capability traits.
//!
//! A node needs exactly one thing to live in a [`Graph`](crate::Graph): a stable
//! [`Identified`] identity. Everything else is a *capability* a payload opts into,
//! and each capability unlocks a feature:
//!
//! | trait | on | unlocks |
//! | --- | --- | --- |
//! | [`Addressed`] | node | multi-scheme address lookup, the RDF `@id` |
//! | [`ContentBearing`] | node | a content-addressed body (a muniment blob) |
//! | [`Labeled`] | node | title + tags, the curated RDF literals |
//! | [`Classified`] | edge | relation-class filtering and render policy |
//! | [`Predicated`] | edge | a predicate IRI: the edge joins the semantic ring |
//!
//! The provided [`Container`](crate::Container) / [`Relation`](crate::Relation)
//! payloads implement all of these. An app that needs more implements the traits
//! on its own struct instead (mere's web node does this).

use std::fmt::Debug;
use std::hash::Hash;

use muniment::Hash as ContentHash;

use crate::taxonomy::RelationClass;

/// The one required node capability: a stable identity that outlives any single
/// graph key and survives serialization.
///
/// The graph maps `Id` to its internal key so a node can be found by identity, not
/// just by position. mere's `id: Uuid` is the canonical instance; the default
/// [`Container`](crate::Container) uses a `String`, so a caller decides what
/// identity means (a URN, a slug, a content hash).
pub trait Identified {
    /// The stable identity type.
    type Id: Clone + Eq + Hash + Debug;

    /// This node's stable identity.
    fn id(&self) -> &Self::Id;
}

/// A scheme-qualified address (`https://…`, `gemini://…`, `file://…`, `mere://…`).
/// The scheme is the substring before the first `:`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Address(pub String);

impl Address {
    /// Wrap an address string.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// The scheme (before the first `:`), or `None` if there is none.
    pub fn scheme(&self) -> Option<&str> {
        self.0.split_once(':').map(|(scheme, _)| scheme)
    }

    /// The full address string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A node that carries one or more addresses across schemes. The first is the
/// primary (canonical) address; the rest are aliases.
pub trait Addressed {
    /// Every address, primary first.
    fn addresses(&self) -> &[Address];

    /// The canonical address, or `None` if the node has none.
    fn primary_address(&self) -> Option<&Address> {
        self.addresses().first()
    }
}

/// A node whose content is a content-addressed blob (stored in a muniment
/// `BlobStore`), plus an optional media-type hint.
pub trait ContentBearing {
    /// The content hash, or `None` if the node has no body yet.
    fn content(&self) -> Option<ContentHash>;

    /// The media type of the content, if known (`text/markdown`, `image/png`).
    fn media_type(&self) -> Option<&str>;
}

/// A node with a human-facing title and semantic tags.
pub trait Labeled {
    /// The display title, if any.
    fn title(&self) -> Option<&str>;

    /// The node's tags.
    fn tags(&self) -> &[String];
}

/// An edge classified by its relation ([`RelationClass`]), for filtering and
/// render policy without touching the edge payload.
pub trait Classified {
    /// This edge's relation class.
    fn class(&self) -> &RelationClass;
}

/// An edge that projects to an RDF predicate. Returns the predicate IRI for a
/// semantic-ring relation, or `None` for an app-private (experience-layer)
/// relation that does not project.
pub trait Predicated {
    /// The predicate IRI, or `None` if this relation is not part of the semantic
    /// ring.
    fn predicate(&self) -> Option<&str>;
}
