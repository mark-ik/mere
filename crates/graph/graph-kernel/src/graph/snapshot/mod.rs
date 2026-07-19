// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Graph snapshot serialization.
//!
//! Originally a single `graph/snapshot.rs` (724 LOC); split per the
//! 2026-05-11 kernel decomposition pass into three sub-modules:
//!
//! - [`to`] — `Graph::to_snapshot`
//! - [`from`] — `Graph::from_snapshot`
//!
//! This `mod.rs` keeps the small shared pieces: the
//! `remove_url_mapping` helper (used by both directions),
//! [`containment_parent_url`] (used by `from_snapshot` and by graph
//! mutators in `mod.rs`'s impl block), and the rkyv trait impls
//! (`Archive` / `Serialize` / `Deserialize`) plus `impl Default for
//! Graph`, all of which are short standalone items.

use rkyv::{Archive, Archived, Deserialize, Place, Resolver, Serialize, rancor::Fallible};

use super::Graph;
use super::identity::NodeKey;
use crate::persistence::GraphSnapshot;

pub mod from;
pub mod to;

impl Graph {
    pub(crate) fn remove_url_mapping(&mut self, url: &str, key: NodeKey) {
        if let Some(keys) = self.url_to_nodes.get_mut(url) {
            keys.retain(|candidate| *candidate != key);
            if keys.is_empty() {
                self.url_to_nodes.remove(url);
            }
        }
    }
}

pub(crate) fn containment_parent_url(url: &url::Url) -> Option<String> {
    if !matches!(url.scheme(), "http" | "https" | "file") {
        return None;
    }

    let mut parent = url.clone();
    parent.set_query(None);
    parent.set_fragment(None);

    let mut segments: Vec<String> = parent
        .path_segments()
        .map(|parts| {
            parts
                .filter(|segment| !segment.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if segments.is_empty() {
        return None;
    }
    segments.pop();

    let parent_path = if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}/", segments.join("/"))
    };

    parent.set_path(&parent_path);
    Some(parent.to_string())
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Archive for Graph {
    type Archived = Archived<GraphSnapshot>;
    type Resolver = Resolver<GraphSnapshot>;

    fn resolve(&self, resolver: Self::Resolver, out: Place<Self::Archived>) {
        let snapshot = self.to_snapshot();
        snapshot.resolve(resolver, out);
    }
}

impl<S> Serialize<S> for Graph
where
    S: Fallible + ?Sized,
    GraphSnapshot: Serialize<S>,
{
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let snapshot = self.to_snapshot();
        snapshot.serialize(serializer)
    }
}

impl<D> Deserialize<Graph, D> for Archived<GraphSnapshot>
where
    D: Fallible + ?Sized,
    Archived<GraphSnapshot>: Deserialize<GraphSnapshot, D>,
{
    fn deserialize(&self, deserializer: &mut D) -> Result<Graph, D::Error> {
        let snapshot = <Archived<GraphSnapshot> as Deserialize<GraphSnapshot, D>>::deserialize(
            self,
            deserializer,
        )?;
        Ok(Graph::from_snapshot(&snapshot))
    }
}
