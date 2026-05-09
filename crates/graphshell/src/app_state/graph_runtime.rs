/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pure graph-runtime reducer slice for `GraphWorkspace`.
//!
//! This is the first portable subset of the donor `graph_runtime` seam. It
//! applies durable node-level graph changes through `mere-kernel::graph`
//! APIs and emits mutation-journal effects for services to persist later.

use mere_kernel::geometry::PortablePoint;
use mere_kernel::graph::NodeKey;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{GraphMutationRecord, GraphWorkspace, MutationPayload, WorkspaceEffect};

/// Durable graph-runtime intent accepted by [`reduce_graph_runtime_intent`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GraphRuntimeIntent {
    AddNode {
        id: Uuid,
        url: String,
        position: PortablePoint,
    },
    RemoveNode {
        node: NodeKey,
    },
    SetNodeTitle {
        node: NodeKey,
        title: String,
    },
    SetNodeUrl {
        node: NodeKey,
        url: String,
    },
    SetNodePinned {
        node: NodeKey,
        pinned: bool,
    },
    InsertNodeTag {
        node: NodeKey,
        tag: String,
    },
    RemoveNodeTag {
        node: NodeKey,
        tag: String,
    },
}

impl GraphRuntimeIntent {
    fn label(&self) -> &'static str {
        match self {
            Self::AddNode { .. } => "graph.add_node",
            Self::RemoveNode { .. } => "graph.remove_node",
            Self::SetNodeTitle { .. } => "graph.set_node_title",
            Self::SetNodeUrl { .. } => "graph.set_node_url",
            Self::SetNodePinned { .. } => "graph.set_node_pinned",
            Self::InsertNodeTag { .. } => "graph.insert_node_tag",
            Self::RemoveNodeTag { .. } => "graph.remove_node_tag",
        }
    }
}

/// Summary of one graph-runtime reducer application.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphRuntimeOutcome {
    pub state_changed: bool,
    pub effects_emitted: usize,
    pub node: Option<NodeKey>,
}

/// Pure graph-runtime validation error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphRuntimeError {
    MissingNode(NodeKey),
    DuplicateNodeId(Uuid),
    MutationPayload(String),
}

pub type GraphRuntimeResult = Result<GraphRuntimeOutcome, GraphRuntimeError>;

/// Apply one durable graph-runtime intent.
pub fn reduce_graph_runtime_intent(
    workspace: &mut GraphWorkspace,
    intent: GraphRuntimeIntent,
) -> GraphRuntimeResult {
    let label = intent.label();
    let intent_for_journal = intent.clone();
    let changed_node = match intent {
        GraphRuntimeIntent::AddNode { id, url, position } => {
            if workspace.domain.graph.get_node_by_id(id).is_some() {
                return Err(GraphRuntimeError::DuplicateNodeId(id));
            }
            Some(workspace.domain.graph.add_node_with_id(id, url, position))
        }
        GraphRuntimeIntent::RemoveNode { node } => {
            if workspace.domain.graph.remove_node(node) {
                Some(node)
            } else {
                return Err(GraphRuntimeError::MissingNode(node));
            }
        }
        GraphRuntimeIntent::SetNodeTitle { node, title } => {
            if workspace.domain.graph.get_node(node).is_none() {
                return Err(GraphRuntimeError::MissingNode(node));
            }
            workspace
                .domain
                .graph
                .set_node_title(node, title)
                .then_some(node)
        }
        GraphRuntimeIntent::SetNodeUrl { node, url } => {
            let Some(existing) = workspace.domain.graph.get_node(node) else {
                return Err(GraphRuntimeError::MissingNode(node));
            };
            if existing.url() == url {
                None
            } else {
                workspace.domain.graph.update_node_url(node, url);
                Some(node)
            }
        }
        GraphRuntimeIntent::SetNodePinned { node, pinned } => {
            if workspace.domain.graph.get_node(node).is_none() {
                return Err(GraphRuntimeError::MissingNode(node));
            }
            workspace
                .domain
                .graph
                .set_node_pinned(node, pinned)
                .then_some(node)
        }
        GraphRuntimeIntent::InsertNodeTag { node, tag } => {
            if workspace.domain.graph.get_node(node).is_none() {
                return Err(GraphRuntimeError::MissingNode(node));
            }
            workspace
                .domain
                .graph
                .insert_node_tag(node, tag)
                .then_some(node)
        }
        GraphRuntimeIntent::RemoveNodeTag { node, tag } => {
            if workspace.domain.graph.get_node(node).is_none() {
                return Err(GraphRuntimeError::MissingNode(node));
            }
            workspace
                .domain
                .graph
                .remove_node_tag(node, &tag)
                .then_some(node)
        }
    };

    let Some(node) = changed_node else {
        return Ok(GraphRuntimeOutcome {
            state_changed: false,
            effects_emitted: 0,
            node: None,
        });
    };

    let payload = serde_json::to_string(&intent_for_journal)
        .map(MutationPayload::TypedJson)
        .map_err(|error| GraphRuntimeError::MutationPayload(error.to_string()))?;
    let sequence = workspace.domain.next_mutation_sequence();
    workspace.push_effect(WorkspaceEffect::AppendGraphMutation(GraphMutationRecord {
        sequence,
        label: label.to_string(),
        payload,
    }));

    Ok(GraphRuntimeOutcome {
        state_changed: true,
        effects_emitted: 1,
        node: Some(node),
    })
}

#[cfg(test)]
mod tests {
    use mere_kernel::geometry::PortablePoint;

    use super::*;
    use crate::app_state::{MutationPayload, WorkspaceEffect};

    fn node_id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    #[test]
    fn add_node_mutates_graph_and_emits_journal_effect() {
        let mut workspace = GraphWorkspace::new();
        let id = node_id(1);

        let outcome = reduce_graph_runtime_intent(
            &mut workspace,
            GraphRuntimeIntent::AddNode {
                id,
                url: "https://example.test".to_string(),
                position: PortablePoint::new(10.0, 20.0),
            },
        )
        .unwrap();

        assert!(outcome.state_changed);
        assert_eq!(outcome.effects_emitted, 1);
        assert_eq!(workspace.domain.graph.node_count(), 1);
        let effects = workspace.drain_effects();
        assert_eq!(effects.len(), 1);
        assert!(matches!(
            effects[0],
            WorkspaceEffect::AppendGraphMutation(_)
        ));
    }

    #[test]
    fn no_op_title_update_does_not_emit_journal_effect() {
        let mut workspace = GraphWorkspace::new();
        let node = reduce_graph_runtime_intent(
            &mut workspace,
            GraphRuntimeIntent::AddNode {
                id: node_id(2),
                url: "https://example.test".to_string(),
                position: PortablePoint::new(0.0, 0.0),
            },
        )
        .unwrap()
        .node
        .unwrap();
        workspace.drain_effects();

        let outcome = reduce_graph_runtime_intent(
            &mut workspace,
            GraphRuntimeIntent::SetNodeTitle {
                node,
                title: "https://example.test".to_string(),
            },
        )
        .unwrap();

        assert!(!outcome.state_changed);
        assert!(workspace.pending_effects.effects.is_empty());
    }

    #[test]
    fn tag_mutation_records_stable_sequence() {
        let mut workspace = GraphWorkspace::new();
        let node = reduce_graph_runtime_intent(
            &mut workspace,
            GraphRuntimeIntent::AddNode {
                id: node_id(3),
                url: "https://example.test".to_string(),
                position: PortablePoint::new(0.0, 0.0),
            },
        )
        .unwrap()
        .node
        .unwrap();
        workspace.drain_effects();

        reduce_graph_runtime_intent(
            &mut workspace,
            GraphRuntimeIntent::InsertNodeTag {
                node,
                tag: "reading".to_string(),
            },
        )
        .unwrap();

        let effects = workspace.drain_effects();
        let WorkspaceEffect::AppendGraphMutation(record) = &effects[0] else {
            panic!("expected journal effect");
        };
        assert_eq!(record.sequence, 1);
        assert_eq!(record.label, "graph.insert_node_tag");
        assert!(matches!(record.payload, MutationPayload::TypedJson(_)));
    }

    #[test]
    fn missing_node_update_is_rejected_without_effects() {
        let mut workspace = GraphWorkspace::new();
        let missing = NodeKey::new(999);

        let error = reduce_graph_runtime_intent(
            &mut workspace,
            GraphRuntimeIntent::SetNodePinned {
                node: missing,
                pinned: true,
            },
        )
        .unwrap_err();

        assert_eq!(error, GraphRuntimeError::MissingNode(missing));
        assert!(workspace.pending_effects.effects.is_empty());
    }
}
