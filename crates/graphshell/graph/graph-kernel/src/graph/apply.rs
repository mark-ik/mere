/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use euclid::default::Point2D;
use uuid::Uuid;

use super::{
    EdgeAssertion, EdgeKey, FrameLayoutHint, Graph, NavigationTrigger, NodeKey, RelationSelector,
};

#[derive(Debug, Clone)]
pub enum GraphDelta {
    AddNode {
        id: Option<Uuid>,
        url: String,
        position: Point2D<f32>,
    },
    AssertRelation {
        from: NodeKey,
        to: NodeKey,
        assertion: EdgeAssertion,
    },
    RemoveNode {
        key: NodeKey,
    },
    ReplayAddNodeWithIdIfMissing {
        id: Uuid,
        url: String,
        position: Point2D<f32>,
    },
    ReplayAssertRelationByIds {
        from_id: Uuid,
        to_id: Uuid,
        assertion: EdgeAssertion,
    },
    ReplayRemoveNodeById {
        node_id: Uuid,
    },
    ReplayRetractRelationsByIds {
        from_id: Uuid,
        to_id: Uuid,
        selector: RelationSelector,
    },
    RetractRelations {
        from: NodeKey,
        to: NodeKey,
        selector: RelationSelector,
    },
    /// Append a traversal event to the `from → to` edge (creating the
    /// edge if absent). `timestamp_ms = None` stamps now-via-the-kernel;
    /// `Some(t)` uses the supplied timestamp (importers, replay).
    AppendTraversal {
        from: NodeKey,
        to: NodeKey,
        trigger: NavigationTrigger,
        timestamp_ms: Option<u64>,
    },
    SetNodeTitle {
        key: NodeKey,
        title: String,
    },
    SetNodeUrl {
        key: NodeKey,
        new_url: String,
    },
    SetNodeThumbnail {
        key: NodeKey,
        png_bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    SetNodeFavicon {
        key: NodeKey,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
    SetNodeMimeHint {
        key: NodeKey,
        mime_hint: Option<String>,
    },
    SetNodeViewerOverride {
        key: NodeKey,
        viewer_override: Option<String>,
    },
    SetNodePinned {
        key: NodeKey,
        is_pinned: bool,
    },
    SetNodeCompatMode {
        key: NodeKey,
        compat_mode: bool,
    },
    AppendFrameLayoutHint {
        key: NodeKey,
        hint: FrameLayoutHint,
    },
    RemoveFrameLayoutHint {
        key: NodeKey,
        hint_index: usize,
    },
    MoveFrameLayoutHint {
        key: NodeKey,
        from_index: usize,
        to_index: usize,
    },
    SetFrameSplitOfferSuppressed {
        key: NodeKey,
        suppressed: bool,
    },
    /// Replace a node's persisted navigation history (entries + current
    /// index). The clamping rule is: `current_index` is treated as `0` when
    /// `entries` is empty, otherwise it is clamped to `entries.len() - 1`.
    /// Sole sanctioned write surface for `Node::history`; the underlying
    /// `Graph` setter is `pub(crate)` so this delta is the only way to
    /// mutate the field from outside `mere-kernel`.
    UpdateNodeHistory {
        key: NodeKey,
        entries: Vec<String>,
        current_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphDeltaResult {
    NodeAdded(NodeKey),
    NodeMaybeAdded(Option<NodeKey>),
    EdgeAdded(Option<EdgeKey>),
    NodeRemoved(bool),
    EdgesRemoved(usize),
    TraversalAppended(bool),
    NodeMetadataUpdated(bool),
    NodeUrlUpdated(Option<String>),
}

pub fn apply_graph_delta(graph: &mut Graph, delta: GraphDelta) -> GraphDeltaResult {
    match delta {
        GraphDelta::AddNode { id, url, position } => {
            let key = match id {
                Some(id) => graph.add_node_with_id(id, url, position),
                #[cfg(not(target_arch = "wasm32"))]
                None => graph.add_node(url, position),
                #[cfg(target_arch = "wasm32")]
                None => panic!("AddNode without an explicit id is not supported on WASM"),
            };
            GraphDeltaResult::NodeAdded(key)
        }
        GraphDelta::AssertRelation {
            from,
            to,
            assertion,
        } => GraphDeltaResult::EdgeAdded(graph.assert_relation(from, to, assertion)),
        GraphDelta::RemoveNode { key } => GraphDeltaResult::NodeRemoved(graph.remove_node(key)),
        GraphDelta::ReplayAddNodeWithIdIfMissing { id, url, position } => {
            GraphDeltaResult::NodeMaybeAdded(
                graph.replay_add_node_with_id_if_missing(id, url, position),
            )
        }
        GraphDelta::ReplayAssertRelationByIds {
            from_id,
            to_id,
            assertion,
        } => GraphDeltaResult::EdgeAdded(
            graph.replay_assert_relation_by_ids(from_id, to_id, assertion),
        ),
        GraphDelta::ReplayRemoveNodeById { node_id } => {
            GraphDeltaResult::NodeRemoved(graph.replay_remove_node_by_id(node_id))
        }
        GraphDelta::ReplayRetractRelationsByIds {
            from_id,
            to_id,
            selector,
        } => GraphDeltaResult::EdgesRemoved(
            graph.replay_retract_relations_by_ids(from_id, to_id, selector),
        ),
        GraphDelta::RetractRelations { from, to, selector } => {
            GraphDeltaResult::EdgesRemoved(graph.retract_relations(from, to, selector))
        }
        GraphDelta::AppendTraversal {
            from,
            to,
            trigger,
            timestamp_ms,
        } => GraphDeltaResult::TraversalAppended(graph.append_traversal(
            from,
            to,
            trigger,
            timestamp_ms,
        )),
        GraphDelta::SetNodeTitle { key, title } => {
            GraphDeltaResult::NodeMetadataUpdated(graph.set_node_title(key, title))
        }
        GraphDelta::SetNodeUrl { key, new_url } => {
            GraphDeltaResult::NodeUrlUpdated(graph.update_node_url(key, new_url))
        }
        GraphDelta::SetNodeThumbnail {
            key,
            png_bytes,
            width,
            height,
        } => GraphDeltaResult::NodeMetadataUpdated(
            graph.set_node_thumbnail(key, png_bytes, width, height),
        ),
        GraphDelta::SetNodeFavicon {
            key,
            rgba,
            width,
            height,
        } => {
            GraphDeltaResult::NodeMetadataUpdated(graph.set_node_favicon(key, rgba, width, height))
        }
        GraphDelta::SetNodeMimeHint { key, mime_hint } => {
            GraphDeltaResult::NodeMetadataUpdated(graph.set_node_mime_hint(key, mime_hint))
        }
        GraphDelta::SetNodeViewerOverride {
            key,
            viewer_override,
        } => GraphDeltaResult::NodeMetadataUpdated(
            graph.set_node_viewer_override(key, viewer_override),
        ),
        GraphDelta::SetNodePinned { key, is_pinned } => {
            GraphDeltaResult::NodeMetadataUpdated(graph.set_node_pinned(key, is_pinned))
        }
        GraphDelta::SetNodeCompatMode { key, compat_mode } => {
            GraphDeltaResult::NodeMetadataUpdated(graph.set_node_compat_mode(key, compat_mode))
        }
        GraphDelta::AppendFrameLayoutHint { key, hint } => {
            GraphDeltaResult::NodeMetadataUpdated(graph.append_frame_layout_hint(key, hint))
        }
        GraphDelta::RemoveFrameLayoutHint { key, hint_index } => {
            GraphDeltaResult::NodeMetadataUpdated(
                graph.remove_frame_layout_hint_at(key, hint_index),
            )
        }
        GraphDelta::MoveFrameLayoutHint {
            key,
            from_index,
            to_index,
        } => GraphDeltaResult::NodeMetadataUpdated(
            graph.move_frame_layout_hint(key, from_index, to_index),
        ),
        GraphDelta::SetFrameSplitOfferSuppressed { key, suppressed } => {
            GraphDeltaResult::NodeMetadataUpdated(
                graph.set_frame_split_offer_suppressed(key, suppressed),
            )
        }
        GraphDelta::UpdateNodeHistory {
            key,
            entries,
            current_index,
        } => GraphDeltaResult::NodeMetadataUpdated(graph.set_node_history_state(
            key,
            entries,
            current_index,
        )),
    }
}
