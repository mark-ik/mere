//! The provided default payloads: [`Container`] and [`Relation`].
//!
//! These implement every capability trait, so an app can build a real graph
//! without designing its own node and edge types. An app that needs more (mere's
//! web node, isometry's entity) implements the traits on its own struct and
//! instantiates `Graph` with that instead.

use muniment::Hash as ContentHash;
use serde::{Deserialize, Serialize};

use crate::caps::{Address, Addressed, Classified, ContentBearing, Identified, Labeled, Predicated};
use crate::taxonomy::RelationClass;

/// A default content-addressed container node.
///
/// Identity is a caller-chosen `String` (a URN, a slug, a content-hash hex). The
/// content body lives out-of-line as a muniment blob referenced by
/// [`content`](Container::content); only the hash travels in the graph.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Container {
    /// The stable identity.
    pub id: String,
    /// Addresses across schemes, primary first.
    pub addresses: Vec<Address>,
    /// The content-addressed body (a muniment blob hash), if any.
    pub content: Option<ContentHash>,
    /// The media type of the content, if known.
    pub media_type: Option<String>,
    /// A display title.
    pub title: Option<String>,
    /// Semantic tags.
    pub tags: Vec<String>,
}

impl Container {
    /// A container with only an identity. Fill the rest with the builder methods
    /// or by assigning fields.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }

    /// Add an address (the first added becomes primary).
    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        self.addresses.push(Address::new(address));
        self
    }

    /// Set the content hash and media type.
    pub fn with_content(mut self, hash: ContentHash, media_type: impl Into<String>) -> Self {
        self.content = Some(hash);
        self.media_type = Some(media_type.into());
        self
    }

    /// Set the title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

impl Identified for Container {
    type Id = String;
    fn id(&self) -> &String {
        &self.id
    }
}

impl Addressed for Container {
    fn addresses(&self) -> &[Address] {
        &self.addresses
    }
}

impl ContentBearing for Container {
    fn content(&self) -> Option<ContentHash> {
        self.content
    }
    fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }
}

impl Labeled for Container {
    fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    fn tags(&self) -> &[String] {
        &self.tags
    }
}

/// A default relation edge: a [`RelationClass`] plus an optional human label.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    /// The relation's class (semantic-ring or app-private).
    pub class: RelationClass,
    /// An optional human-facing label for this specific edge.
    pub label: Option<String>,
}

impl Relation {
    /// A relation of the given class, unlabeled.
    pub fn new(class: RelationClass) -> Self {
        Self { class, label: None }
    }

    /// Attach a label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Classified for Relation {
    fn class(&self) -> &RelationClass {
        &self.class
    }
}

impl Predicated for Relation {
    fn predicate(&self) -> Option<&str> {
        self.class.predicate()
    }
}
