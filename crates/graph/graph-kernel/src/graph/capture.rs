/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Stable-id graph-delta capture/replay helpers.
//!
//! This records the graph mutations that already have stable-id replay forms in
//! [`super::apply::GraphDelta`]: add/remove node, assert/retract relation,
//! traversal append, media writes, navigation chronology/history mutations, the
//! lighter per-node content/presentation setters, semantic-predicate edge
//! writes, the node-enrichment setters, and deterministic frame/history state
//! setters. The broader content-mutation lane still stays separate until those
//! writes grow stable-id replay forms of their own.

use std::cell::RefCell;
use std::sync::Arc;

use euclid::default::Point2D;
use node_lineage::TransitionKind;
use rkyv::{Archive, Deserialize, Serialize};
use uuid::Uuid;

use super::apply::{GraphDelta, apply_graph_delta};
use super::{
    Coupling, CouplingResponse, EdgeAssertion, Field, FieldDefinition, FieldExtent, FieldId,
    FieldLifecycle, FrameLayoutHint, Graph, NavigationTrigger, NodeSelector, RelationSelector,
};
use crate::persistence::{
    PersistedCoupling, PersistedCouplingResponse, PersistedField, PersistedFieldExtent,
    PersistedFieldLifecycle, PersistedNodeSelector,
};
use crate::types::{NodeClassification, NodeDerivation, NodeProperty};

/// The stable-id, serializable mirror of the replayable graph deltas.
#[derive(
    Debug, Clone, PartialEq, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize,
)]
pub enum CapturedDelta {
    ReplayAddNodeWithIdIfMissing {
        id: String,
        url: String,
        position: [f32; 2],
    },
    ReplayAssertRelationByIds {
        from_id: String,
        to_id: String,
        assertion: EdgeAssertion,
    },
    ReplayRemoveNodeById {
        node_id: String,
    },
    ReplayRetractRelationsByIds {
        from_id: String,
        to_id: String,
        selector: RelationSelector,
    },
    ReplayAppendTraversalByIds {
        from_id: String,
        to_id: String,
        trigger: NavigationTrigger,
        timestamp_ms: u64,
    },
    ReplaySetNodeTitleById {
        node_id: String,
        title: String,
    },
    ReplaySetNodeUrlById {
        node_id: String,
        new_url: String,
    },
    ReplaySetNodeThumbnailById {
        node_id: String,
        png_bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    ReplaySetNodeFaviconById {
        node_id: String,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
    ReplaySetNodeMimeHintById {
        node_id: String,
        mime_hint: Option<String>,
    },
    ReplaySetNodeViewerOverrideById {
        node_id: String,
        viewer_override: Option<String>,
    },
    ReplaySetNodePinnedById {
        node_id: String,
        is_pinned: bool,
    },
    ReplaySetNodeCompatModeById {
        node_id: String,
        compat_mode: bool,
    },
    ReplayInsertNodeTagById {
        node_id: String,
        tag: String,
    },
    ReplayRemoveNodeTagById {
        node_id: String,
        tag: String,
    },
    ReplaySetNodeBodyById {
        node_id: String,
        body: Option<String>,
    },
    ReplayNavigateNodeById {
        node_id: String,
        url: String,
        transition: TransitionKind,
        timestamp_ms: u64,
        last_session_visited: u64,
    },
    ReplayBranchHistoryByIds {
        child_id: String,
        parent_id: String,
    },
    ReplayNodeHistoryBackById {
        node_id: String,
        timestamp_ms: u64,
    },
    ReplayNodeHistoryForwardById {
        node_id: String,
        timestamp_ms: u64,
    },
    ReplayAppendNodePropertyById {
        node_id: String,
        property: NodeProperty,
    },
    ReplayAddNodeClassificationById {
        node_id: String,
        classification: NodeClassification,
    },
    ReplayRecordNodeDerivationById {
        node_id: String,
        derivation: NodeDerivation,
    },
    ReplaySetEdgeSemanticPredicateByIds {
        from_id: String,
        to_id: String,
        predicate: Option<String>,
    },
    ReplayAssertSemanticPredicateByIds {
        from_id: String,
        to_id: String,
        predicate: String,
    },
    ReplayAppendFrameLayoutHintById {
        node_id: String,
        hint: FrameLayoutHint,
    },
    ReplayRemoveFrameLayoutHintById {
        node_id: String,
        hint_index: usize,
    },
    ReplayMoveFrameLayoutHintById {
        node_id: String,
        from_index: usize,
        to_index: usize,
    },
    ReplaySetFrameSplitOfferSuppressedById {
        node_id: String,
        suppressed: bool,
    },
    ReplayUpdateNodeHistoryById {
        node_id: String,
        entries: Vec<String>,
        current_index: usize,
    },
    ReplayAddField {
        field: PersistedField,
    },
    ReplayRetireFieldById {
        field_id: String,
    },
    ReplayAddCoupling {
        coupling: PersistedCoupling,
    },
    ReplaySetFieldCouplingStrengthByFieldId {
        field_id: String,
        strength: f32,
    },
}

impl CapturedDelta {
    /// The graph-kernel replay delta this captured record folds back into.
    pub fn replay_delta(&self) -> GraphDelta {
        match self {
            Self::ReplayAddNodeWithIdIfMissing { id, url, position } => {
                GraphDelta::ReplayAddNodeWithIdIfMissing {
                    id: parse_uuid(id),
                    url: url.clone(),
                    position: Point2D::new(position[0], position[1]),
                }
            }
            Self::ReplayAssertRelationByIds {
                from_id,
                to_id,
                assertion,
            } => GraphDelta::ReplayAssertRelationByIds {
                from_id: parse_uuid(from_id),
                to_id: parse_uuid(to_id),
                assertion: assertion.clone(),
            },
            Self::ReplayRemoveNodeById { node_id } => GraphDelta::ReplayRemoveNodeById {
                node_id: parse_uuid(node_id),
            },
            Self::ReplayRetractRelationsByIds {
                from_id,
                to_id,
                selector,
            } => GraphDelta::ReplayRetractRelationsByIds {
                from_id: parse_uuid(from_id),
                to_id: parse_uuid(to_id),
                selector: *selector,
            },
            Self::ReplayAppendTraversalByIds {
                from_id,
                to_id,
                trigger,
                timestamp_ms,
            } => GraphDelta::ReplayAppendTraversalByIds {
                from_id: parse_uuid(from_id),
                to_id: parse_uuid(to_id),
                trigger: *trigger,
                timestamp_ms: *timestamp_ms,
            },
            Self::ReplaySetNodeTitleById { node_id, title } => GraphDelta::ReplaySetNodeTitleById {
                node_id: parse_uuid(node_id),
                title: title.clone(),
            },
            Self::ReplaySetNodeUrlById { node_id, new_url } => GraphDelta::ReplaySetNodeUrlById {
                node_id: parse_uuid(node_id),
                new_url: new_url.clone(),
            },
            Self::ReplaySetNodeThumbnailById {
                node_id,
                png_bytes,
                width,
                height,
            } => GraphDelta::ReplaySetNodeThumbnailById {
                node_id: parse_uuid(node_id),
                png_bytes: png_bytes.clone(),
                width: *width,
                height: *height,
            },
            Self::ReplaySetNodeFaviconById {
                node_id,
                rgba,
                width,
                height,
            } => GraphDelta::ReplaySetNodeFaviconById {
                node_id: parse_uuid(node_id),
                rgba: rgba.clone(),
                width: *width,
                height: *height,
            },
            Self::ReplaySetNodeMimeHintById { node_id, mime_hint } => {
                GraphDelta::ReplaySetNodeMimeHintById {
                    node_id: parse_uuid(node_id),
                    mime_hint: mime_hint.clone(),
                }
            }
            Self::ReplaySetNodeViewerOverrideById {
                node_id,
                viewer_override,
            } => GraphDelta::ReplaySetNodeViewerOverrideById {
                node_id: parse_uuid(node_id),
                viewer_override: viewer_override.clone(),
            },
            Self::ReplaySetNodePinnedById { node_id, is_pinned } => {
                GraphDelta::ReplaySetNodePinnedById {
                    node_id: parse_uuid(node_id),
                    is_pinned: *is_pinned,
                }
            }
            Self::ReplaySetNodeCompatModeById {
                node_id,
                compat_mode,
            } => GraphDelta::ReplaySetNodeCompatModeById {
                node_id: parse_uuid(node_id),
                compat_mode: *compat_mode,
            },
            Self::ReplayInsertNodeTagById { node_id, tag } => GraphDelta::ReplayInsertNodeTagById {
                node_id: parse_uuid(node_id),
                tag: tag.clone(),
            },
            Self::ReplayRemoveNodeTagById { node_id, tag } => GraphDelta::ReplayRemoveNodeTagById {
                node_id: parse_uuid(node_id),
                tag: tag.clone(),
            },
            Self::ReplaySetNodeBodyById { node_id, body } => GraphDelta::ReplaySetNodeBodyById {
                node_id: parse_uuid(node_id),
                body: body.clone(),
            },
            Self::ReplayNavigateNodeById {
                node_id,
                url,
                transition,
                timestamp_ms,
                last_session_visited,
            } => GraphDelta::ReplayNavigateNodeById {
                node_id: parse_uuid(node_id),
                url: url.clone(),
                transition: *transition,
                timestamp_ms: *timestamp_ms,
                last_session_visited: *last_session_visited,
            },
            Self::ReplayBranchHistoryByIds {
                child_id,
                parent_id,
            } => GraphDelta::ReplayBranchHistoryByIds {
                child_id: parse_uuid(child_id),
                parent_id: parse_uuid(parent_id),
            },
            Self::ReplayNodeHistoryBackById {
                node_id,
                timestamp_ms,
            } => GraphDelta::ReplayNodeHistoryBackById {
                node_id: parse_uuid(node_id),
                timestamp_ms: *timestamp_ms,
            },
            Self::ReplayNodeHistoryForwardById {
                node_id,
                timestamp_ms,
            } => GraphDelta::ReplayNodeHistoryForwardById {
                node_id: parse_uuid(node_id),
                timestamp_ms: *timestamp_ms,
            },
            Self::ReplayAppendNodePropertyById { node_id, property } => {
                GraphDelta::ReplayAppendNodePropertyById {
                    node_id: parse_uuid(node_id),
                    property: property.clone(),
                }
            }
            Self::ReplayAddNodeClassificationById {
                node_id,
                classification,
            } => GraphDelta::ReplayAddNodeClassificationById {
                node_id: parse_uuid(node_id),
                classification: classification.clone(),
            },
            Self::ReplayRecordNodeDerivationById {
                node_id,
                derivation,
            } => GraphDelta::ReplayRecordNodeDerivationById {
                node_id: parse_uuid(node_id),
                derivation: derivation.clone(),
            },
            Self::ReplaySetEdgeSemanticPredicateByIds {
                from_id,
                to_id,
                predicate,
            } => GraphDelta::ReplaySetEdgeSemanticPredicateByIds {
                from_id: parse_uuid(from_id),
                to_id: parse_uuid(to_id),
                predicate: predicate.clone(),
            },
            Self::ReplayAssertSemanticPredicateByIds {
                from_id,
                to_id,
                predicate,
            } => GraphDelta::ReplayAssertSemanticPredicateByIds {
                from_id: parse_uuid(from_id),
                to_id: parse_uuid(to_id),
                predicate: predicate.clone(),
            },
            Self::ReplayAppendFrameLayoutHintById { node_id, hint } => {
                GraphDelta::ReplayAppendFrameLayoutHintById {
                    node_id: parse_uuid(node_id),
                    hint: hint.clone(),
                }
            }
            Self::ReplayRemoveFrameLayoutHintById {
                node_id,
                hint_index,
            } => GraphDelta::ReplayRemoveFrameLayoutHintById {
                node_id: parse_uuid(node_id),
                hint_index: *hint_index,
            },
            Self::ReplayMoveFrameLayoutHintById {
                node_id,
                from_index,
                to_index,
            } => GraphDelta::ReplayMoveFrameLayoutHintById {
                node_id: parse_uuid(node_id),
                from_index: *from_index,
                to_index: *to_index,
            },
            Self::ReplaySetFrameSplitOfferSuppressedById {
                node_id,
                suppressed,
            } => GraphDelta::ReplaySetFrameSplitOfferSuppressedById {
                node_id: parse_uuid(node_id),
                suppressed: *suppressed,
            },
            Self::ReplayUpdateNodeHistoryById {
                node_id,
                entries,
                current_index,
            } => GraphDelta::ReplayUpdateNodeHistoryById {
                node_id: parse_uuid(node_id),
                entries: entries.clone(),
                current_index: *current_index,
            },
            Self::ReplayAddField { field } => GraphDelta::ReplayAddField {
                field: field.clone(),
            },
            Self::ReplayRetireFieldById { field_id } => GraphDelta::ReplayRetireFieldById {
                field_id: field_id.clone(),
            },
            Self::ReplayAddCoupling { coupling } => GraphDelta::ReplayAddCoupling {
                coupling: coupling.clone(),
            },
            Self::ReplaySetFieldCouplingStrengthByFieldId { field_id, strength } => {
                GraphDelta::ReplaySetFieldCouplingStrengthByFieldId {
                    field_id: field_id.clone(),
                    strength: *strength,
                }
            }
        }
    }
}

fn parse_uuid(id: &str) -> Uuid {
    Uuid::parse_str(id).expect("captured stable ids should always parse")
}

pub(crate) fn persisted_field_from_field(field: &Field) -> PersistedField {
    PersistedField {
        id: field.id.as_uuid().to_string(),
        name: field.name.clone(),
        definition_json: serde_json::to_string(&field.definition).unwrap_or_default(),
        extent: match &field.extent {
            FieldExtent::Global => PersistedFieldExtent::Global,
            FieldExtent::Region {
                min_x,
                min_y,
                max_x,
                max_y,
            } => PersistedFieldExtent::Region {
                min_x: *min_x,
                min_y: *min_y,
                max_x: *max_x,
                max_y: *max_y,
            },
            FieldExtent::AttachedToNode(id) => PersistedFieldExtent::AttachedToNode(id.to_string()),
        },
        lifecycle: match field.lifecycle {
            FieldLifecycle::Active => PersistedFieldLifecycle::Active,
            FieldLifecycle::Retired => PersistedFieldLifecycle::Retired,
        },
    }
}

pub(crate) fn field_from_persisted(pfield: &PersistedField) -> Option<Field> {
    let id = Uuid::parse_str(&pfield.id).ok()?;
    let definition = serde_json::from_str::<FieldDefinition>(&pfield.definition_json).ok()?;
    let extent = match &pfield.extent {
        PersistedFieldExtent::Global => FieldExtent::Global,
        PersistedFieldExtent::Region {
            min_x,
            min_y,
            max_x,
            max_y,
        } => FieldExtent::Region {
            min_x: *min_x,
            min_y: *min_y,
            max_x: *max_x,
            max_y: *max_y,
        },
        PersistedFieldExtent::AttachedToNode(s) => Uuid::parse_str(s)
            .map(FieldExtent::AttachedToNode)
            .unwrap_or(FieldExtent::Global),
    };
    let mut field = Field::new(FieldId::from_uuid(id), definition).with_extent(extent);
    if let Some(name) = &pfield.name {
        field = field.with_name(name.clone());
    }
    field.lifecycle = match pfield.lifecycle {
        PersistedFieldLifecycle::Active => FieldLifecycle::Active,
        PersistedFieldLifecycle::Retired => FieldLifecycle::Retired,
    };
    Some(field)
}

pub(crate) fn persisted_coupling_from_coupling(coupling: &Coupling) -> PersistedCoupling {
    PersistedCoupling {
        id: coupling.id.as_uuid().to_string(),
        field_id: coupling.field.as_uuid().to_string(),
        selector: match &coupling.selector {
            NodeSelector::All => PersistedNodeSelector::All,
            NodeSelector::Tagged(tag) => PersistedNodeSelector::Tagged(tag.clone()),
            NodeSelector::Kind(kind) => PersistedNodeSelector::Kind(kind.clone()),
            NodeSelector::NotTagged(tag) => PersistedNodeSelector::NotTagged(tag.clone()),
        },
        response: match &coupling.response {
            CouplingResponse::AttractToMin => PersistedCouplingResponse::AttractToMin,
            CouplingResponse::RepelFromMax => PersistedCouplingResponse::RepelFromMax,
            CouplingResponse::AlignVelocity => PersistedCouplingResponse::AlignVelocity,
            CouplingResponse::FlowAdvect => PersistedCouplingResponse::FlowAdvect,
            CouplingResponse::DampenInside { factor } => {
                PersistedCouplingResponse::DampenInside { factor: *factor }
            }
            CouplingResponse::ContainmentWall => PersistedCouplingResponse::ContainmentWall,
            CouplingResponse::Open { predicate } => PersistedCouplingResponse::Open {
                predicate: predicate.clone(),
            },
        },
        strength: coupling.strength,
    }
}

pub(crate) fn coupling_from_persisted(pcoupling: &PersistedCoupling) -> Option<Coupling> {
    let cid = Uuid::parse_str(&pcoupling.id).ok()?;
    let fid = Uuid::parse_str(&pcoupling.field_id).ok()?;
    let selector = match &pcoupling.selector {
        PersistedNodeSelector::All => NodeSelector::All,
        PersistedNodeSelector::Tagged(tag) => NodeSelector::Tagged(tag.clone()),
        PersistedNodeSelector::Kind(kind) => NodeSelector::Kind(kind.clone()),
        PersistedNodeSelector::NotTagged(tag) => NodeSelector::NotTagged(tag.clone()),
    };
    let response = match &pcoupling.response {
        PersistedCouplingResponse::AttractToMin => CouplingResponse::AttractToMin,
        PersistedCouplingResponse::RepelFromMax => CouplingResponse::RepelFromMax,
        PersistedCouplingResponse::AlignVelocity => CouplingResponse::AlignVelocity,
        PersistedCouplingResponse::FlowAdvect => CouplingResponse::FlowAdvect,
        PersistedCouplingResponse::DampenInside { factor } => {
            CouplingResponse::DampenInside { factor: *factor }
        }
        PersistedCouplingResponse::ContainmentWall => CouplingResponse::ContainmentWall,
        PersistedCouplingResponse::Open { predicate } => CouplingResponse::Open {
            predicate: predicate.clone(),
        },
    };
    Some(Coupling::new(
        super::CouplingId::from_uuid(cid),
        FieldId::from_uuid(fid),
        selector,
        response,
        pcoupling.strength,
    ))
}

/// Cheap live counts for the graph's arena-like tables, surfaced in Apparatus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GraphTableStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub relation_count: usize,
    pub field_count: usize,
    pub coupling_count: usize,
    pub history_owner_count: usize,
    pub history_entry_count: usize,
    pub history_visit_count: usize,
}

impl Graph {
    /// Cheap live counts for the graph's kernel-owned tables.
    pub fn table_stats(&self) -> GraphTableStats {
        GraphTableStats {
            node_count: self.node_count(),
            edge_count: self.edge_count(),
            relation_count: self.relations().count(),
            field_count: self.fields().count(),
            coupling_count: self.couplings().count(),
            history_owner_count: self.nav.owner_count(),
            history_entry_count: self.nav.entry_count(),
            history_visit_count: self.nav.visit_count(),
        }
    }
}

/// Replay a captured-delta stream against an empty graph.
pub fn replay_captured_deltas<I>(deltas: I) -> Graph
where
    I: IntoIterator<Item = CapturedDelta>,
{
    let mut graph = Graph::new();
    for delta in deltas {
        let _ = apply_graph_delta(&mut graph, delta.replay_delta());
    }
    graph
}

type CaptureHook = dyn Fn(&CapturedDelta) + Send + Sync + 'static;

thread_local! {
    static CAPTURE_HOOK: RefCell<Option<Arc<CaptureHook>>> = RefCell::new(None);
}

/// Install or clear the current thread's graph-delta capture hook.
pub fn set_captured_delta_hook(hook: Option<Arc<CaptureHook>>) {
    CAPTURE_HOOK.with(|slot| *slot.borrow_mut() = hook);
}

pub(crate) fn record_captured_delta(delta: &CapturedDelta) {
    CAPTURE_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow().as_ref() {
            hook(delta);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use crate::graph::{
        ContainmentSubKind, Coupling, CouplingId, CouplingResponse, EdgeAssertion, Field,
        FieldDefinition, FieldExtent, FieldId, Graph, NavigationTrigger, NodeSelector,
        ProvenanceSubKind, ScalarField, SemanticSubKind, SharedNavigationMemory,
    };
    use crate::types::{
        ClassificationProvenance, ClassificationScheme, ClassificationStatus, FrameLayoutHint,
        NodeClassification, NodeDerivation, NodeProperty, SplitOrientation,
    };

    fn point(x: f32, y: f32) -> Point2D<f32> {
        Point2D::new(x, y)
    }

    fn sample_field(id: FieldId) -> Field {
        Field::new(id, FieldDefinition::Scalar(ScalarField::Const(1.0)))
            .with_name("focus")
            .with_extent(FieldExtent::Region {
                min_x: -10.0,
                min_y: -20.0,
                max_x: 30.0,
                max_y: 40.0,
            })
    }

    fn sample_coupling(id: CouplingId, field: FieldId) -> Coupling {
        Coupling::new(
            id,
            field,
            NodeSelector::Kind("paper".into()),
            CouplingResponse::DampenInside { factor: 0.3 },
            1.5,
        )
    }

    #[test]
    fn structural_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplayAssertRelationByIds {
            from_id: Uuid::from_u128(1).to_string(),
            to_id: Uuid::from_u128(2).to_string(),
            assertion: EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: Some("next".into()),
                decay_progress: Some(0.25),
            },
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn traversal_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplayAppendTraversalByIds {
            from_id: Uuid::from_u128(3).to_string(),
            to_id: Uuid::from_u128(4).to_string(),
            trigger: NavigationTrigger::Redirect,
            timestamp_ms: 12_345,
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn node_content_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplaySetNodeBodyById {
            node_id: Uuid::from_u128(5).to_string(),
            body: Some("hello".into()),
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn node_enrichment_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplayAddNodeClassificationById {
            node_id: Uuid::from_u128(6).to_string(),
            classification: NodeClassification {
                scheme: ClassificationScheme::ContentKind,
                value: "article".into(),
                label: Some("Article".into()),
                confidence: 1.0,
                provenance: ClassificationProvenance::UserAuthored,
                status: ClassificationStatus::Accepted,
                primary: true,
            },
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn node_media_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplaySetNodeThumbnailById {
            node_id: Uuid::from_u128(7).to_string(),
            png_bytes: vec![1, 2, 3, 4],
            width: 2,
            height: 2,
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn navigation_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplayNavigateNodeById {
            node_id: Uuid::from_u128(8).to_string(),
            url: "https://example.test/two".into(),
            transition: TransitionKind::UrlTyped,
            timestamp_ms: 123,
            last_session_visited: 77,
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn semantic_edge_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplaySetEdgeSemanticPredicateByIds {
            from_id: Uuid::from_u128(81).to_string(),
            to_id: Uuid::from_u128(82).to_string(),
            predicate: Some("https://schema.org/author".into()),
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn field_layer_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplayAddField {
            field: persisted_field_from_field(&sample_field(FieldId::from_uuid(Uuid::from_u128(
                9,
            )))),
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn frame_history_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplayUpdateNodeHistoryById {
            node_id: Uuid::from_u128(10).to_string(),
            entries: vec!["https://a.test".into(), "https://b.test".into()],
            current_index: 1,
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn replay_captured_deltas_rebuilds_relation_traversal_media_navigation_field_enrichment_and_frame_history_state()
     {
        let a_id = Uuid::from_u128(11);
        let b_id = Uuid::from_u128(12);
        let c_id = Uuid::from_u128(13);
        let field_id = FieldId::from_uuid(Uuid::from_u128(14));
        let coupling_id = CouplingId::from_uuid(Uuid::from_u128(15));
        let deltas = vec![
            CapturedDelta::ReplayAddNodeWithIdIfMissing {
                id: a_id.to_string(),
                url: "https://a.test".into(),
                position: [1.0, 2.0],
            },
            CapturedDelta::ReplayAddNodeWithIdIfMissing {
                id: b_id.to_string(),
                url: "https://b.test".into(),
                position: [3.0, 4.0],
            },
            CapturedDelta::ReplayAddNodeWithIdIfMissing {
                id: c_id.to_string(),
                url: "https://c.test".into(),
                position: [5.0, 6.0],
            },
            CapturedDelta::ReplayAssertRelationByIds {
                from_id: a_id.to_string(),
                to_id: b_id.to_string(),
                assertion: EdgeAssertion::Containment {
                    sub_kind: ContainmentSubKind::CollectionMember,
                },
            },
            CapturedDelta::ReplayAppendTraversalByIds {
                from_id: a_id.to_string(),
                to_id: b_id.to_string(),
                trigger: NavigationTrigger::LinkClick,
                timestamp_ms: 6_789,
            },
            CapturedDelta::ReplaySetNodeTitleById {
                node_id: a_id.to_string(),
                title: "Alpha".into(),
            },
            CapturedDelta::ReplaySetNodeUrlById {
                node_id: a_id.to_string(),
                new_url: "https://a.test/next".into(),
            },
            CapturedDelta::ReplaySetNodeThumbnailById {
                node_id: a_id.to_string(),
                png_bytes: vec![0x89, b'P', b'N', b'G'],
                width: 1,
                height: 1,
            },
            CapturedDelta::ReplaySetNodeFaviconById {
                node_id: a_id.to_string(),
                rgba: vec![255, 0, 0, 255],
                width: 1,
                height: 1,
            },
            CapturedDelta::ReplaySetNodeMimeHintById {
                node_id: a_id.to_string(),
                mime_hint: Some("text/html".into()),
            },
            CapturedDelta::ReplaySetNodeViewerOverrideById {
                node_id: a_id.to_string(),
                viewer_override: Some("viewer:note".into()),
            },
            CapturedDelta::ReplaySetNodePinnedById {
                node_id: a_id.to_string(),
                is_pinned: true,
            },
            CapturedDelta::ReplaySetNodeCompatModeById {
                node_id: a_id.to_string(),
                compat_mode: true,
            },
            CapturedDelta::ReplayInsertNodeTagById {
                node_id: a_id.to_string(),
                tag: "research".into(),
            },
            CapturedDelta::ReplayRemoveNodeTagById {
                node_id: a_id.to_string(),
                tag: "research".into(),
            },
            CapturedDelta::ReplaySetNodeBodyById {
                node_id: a_id.to_string(),
                body: Some("body".into()),
            },
            CapturedDelta::ReplayNavigateNodeById {
                node_id: b_id.to_string(),
                url: "https://b.test/one".into(),
                transition: TransitionKind::UrlTyped,
                timestamp_ms: 111,
                last_session_visited: 77,
            },
            CapturedDelta::ReplayNavigateNodeById {
                node_id: b_id.to_string(),
                url: "https://b.test/two".into(),
                transition: TransitionKind::UrlTyped,
                timestamp_ms: 222,
                last_session_visited: 77,
            },
            CapturedDelta::ReplayNodeHistoryBackById {
                node_id: b_id.to_string(),
                timestamp_ms: 333,
            },
            CapturedDelta::ReplayNodeHistoryForwardById {
                node_id: b_id.to_string(),
                timestamp_ms: 444,
            },
            CapturedDelta::ReplayBranchHistoryByIds {
                child_id: c_id.to_string(),
                parent_id: b_id.to_string(),
            },
            CapturedDelta::ReplayNavigateNodeById {
                node_id: c_id.to_string(),
                url: "https://c.test/branched".into(),
                transition: TransitionKind::UrlTyped,
                timestamp_ms: 555,
                last_session_visited: 77,
            },
            CapturedDelta::ReplayAddField {
                field: persisted_field_from_field(&sample_field(field_id)),
            },
            CapturedDelta::ReplayAddCoupling {
                coupling: persisted_coupling_from_coupling(&sample_coupling(coupling_id, field_id)),
            },
            CapturedDelta::ReplaySetFieldCouplingStrengthByFieldId {
                field_id: field_id.as_uuid().to_string(),
                strength: 2.0,
            },
            CapturedDelta::ReplayRetireFieldById {
                field_id: field_id.as_uuid().to_string(),
            },
            CapturedDelta::ReplayAppendNodePropertyById {
                node_id: a_id.to_string(),
                property: NodeProperty {
                    predicate: "https://schema.org/datePublished".into(),
                    value: "2026-07-02".into(),
                },
            },
            CapturedDelta::ReplayAddNodeClassificationById {
                node_id: a_id.to_string(),
                classification: NodeClassification {
                    scheme: ClassificationScheme::ContentKind,
                    value: "article".into(),
                    label: Some("Article".into()),
                    confidence: 1.0,
                    provenance: ClassificationProvenance::UserAuthored,
                    status: ClassificationStatus::Accepted,
                    primary: true,
                },
            },
            CapturedDelta::ReplayRecordNodeDerivationById {
                node_id: a_id.to_string(),
                derivation: NodeDerivation {
                    sub_kind: ProvenanceSubKind::ExtractedFrom,
                    source_node: Uuid::from_u128(99).to_string(),
                    source_graph: Some("graph:test".into()),
                },
            },
            CapturedDelta::ReplaySetEdgeSemanticPredicateByIds {
                from_id: a_id.to_string(),
                to_id: b_id.to_string(),
                predicate: Some("https://schema.org/author".into()),
            },
            CapturedDelta::ReplayAssertSemanticPredicateByIds {
                from_id: b_id.to_string(),
                to_id: c_id.to_string(),
                predicate: "https://schema.org/citation".into(),
            },
            CapturedDelta::ReplayAppendFrameLayoutHintById {
                node_id: a_id.to_string(),
                hint: FrameLayoutHint::SplitHalf {
                    first: b_id.to_string(),
                    second: c_id.to_string(),
                    orientation: SplitOrientation::Vertical,
                },
            },
            CapturedDelta::ReplayAppendFrameLayoutHintById {
                node_id: a_id.to_string(),
                hint: FrameLayoutHint::SplitHalf {
                    first: c_id.to_string(),
                    second: b_id.to_string(),
                    orientation: SplitOrientation::Horizontal,
                },
            },
            CapturedDelta::ReplayMoveFrameLayoutHintById {
                node_id: a_id.to_string(),
                from_index: 0,
                to_index: 1,
            },
            CapturedDelta::ReplayRemoveFrameLayoutHintById {
                node_id: a_id.to_string(),
                hint_index: 1,
            },
            CapturedDelta::ReplaySetFrameSplitOfferSuppressedById {
                node_id: a_id.to_string(),
                suppressed: true,
            },
            CapturedDelta::ReplayUpdateNodeHistoryById {
                node_id: a_id.to_string(),
                entries: vec![
                    "https://a.test/one".into(),
                    "https://a.test/two".into(),
                    "https://a.test/three".into(),
                ],
                current_index: 9,
            },
        ];

        let graph = replay_captured_deltas(deltas);
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
        assert!(graph.get_node_by_id(a_id).is_some());
        assert!(graph.get_node_by_id(b_id).is_some());
        assert!(graph.get_node_by_id(c_id).is_some());
        assert_eq!(graph.relations().count(), 2);
        let from = graph.get_node_key_by_id(a_id).expect("from key");
        let to = graph.get_node_key_by_id(b_id).expect("to key");
        let edge = graph.find_edge_key(from, to).expect("edge key");
        let payload = graph.get_edge(edge).expect("edge payload");
        assert_eq!(payload.traversals().len(), 1);
        assert_eq!(payload.metrics().total_navigations, 1);
        assert_eq!(payload.metrics().last_navigated_at, Some(6_789));
        let node = graph.get_node(from).expect("node payload");
        assert_eq!(node.title, "Alpha");
        assert_eq!(node.url(), "https://a.test/next");
        assert_eq!(
            node.thumbnail_png.as_deref(),
            Some(&[0x89, b'P', b'N', b'G'][..])
        );
        assert_eq!(node.thumbnail_width, 1);
        assert_eq!(node.thumbnail_height, 1);
        assert_eq!(node.favicon_rgba.as_deref(), Some(&[255, 0, 0, 255][..]));
        assert_eq!(node.favicon_width, 1);
        assert_eq!(node.favicon_height, 1);
        assert_eq!(node.mime_hint.as_deref(), Some("text/html"));
        assert_eq!(node.viewer_override.as_deref(), Some("viewer:note"));
        assert!(node.is_pinned);
        assert!(node.compat_mode);
        assert_eq!(node.body.as_deref(), Some("body"));
        assert!(!node.tags.contains("research"));
        assert_eq!(node.properties.len(), 1);
        assert_eq!(
            node.properties[0].predicate,
            "https://schema.org/datePublished"
        );
        assert_eq!(node.properties[0].value, "2026-07-02");
        assert_eq!(node.classifications.len(), 1);
        assert_eq!(node.classifications[0].value, "article");
        assert_eq!(node.derivations.len(), 1);
        assert_eq!(
            node.derivations[0].sub_kind,
            ProvenanceSubKind::ExtractedFrom
        );
        assert_eq!(
            payload
                .semantic_data()
                .and_then(|data| data.predicate.as_deref()),
            Some("https://schema.org/author")
        );
        let history_node_key = graph.get_node_key_by_id(b_id).expect("history node key");
        let history_node = graph.get_node(history_node_key).expect("history node");
        assert_eq!(history_node.url(), "https://b.test/two");
        assert_eq!(history_node.last_session_visited, 77);
        let history_projection = graph.node_history_projection(history_node_key);
        assert_eq!(
            history_projection.entries,
            vec![
                "https://b.test/one".to_string(),
                "https://b.test/two".to_string(),
            ]
        );
        assert_eq!(history_projection.current_index, 1);
        let branched_node_key = graph.get_node_key_by_id(c_id).expect("branched node key");
        let branched_node = graph.get_node(branched_node_key).expect("branched node");
        assert_eq!(branched_node.url(), "https://c.test/branched");
        assert_eq!(branched_node.last_session_visited, 77);
        let semantic_edge = graph
            .find_edge_key(history_node_key, branched_node_key)
            .expect("semantic edge key");
        let semantic_payload = graph
            .get_edge(semantic_edge)
            .expect("semantic edge payload");
        assert_eq!(
            semantic_payload
                .semantic_data()
                .and_then(|data| data.predicate.as_deref()),
            Some("https://schema.org/citation")
        );
        let field = graph.field(field_id).expect("field payload");
        assert_eq!(field.name.as_deref(), Some("focus"));
        assert!(!field.is_active());
        assert_eq!(
            field.extent,
            FieldExtent::Region {
                min_x: -10.0,
                min_y: -20.0,
                max_x: 30.0,
                max_y: 40.0,
            }
        );
        let coupling = graph
            .couplings_for_field(field_id)
            .next()
            .expect("field coupling");
        assert_eq!(coupling.id, coupling_id);
        assert_eq!(coupling.selector, NodeSelector::Kind("paper".into()));
        assert_eq!(
            coupling.response,
            CouplingResponse::DampenInside { factor: 0.3 }
        );
        assert_eq!(coupling.strength, 2.0);
        let hints = graph.frame_layout_hints(from).expect("frame layout hints");
        assert_eq!(hints.len(), 1);
        assert_eq!(
            hints[0],
            FrameLayoutHint::SplitHalf {
                first: c_id.to_string(),
                second: b_id.to_string(),
                orientation: SplitOrientation::Horizontal,
            }
        );
        assert_eq!(graph.frame_split_offer_suppressed(from), Some(true));
        let history = graph.node_history_projection(from);
        assert_eq!(
            history.entries,
            vec![
                "https://a.test/one".to_string(),
                "https://a.test/two".to_string(),
                "https://a.test/three".to_string(),
            ]
        );
        assert_eq!(history.current_index, 2);
        let mut expected_nav = SharedNavigationMemory::empty();
        expected_nav.record_visit(b_id, "https://b.test/one", TransitionKind::UrlTyped, 111);
        expected_nav.record_visit(b_id, "https://b.test/two", TransitionKind::UrlTyped, 222);
        assert_eq!(
            expected_nav.back(b_id, 333).as_deref(),
            Some("https://b.test/one")
        );
        assert_eq!(
            expected_nav.forward(b_id, 444).as_deref(),
            Some("https://b.test/two")
        );
        expected_nav.spawn(c_id, b_id);
        expected_nav.record_visit(
            c_id,
            "https://c.test/branched",
            TransitionKind::UrlTyped,
            555,
        );
        expected_nav.seed_linear(
            a_id,
            vec![
                "https://a.test/one".to_string(),
                "https://a.test/two".to_string(),
                "https://a.test/three".to_string(),
            ],
            9,
        );
        let mut actual_snapshot = graph.to_snapshot().navigation.snapshot().clone();
        for owner in &mut actual_snapshot.owners {
            owner.owned_visits.sort_unstable();
        }
        for visit in &mut actual_snapshot.visits {
            visit.bindings.sort_by_key(|binding| {
                (
                    binding.owner,
                    binding.forward_child.unwrap_or(usize::MAX),
                    binding.last_accessed_at_ms,
                )
            });
        }
        let mut expected_snapshot = expected_nav.snapshot().clone();
        for owner in &mut expected_snapshot.owners {
            owner.owned_visits.sort_unstable();
        }
        for visit in &mut expected_snapshot.visits {
            visit.bindings.sort_by_key(|binding| {
                (
                    binding.owner,
                    binding.forward_child.unwrap_or(usize::MAX),
                    binding.last_accessed_at_ms,
                )
            });
        }
        assert_eq!(actual_snapshot, expected_snapshot);
    }

    #[test]
    fn capture_hook_receives_replayable_apply_events() {
        let captured = Arc::new(Mutex::new(Vec::<CapturedDelta>::new()));
        let sink = captured.clone();
        set_captured_delta_hook(Some(Arc::new(move |delta| {
            sink.lock().expect("capture sink").push(delta.clone());
        })));

        let mut graph = Graph::new();
        graph.set_current_session(77);
        let a = crate::graph::apply::add_node(
            &mut graph,
            Some(Uuid::from_u128(21)),
            "https://a.test".into(),
            point(0.0, 0.0),
        );
        let b = crate::graph::apply::add_node(
            &mut graph,
            Some(Uuid::from_u128(22)),
            "https://b.test".into(),
            point(1.0, 0.0),
        );
        let c = crate::graph::apply::add_node(
            &mut graph,
            Some(Uuid::from_u128(23)),
            "https://c.test".into(),
            point(2.0, 0.0),
        );
        let field_id = FieldId::from_uuid(Uuid::from_u128(24));
        let coupling_id = CouplingId::from_uuid(Uuid::from_u128(25));
        crate::graph::apply::assert_relation(
            &mut graph,
            a,
            b,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::AppendTraversal {
                from: a,
                to: b,
                trigger: NavigationTrigger::Programmatic,
                timestamp_ms: Some(4_321),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeTitle {
                key: a,
                title: "Alpha".into(),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeUrl {
                key: a,
                new_url: "https://a.test/next".into(),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeThumbnail {
                key: a,
                png_bytes: vec![0x89, b'P', b'N', b'G'],
                width: 1,
                height: 1,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeFavicon {
                key: a,
                rgba: vec![255, 0, 0, 255],
                width: 1,
                height: 1,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeMimeHint {
                key: a,
                mime_hint: Some("text/html".into()),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeViewerOverride {
                key: a,
                viewer_override: Some("viewer:note".into()),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodePinned {
                key: a,
                is_pinned: true,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeCompatMode {
                key: a,
                compat_mode: true,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::InsertNodeTag {
                key: a,
                tag: "research".into(),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::RemoveNodeTag {
                key: a,
                tag: "research".into(),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeBody {
                key: a,
                body: Some("body".into()),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::NavigateNode {
                key: b,
                url: "https://b.test/one".into(),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::NavigateNode {
                key: b,
                url: "https://b.test/two".into(),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::NodeHistoryBack { key: b },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::NodeHistoryForward { key: b },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::BranchHistory {
                child: c,
                parent: b,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::NavigateNode {
                key: c,
                url: "https://c.test/branched".into(),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::AddField {
                field: sample_field(field_id),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::AddCoupling {
                coupling: sample_coupling(coupling_id, field_id),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetFieldCouplingStrength {
                field: field_id,
                strength: 2.0,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::RetireField { id: field_id },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::AppendNodeProperty {
                key: a,
                property: NodeProperty {
                    predicate: "https://schema.org/datePublished".into(),
                    value: "2026-07-02".into(),
                },
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::AddNodeClassification {
                key: a,
                classification: NodeClassification {
                    scheme: ClassificationScheme::ContentKind,
                    value: "article".into(),
                    label: Some("Article".into()),
                    confidence: 1.0,
                    provenance: ClassificationProvenance::UserAuthored,
                    status: ClassificationStatus::Accepted,
                    primary: true,
                },
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::RecordNodeDerivation {
                key: a,
                derivation: NodeDerivation {
                    sub_kind: ProvenanceSubKind::ExtractedFrom,
                    source_node: Uuid::from_u128(99).to_string(),
                    source_graph: Some("graph:test".into()),
                },
            },
        );
        let ab_edge = graph.find_edge_key(a, b).expect("a->b edge");
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetEdgeSemanticPredicate {
                edge: ab_edge,
                predicate: Some("https://schema.org/author".into()),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::AssertSemanticPredicate {
                from: b,
                to: c,
                predicate: "https://schema.org/citation".into(),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::AppendFrameLayoutHint {
                key: a,
                hint: FrameLayoutHint::SplitHalf {
                    first: Uuid::from_u128(22).to_string(),
                    second: Uuid::from_u128(23).to_string(),
                    orientation: SplitOrientation::Vertical,
                },
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::AppendFrameLayoutHint {
                key: a,
                hint: FrameLayoutHint::SplitHalf {
                    first: Uuid::from_u128(23).to_string(),
                    second: Uuid::from_u128(22).to_string(),
                    orientation: SplitOrientation::Horizontal,
                },
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::MoveFrameLayoutHint {
                key: a,
                from_index: 0,
                to_index: 1,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::RemoveFrameLayoutHint {
                key: a,
                hint_index: 1,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetFrameSplitOfferSuppressed {
                key: a,
                suppressed: true,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::UpdateNodeHistory {
                key: a,
                entries: vec![
                    "https://a.test/one".into(),
                    "https://a.test/two".into(),
                    "https://a.test/three".into(),
                ],
                current_index: 9,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::RetractRelations {
                from: a,
                to: b,
                selector: RelationSelector::Semantic(SemanticSubKind::Hyperlink),
            },
        );
        let _ =
            crate::graph::apply::apply_graph_delta(&mut graph, GraphDelta::RemoveNode { key: b });

        set_captured_delta_hook(None);

        let out = captured.lock().expect("capture sink");
        assert_eq!(out.len(), 39);
        assert!(matches!(
            out[0],
            CapturedDelta::ReplayAddNodeWithIdIfMissing { .. }
        ));
        assert!(matches!(
            out[1],
            CapturedDelta::ReplayAddNodeWithIdIfMissing { .. }
        ));
        assert!(matches!(
            out[2],
            CapturedDelta::ReplayAddNodeWithIdIfMissing { .. }
        ));
        assert!(matches!(
            out[3],
            CapturedDelta::ReplayAssertRelationByIds { .. }
        ));
        assert!(matches!(
            out[4],
            CapturedDelta::ReplayAppendTraversalByIds { .. }
        ));
        assert!(matches!(
            out[5],
            CapturedDelta::ReplaySetNodeTitleById { .. }
        ));
        assert!(matches!(out[6], CapturedDelta::ReplaySetNodeUrlById { .. }));
        assert!(matches!(
            out[7],
            CapturedDelta::ReplaySetNodeThumbnailById { .. }
        ));
        assert!(matches!(
            out[8],
            CapturedDelta::ReplaySetNodeFaviconById { .. }
        ));
        assert!(matches!(
            out[9],
            CapturedDelta::ReplaySetNodeMimeHintById { .. }
        ));
        assert!(matches!(
            out[10],
            CapturedDelta::ReplaySetNodeViewerOverrideById { .. }
        ));
        assert!(matches!(
            out[11],
            CapturedDelta::ReplaySetNodePinnedById { .. }
        ));
        assert!(matches!(
            out[12],
            CapturedDelta::ReplaySetNodeCompatModeById { .. }
        ));
        assert!(matches!(
            out[13],
            CapturedDelta::ReplayInsertNodeTagById { .. }
        ));
        assert!(matches!(
            out[14],
            CapturedDelta::ReplayRemoveNodeTagById { .. }
        ));
        assert!(matches!(
            out[15],
            CapturedDelta::ReplaySetNodeBodyById { .. }
        ));
        assert!(matches!(
            out[16],
            CapturedDelta::ReplayNavigateNodeById { .. }
        ));
        assert!(matches!(
            out[17],
            CapturedDelta::ReplayNavigateNodeById { .. }
        ));
        assert!(matches!(
            out[18],
            CapturedDelta::ReplayNodeHistoryBackById { .. }
        ));
        assert!(matches!(
            out[19],
            CapturedDelta::ReplayNodeHistoryForwardById { .. }
        ));
        assert!(matches!(
            out[20],
            CapturedDelta::ReplayBranchHistoryByIds { .. }
        ));
        assert!(matches!(
            out[21],
            CapturedDelta::ReplayNavigateNodeById { .. }
        ));
        assert!(matches!(out[22], CapturedDelta::ReplayAddField { .. }));
        assert!(matches!(out[23], CapturedDelta::ReplayAddCoupling { .. }));
        assert!(matches!(
            out[24],
            CapturedDelta::ReplaySetFieldCouplingStrengthByFieldId { .. }
        ));
        assert!(matches!(
            out[25],
            CapturedDelta::ReplayRetireFieldById { .. }
        ));
        assert!(matches!(
            out[26],
            CapturedDelta::ReplayAppendNodePropertyById { .. }
        ));
        assert!(matches!(
            out[27],
            CapturedDelta::ReplayAddNodeClassificationById { .. }
        ));
        assert!(matches!(
            out[28],
            CapturedDelta::ReplayRecordNodeDerivationById { .. }
        ));
        assert!(matches!(
            out[29],
            CapturedDelta::ReplaySetEdgeSemanticPredicateByIds { .. }
        ));
        assert!(matches!(
            out[30],
            CapturedDelta::ReplayAssertSemanticPredicateByIds { .. }
        ));
        assert!(matches!(
            out[31],
            CapturedDelta::ReplayAppendFrameLayoutHintById { .. }
        ));
        assert!(matches!(
            out[32],
            CapturedDelta::ReplayAppendFrameLayoutHintById { .. }
        ));
        assert!(matches!(
            out[33],
            CapturedDelta::ReplayMoveFrameLayoutHintById { .. }
        ));
        assert!(matches!(
            out[34],
            CapturedDelta::ReplayRemoveFrameLayoutHintById { .. }
        ));
        assert!(matches!(
            out[35],
            CapturedDelta::ReplaySetFrameSplitOfferSuppressedById { .. }
        ));
        assert!(matches!(
            out[36],
            CapturedDelta::ReplayUpdateNodeHistoryById { .. }
        ));
        assert!(matches!(
            out[37],
            CapturedDelta::ReplayRetractRelationsByIds { .. }
        ));
        assert!(matches!(
            out[38],
            CapturedDelta::ReplayRemoveNodeById { .. }
        ));
    }
}
