//! Address-access facts projected into Mere's unknown-forward facet store.

use chartulary::{AcceptAll, FacetError, FacetId};
use mere::kernel::graph::{Graph, NodeKey};
use serde::{Deserialize, Serialize};

/// Stable facet id for Graphshell's portable access history.
pub const ACCESS_HISTORY_FACET: &str = "graphshell.access-history/v1";

/// The public identity context attached to one address access.
///
/// These are references only. Personae secrets and signing authority remain in
/// the native host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessContext {
    pub persona: String,
    pub device: String,
    pub at_ms: u64,
}

/// One durable record of an address being handed to a selected handler.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessRecord {
    pub persona: String,
    pub device: String,
    pub at_ms: u64,
    pub handler: String,
}

/// Ordered accesses for one addressed node.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessHistory {
    pub records: Vec<AccessRecord>,
}

#[derive(Debug)]
pub enum AccessError {
    UnknownNode,
    InvalidFacet(serde_json::Error),
    RejectedFacet(FacetError),
}

impl std::fmt::Display for AccessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNode => write!(formatter, "access target is not in the Mere graph"),
            Self::InvalidFacet(error) => write!(formatter, "access history is invalid: {error}"),
            Self::RejectedFacet(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for AccessError {}

/// Read one node's access history. An absent facet is an empty history.
pub fn access_history(graph: &Graph, key: NodeKey) -> Result<AccessHistory, AccessError> {
    let node = graph.get_node(key).ok_or(AccessError::UnknownNode)?;
    let Some(value) = graph
        .facets()
        .get(&node.id, &FacetId::new(ACCESS_HISTORY_FACET))
    else {
        return Ok(AccessHistory::default());
    };
    serde_json::from_value(value.clone()).map_err(AccessError::InvalidFacet)
}

/// Append an access through the host extension gate.
pub fn record_access(
    graph: &mut Graph,
    key: NodeKey,
    context: &AccessContext,
    handler: &str,
) -> Result<(), AccessError> {
    let node_id = graph
        .get_node(key)
        .map(|node| node.id)
        .ok_or(AccessError::UnknownNode)?;
    let mut history = access_history(graph, key)?;
    history.records.push(AccessRecord {
        persona: context.persona.clone(),
        device: context.device.clone(),
        at_ms: context.at_ms,
        handler: handler.to_string(),
    });
    let value = serde_json::to_value(history).expect("AccessHistory always serializes");
    graph
        .facets_mut()
        .set(
            node_id,
            FacetId::new(ACCESS_HISTORY_FACET),
            value,
            &AcceptAll,
        )
        .map_err(AccessError::RejectedFacet)
}
