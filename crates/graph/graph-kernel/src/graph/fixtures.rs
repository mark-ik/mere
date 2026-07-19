// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test-fixture escape hatch over the `pub(crate)` graph mutators (write-path
//! migration, 2026-07-01).
//!
//! Production code routes every primitive durable mutation through
//! [`apply_graph_delta`](super::apply::apply_graph_delta) — the Phase 6.5
//! single-write-path boundary, enforced by `pub(crate)` visibility on the raw
//! mutators. Test fixtures building sample graphs would gain nothing from that
//! routing (they are not runtime paths and will never be event-logged), so this
//! extension trait re-exposes the raw mutators to them with call sites
//! unchanged: enable the kernel `fixtures` cargo feature in `dev-dependencies`
//! and `use kernel::graph::fixtures::GraphFixtures;` in the test module.
//!
//! The feature must never be enabled by a non-dev dependency — that would
//! reopen the boundary this module deliberately tunnels through for tests.

use euclid::default::Point2D;
use uuid::Uuid;

use super::edge_payload::EdgePayload;
use super::{Coupling, EdgeAssertion, EdgeKey, Field, Graph, Node, NodeKey, RelationSelector};

/// Raw-mutator access for test fixtures. Every method delegates to the
/// `pub(crate)` inherent mutator of the same name.
pub trait GraphFixtures {
    #[cfg(not(target_arch = "wasm32"))]
    fn add_node(&mut self, url: String, position: Point2D<f32>) -> NodeKey;
    fn add_node_with_id(&mut self, id: Uuid, url: String, position: Point2D<f32>) -> NodeKey;
    fn remove_node(&mut self, key: NodeKey) -> bool;
    fn assert_relation(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        assertion: EdgeAssertion,
    ) -> Option<EdgeKey>;
    fn assert_semantic_predicate(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        predicate: String,
    ) -> Option<EdgeKey>;
    fn retract_relations(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        selector: RelationSelector,
    ) -> usize;
    fn navigate_node(&mut self, key: NodeKey, url: &str);
    fn branch_history(&mut self, child: NodeKey, parent: NodeKey);
    fn node_history_back(&mut self, key: NodeKey) -> Option<String>;
    fn node_history_forward(&mut self, key: NodeKey) -> Option<String>;
    fn insert_node_tag(&mut self, key: NodeKey, tag: String) -> bool;
    fn remove_node_tag(&mut self, key: NodeKey, tag: &str) -> bool;
    fn set_node_title(&mut self, key: NodeKey, title: String) -> bool;
    fn set_node_body(&mut self, key: NodeKey, body: Option<String>) -> bool;
    fn set_node_mime_hint(&mut self, key: NodeKey, mime_hint: Option<String>) -> bool;
    fn set_node_thumbnail(
        &mut self,
        key: NodeKey,
        png_bytes: Vec<u8>,
        width: u32,
        height: u32,
    ) -> bool;
    fn set_node_favicon(&mut self, key: NodeKey, rgba: Vec<u8>, width: u32, height: u32) -> bool;
    fn add_field(&mut self, field: Field);
    fn add_coupling(&mut self, coupling: Coupling);
    fn get_node_mut(&mut self, key: NodeKey) -> Option<&mut Node>;
    fn get_edge_mut(&mut self, key: EdgeKey) -> Option<&mut EdgePayload>;
}

impl GraphFixtures for Graph {
    #[cfg(not(target_arch = "wasm32"))]
    fn add_node(&mut self, url: String, position: Point2D<f32>) -> NodeKey {
        Graph::add_node(self, url, position)
    }
    fn add_node_with_id(&mut self, id: Uuid, url: String, position: Point2D<f32>) -> NodeKey {
        Graph::add_node_with_id(self, id, url, position)
    }
    fn remove_node(&mut self, key: NodeKey) -> bool {
        Graph::remove_node(self, key)
    }
    fn assert_relation(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        assertion: EdgeAssertion,
    ) -> Option<EdgeKey> {
        Graph::assert_relation(self, from, to, assertion)
    }
    fn assert_semantic_predicate(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        predicate: String,
    ) -> Option<EdgeKey> {
        Graph::assert_semantic_predicate(self, from, to, predicate)
    }
    fn retract_relations(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        selector: RelationSelector,
    ) -> usize {
        Graph::retract_relations(self, from, to, selector)
    }
    fn navigate_node(&mut self, key: NodeKey, url: &str) {
        Graph::navigate_node(self, key, url)
    }
    fn branch_history(&mut self, child: NodeKey, parent: NodeKey) {
        Graph::branch_history(self, child, parent)
    }
    fn node_history_back(&mut self, key: NodeKey) -> Option<String> {
        Graph::node_history_back(self, key)
    }
    fn node_history_forward(&mut self, key: NodeKey) -> Option<String> {
        Graph::node_history_forward(self, key)
    }
    fn insert_node_tag(&mut self, key: NodeKey, tag: String) -> bool {
        Graph::insert_node_tag(self, key, tag)
    }
    fn remove_node_tag(&mut self, key: NodeKey, tag: &str) -> bool {
        Graph::remove_node_tag(self, key, tag)
    }
    fn set_node_title(&mut self, key: NodeKey, title: String) -> bool {
        Graph::set_node_title(self, key, title)
    }
    fn set_node_body(&mut self, key: NodeKey, body: Option<String>) -> bool {
        Graph::set_node_body(self, key, body)
    }
    fn set_node_mime_hint(&mut self, key: NodeKey, mime_hint: Option<String>) -> bool {
        Graph::set_node_mime_hint(self, key, mime_hint)
    }
    fn set_node_thumbnail(
        &mut self,
        key: NodeKey,
        png_bytes: Vec<u8>,
        width: u32,
        height: u32,
    ) -> bool {
        Graph::set_node_thumbnail(self, key, png_bytes, width, height)
    }
    fn set_node_favicon(&mut self, key: NodeKey, rgba: Vec<u8>, width: u32, height: u32) -> bool {
        Graph::set_node_favicon(self, key, rgba, width, height)
    }
    fn add_field(&mut self, field: Field) {
        Graph::add_field(self, field)
    }
    fn add_coupling(&mut self, coupling: Coupling) {
        Graph::add_coupling(self, coupling)
    }
    fn get_node_mut(&mut self, key: NodeKey) -> Option<&mut Node> {
        Graph::get_node_mut(self, key)
    }
    fn get_edge_mut(&mut self, key: EdgeKey) -> Option<&mut EdgePayload> {
        Graph::get_edge_mut(self, key)
    }
}
