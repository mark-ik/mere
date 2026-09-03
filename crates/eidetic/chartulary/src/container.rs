// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The provided default payloads: [`Container`] and [`Relation`].
//!
//! These implement every capability trait, so an app can build a real graph
//! without designing its own node and edge types. An app that needs more (mere's
//! web node, isometry's entity) implements the traits on its own struct and
//! instantiates `Graph` with that instead.

use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;

use muniment::Hash as ContentHash;
use muniment::LogId;
use serde::{Deserialize, Serialize};

use crate::caps::{
    Address, Addressed, Classified, ContentBearing, GraphBearing, Identified, Labeled, Predicated,
};
use crate::taxonomy::RelationClass;

/// An address representation that can project into chartulary's common
/// [`Address`] vocabulary.
///
/// The default container stores [`Address`] directly. A host whose address
/// representation carries more information can implement this trait for that
/// local type and still use the same [`Container`].
pub trait ContainerAddress: Clone {
    /// Project this stored address into the shared address vocabulary.
    fn to_address(&self) -> Address;
}

impl ContainerAddress for Address {
    fn to_address(&self) -> Address {
        self.clone()
    }
}

/// A default content-addressed container node.
///
/// `Id` and `A` are host-selectable so an application can use its real stable
/// identity and address representation without wrapping the container in a
/// second node type. The defaults retain the original convenient
/// `String`/[`Address`] shape.
///
/// Durable content normally lives out of line as a muniment blob referenced by
/// [`content`](Container::content). Small authored text may instead ride
/// inline in [`body`](Container::body); [`ContentBearing`] still projects a
/// content hash for it. This is intentionally one container capability, not a
/// web-node exception.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Container<Id = String, A = Address> {
    /// The stable identity.
    pub id: Id,
    /// Addresses across schemes, primary first.
    pub addresses: Vec<A>,
    /// The content-addressed body (a muniment blob hash), if any.
    pub content: Option<ContentHash>,
    /// Small authored text carried inline. Mutually exclusive with `content`
    /// through the builders; hosts migrating legacy data may normalize it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The media type of the content, if known.
    pub media_type: Option<String>,
    /// A display title. Empty means untitled.
    #[serde(default, deserialize_with = "deserialize_title")]
    pub title: String,
    /// Semantic tags.
    #[serde(default)]
    pub tags: HashSet<String>,
    /// The log identity of a nested graph contained within this node, if any.
    /// Absent in pre-nesting data, so old slots load unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nested: Option<LogId>,
}

fn deserialize_title<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

impl<Id: Default, A> Default for Container<Id, A> {
    fn default() -> Self {
        Self::with_identity(Id::default())
    }
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
}

impl<Id, A> Container<Id, A> {
    /// A container with a host-native identity and address representation.
    pub fn with_identity(id: Id) -> Self {
        Self {
            id,
            addresses: Vec::new(),
            content: None,
            body: None,
            media_type: None,
            title: String::new(),
            tags: HashSet::new(),
            nested: None,
        }
    }

    /// Add one host-native address. The first is primary.
    pub fn with_address_record(mut self, address: A) -> Self {
        self.addresses.push(address);
        self
    }

    /// Set the content hash and media type.
    pub fn with_content(mut self, hash: ContentHash, media_type: impl Into<String>) -> Self {
        self.content = Some(hash);
        self.body = None;
        self.media_type = Some(media_type.into());
        self
    }

    /// Carry small authored text inline.
    pub fn with_inline_text(
        mut self,
        body: impl Into<String>,
        media_type: impl Into<String>,
    ) -> Self {
        self.content = None;
        self.body = Some(body.into());
        self.media_type = Some(media_type.into());
        self
    }

    /// Set the title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Bear a nested graph by its log identity.
    pub fn with_nested(mut self, id: LogId) -> Self {
        self.nested = Some(id);
        self
    }
}

impl<Id, A> Identified for Container<Id, A>
where
    Id: Clone + Eq + Ord + Hash + Debug,
{
    type Id = Id;
    fn id(&self) -> &Id {
        &self.id
    }
}

impl<Id, A> Addressed for Container<Id, A>
where
    A: ContainerAddress,
{
    fn addresses(&self) -> Vec<Address> {
        self.addresses
            .iter()
            .map(ContainerAddress::to_address)
            .collect()
    }
}

impl<Id, A> ContentBearing for Container<Id, A> {
    fn content(&self) -> Option<ContentHash> {
        self.content.or_else(|| {
            self.body
                .as_deref()
                .map(|body| ContentHash::of(body.as_bytes()))
        })
    }
    fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }
}

impl<Id, A> GraphBearing for Container<Id, A> {
    fn nested(&self) -> Option<&LogId> {
        self.nested.as_ref()
    }
}

impl<Id, A> Labeled for Container<Id, A> {
    fn title(&self) -> Option<&str> {
        (!self.title.is_empty()).then_some(self.title.as_str())
    }
    fn tags(&self) -> Vec<String> {
        self.tags.iter().cloned().collect()
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
    fn class(&self) -> RelationClass {
        self.class.clone()
    }
}

impl Predicated for Relation {
    fn predicate(&self) -> Option<&str> {
        self.class.predicate()
    }
}
