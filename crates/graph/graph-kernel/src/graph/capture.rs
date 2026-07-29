// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Stable-id graph-delta capture/replay helpers.
//!
//! This records the graph mutations that already have stable-id replay forms in
//! [`super::apply::GraphDelta`]: add/remove node, assert/retract relation,
//! traversal append, media writes, navigation chronology/history mutations, the
//! lighter per-node content/presentation setters, semantic-predicate edge
//! writes, the node-enrichment setters, import-record truth, and deterministic
//! frame/history state setters. The broader content-mutation lane still stays
//! separate until those writes grow stable-id replay forms of their own.

use std::cell::RefCell;
use std::sync::Arc;

use chartulary::stemma::TransitionKind;
use euclid::default::Point2D;
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
use crate::types::{
    BadgeIcon, ClassificationScheme, ClassificationStatus, ImageRef, ImageRole, ImportRecord,
    NodeClassification, NodeDerivation, NodeProperty,
};

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
    ReplaySetNodeImageById {
        node_id: String,
        role: ImageRole,
        image: ImageRef,
    },
    // --- Pre-phase-2 inline imagery: readable, never written ---------------
    //
    // Journals recorded before the node-image externalization carry raw bytes
    // in these. They are kept so an existing journal still deserializes and
    // replays; `replay_delta` returns `None` for them, so the pixels are
    // dropped rather than restored. That is deliberate: a preview is
    // experience, not truth, and the next capture re-deposits it. Nothing
    // emits these variants any more.
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
    ReplaySetNodeNestedById {
        node_id: String,
        nested: Option<String>,
    },
    ReplaySetNodePinnedById {
        node_id: String,
        is_pinned: bool,
    },
    ReplaySetNodeFacetById {
        node_id: String,
        facet: String,
        value_json: String,
    },
    ReplayRemoveNodeFacetById {
        node_id: String,
        facet: String,
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
    ReplayRemoveNodeClassificationById {
        node_id: String,
        scheme: ClassificationScheme,
        value: String,
    },
    ReplaySetNodeClassificationStatusById {
        node_id: String,
        scheme: ClassificationScheme,
        value: String,
        status: ClassificationStatus,
    },
    ReplaySetNodePrimaryClassificationById {
        node_id: String,
        scheme: ClassificationScheme,
        value: String,
    },
    ReplayRecordNodeDerivationById {
        node_id: String,
        derivation: NodeDerivation,
    },
    ReplaySetNodeTagIconOverrideById {
        node_id: String,
        tag: String,
        icon: Option<BadgeIcon>,
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
    ReplaySetImportRecords {
        import_records: Vec<ImportRecord>,
    },
    ReplayTouchNodeLastVisitedById {
        node_id: String,
        timestamp_ms: u64,
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
    ReplayActivateFieldById {
        field_id: String,
    },
    ReplayRetractCouplingById {
        coupling_id: String,
    },
}

impl CapturedDelta {
    /// The graph-kernel replay delta this captured record folds back into.
    ///
    /// `None` for records with no live equivalent (pre-phase-2 inline
    /// imagery), which a replay skips.
    pub fn replay_delta(&self) -> Option<GraphDelta> {
        Some(match self {
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
            Self::ReplaySetNodeImageById {
                node_id,
                role,
                image,
            } => GraphDelta::ReplaySetNodeImageById {
                node_id: parse_uuid(node_id),
                role: *role,
                image: *image,
            },
            // Legacy inline imagery has no live delta to fold into: the kernel
            // cannot store a blob (that is async, and the store's job), so the
            // bytes are dropped and the preview regenerates on next capture.
            Self::ReplaySetNodeThumbnailById { .. } | Self::ReplaySetNodeFaviconById { .. } => {
                return None;
            }
            Self::ReplaySetNodeMimeHintById { node_id, mime_hint } => {
                GraphDelta::ReplaySetNodeMimeHintById {
                    node_id: parse_uuid(node_id),
                    mime_hint: mime_hint.clone(),
                }
            }
            Self::ReplaySetNodeNestedById { node_id, nested } => {
                GraphDelta::ReplaySetNodeNestedById {
                    node_id: parse_uuid(node_id),
                    nested: nested.clone(),
                }
            }
            Self::ReplaySetNodePinnedById { node_id, is_pinned } => {
                GraphDelta::ReplaySetNodePinnedById {
                    node_id: parse_uuid(node_id),
                    is_pinned: *is_pinned,
                }
            }
            Self::ReplaySetNodeFacetById {
                node_id,
                facet,
                value_json,
            } => {
                let value = serde_json::from_str(value_json).ok()?;
                GraphDelta::ReplaySetNodeFacetById {
                    node_id: parse_uuid(node_id),
                    facet: facet.clone(),
                    value,
                }
            }
            Self::ReplayRemoveNodeFacetById { node_id, facet } => {
                GraphDelta::ReplayRemoveNodeFacetById {
                    node_id: parse_uuid(node_id),
                    facet: facet.clone(),
                }
            }
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
            Self::ReplayRemoveNodeClassificationById {
                node_id,
                scheme,
                value,
            } => GraphDelta::ReplayRemoveNodeClassificationById {
                node_id: parse_uuid(node_id),
                scheme: scheme.clone(),
                value: value.clone(),
            },
            Self::ReplaySetNodeClassificationStatusById {
                node_id,
                scheme,
                value,
                status,
            } => GraphDelta::ReplaySetNodeClassificationStatusById {
                node_id: parse_uuid(node_id),
                scheme: scheme.clone(),
                value: value.clone(),
                status: status.clone(),
            },
            Self::ReplaySetNodePrimaryClassificationById {
                node_id,
                scheme,
                value,
            } => GraphDelta::ReplaySetNodePrimaryClassificationById {
                node_id: parse_uuid(node_id),
                scheme: scheme.clone(),
                value: value.clone(),
            },
            Self::ReplayRecordNodeDerivationById {
                node_id,
                derivation,
            } => GraphDelta::ReplayRecordNodeDerivationById {
                node_id: parse_uuid(node_id),
                derivation: derivation.clone(),
            },
            Self::ReplaySetNodeTagIconOverrideById { node_id, tag, icon } => {
                GraphDelta::ReplaySetNodeTagIconOverrideById {
                    node_id: parse_uuid(node_id),
                    tag: tag.clone(),
                    icon: icon.clone(),
                }
            }
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
            Self::ReplaySetImportRecords { import_records } => GraphDelta::ReplaySetImportRecords {
                import_records: import_records.clone(),
            },
            Self::ReplayTouchNodeLastVisitedById {
                node_id,
                timestamp_ms,
            } => GraphDelta::ReplayTouchNodeLastVisitedById {
                node_id: parse_uuid(node_id),
                timestamp_ms: *timestamp_ms,
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
            Self::ReplayActivateFieldById { field_id } => GraphDelta::ReplayActivateFieldById {
                field_id: field_id.clone(),
            },
            Self::ReplayRetractCouplingById { coupling_id } => {
                GraphDelta::ReplayRetractCouplingById {
                    coupling_id: coupling_id.clone(),
                }
            }
        })
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
    replay_captured_deltas_onto(&mut graph, deltas);
    graph
}

/// Replay a captured-delta stream against an existing graph, advancing it in
/// place. The incremental twin of [`replay_captured_deltas`]: materialize a
/// checkpoint (a `GraphSnapshot`), then apply only the journal entries recorded
/// after it. Live editing and replay both funnel through `apply_graph_delta`, so a
/// replayed graph cannot diverge from the one the edits were captured from — the
/// edit-spine invariant. (See `graph/journal.rs`.)
pub fn replay_captured_deltas_onto<I>(graph: &mut Graph, deltas: I)
where
    I: IntoIterator<Item = CapturedDelta>,
{
    for delta in deltas {
        if let Some(delta) = delta.replay_delta() {
            let _ = apply_graph_delta(graph, delta);
        }
    }
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
        BadgeIcon, ClassificationProvenance, ClassificationScheme, ClassificationStatus,
        FrameLayoutHint, ImportRecordMembership, NodeClassification, NodeDerivation,
        NodeImportProvenance, NodeProperty, SplitOrientation,
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
    fn arbitrary_facets_use_the_same_capture_and_replay_spine() {
        let captured = Arc::new(Mutex::new(Vec::<CapturedDelta>::new()));
        let sink = captured.clone();
        set_captured_delta_hook(Some(Arc::new(move |delta| {
            sink.lock().expect("capture sink").push(delta.clone());
        })));

        let id = Uuid::from_u128(0xfac7);
        let mut graph = Graph::new();
        let key = crate::graph::apply::add_node(
            &mut graph,
            Some(id),
            "mere://facet-test".into(),
            point(0.0, 0.0),
        );
        let value = serde_json::json!({"portable": true});
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeFacet {
                key,
                facet: "example.portable/v1".into(),
                value: value.clone(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeFacet {
                key,
                facet: "example.portable/v1".into(),
                value: value.clone(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::RemoveNodeFacet {
                key,
                facet: "example.portable/v1".into(),
            },
        );
        set_captured_delta_hook(None);

        let captured = captured.lock().expect("capture sink");
        assert_eq!(
            captured.len(),
            3,
            "add, first set, and remove are captured; an identical set is not"
        );
        let set_projection = replay_captured_deltas(captured[..2].iter().cloned());
        assert_eq!(
            set_projection
                .facets()
                .get(&id, &chartulary::FacetId::new("example.portable/v1"),),
            Some(&value)
        );
        let removed_projection = replay_captured_deltas(captured.iter().cloned());
        assert!(
            removed_projection
                .facets()
                .get(&id, &chartulary::FacetId::new("example.portable/v1"),)
                .is_none()
        );
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
    fn classification_admin_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplaySetNodeClassificationStatusById {
            node_id: Uuid::from_u128(61).to_string(),
            scheme: ClassificationScheme::ContentKind,
            value: "article".into(),
            status: ClassificationStatus::Verified,
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn tag_presentation_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplaySetNodeTagIconOverrideById {
            node_id: Uuid::from_u128(62).to_string(),
            tag: "paper".into(),
            icon: Some(BadgeIcon::Lucide("file-text".into())),
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn node_media_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplaySetNodeImageById {
            node_id: Uuid::from_u128(7).to_string(),
            role: ImageRole::Preview,
            image: ImageRef::new([4u8; 32], 2, 2),
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    /// A journal written before the node-image externalization must still
    /// decode — losing the ability to read an existing journal would be a
    /// far worse failure than losing a regenerable preview.
    #[test]
    fn a_legacy_inline_image_record_still_decodes_and_is_skipped_on_replay() {
        let delta = CapturedDelta::ReplaySetNodeThumbnailById {
            node_id: Uuid::from_u128(7).to_string(),
            png_bytes: vec![1, 2, 3, 4],
            width: 2,
            height: 2,
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta, "an old journal entry still reads");
        assert!(
            restored.replay_delta().is_none(),
            "it has no live delta: the pixels are dropped, not restored"
        );
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
    fn field_admin_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplayRetractCouplingById {
            coupling_id: Uuid::from_u128(91).to_string(),
        };
        let bytes = postcard::to_allocvec(&delta).expect("encode");
        let restored: CapturedDelta = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored, delta);
    }

    #[test]
    fn last_visited_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplayTouchNodeLastVisitedById {
            node_id: Uuid::from_u128(93).to_string(),
            timestamp_ms: 1_763_573_400_123,
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
    fn import_records_captured_delta_round_trips_through_postcard() {
        let delta = CapturedDelta::ReplaySetImportRecords {
            import_records: vec![ImportRecord {
                record_id: "import-record:seed".into(),
                source_id: "import:seed".into(),
                source_label: "Seed import".into(),
                imported_at_secs: 1_763_500_800,
                memberships: vec![ImportRecordMembership {
                    node_id: Uuid::from_u128(10).to_string(),
                    suppressed: false,
                }],
            }],
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
            CapturedDelta::ReplaySetNodeImageById {
                node_id: a_id.to_string(),
                role: ImageRole::Preview,
                image: ImageRef::new([7u8; 32], 1, 1),
            },
            CapturedDelta::ReplaySetNodeImageById {
                node_id: a_id.to_string(),
                role: ImageRole::Favicon,
                image: ImageRef::new([9u8; 32], 1, 1),
            },
            CapturedDelta::ReplaySetNodeMimeHintById {
                node_id: a_id.to_string(),
                mime_hint: Some("text/html".into()),
            },
            CapturedDelta::ReplaySetNodePinnedById {
                node_id: a_id.to_string(),
                is_pinned: true,
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
            CapturedDelta::ReplayTouchNodeLastVisitedById {
                node_id: a_id.to_string(),
                timestamp_ms: 1_763_573_400_123,
            },
            CapturedDelta::ReplayInsertNodeTagById {
                node_id: a_id.to_string(),
                tag: "paper".into(),
            },
            CapturedDelta::ReplaySetNodeTagIconOverrideById {
                node_id: a_id.to_string(),
                tag: "paper".into(),
                icon: Some(BadgeIcon::Lucide("file-text".into())),
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
            CapturedDelta::ReplayActivateFieldById {
                field_id: field_id.as_uuid().to_string(),
            },
            CapturedDelta::ReplayRetractCouplingById {
                coupling_id: coupling_id.as_uuid().to_string(),
            },
            CapturedDelta::ReplayAddCoupling {
                coupling: persisted_coupling_from_coupling(&sample_coupling(coupling_id, field_id)),
            },
            CapturedDelta::ReplaySetFieldCouplingStrengthByFieldId {
                field_id: field_id.as_uuid().to_string(),
                strength: 2.0,
            },
            CapturedDelta::ReplayAppendNodePropertyById {
                node_id: a_id.to_string(),
                property: NodeProperty::new(
                    "https://schema.org/datePublished".into(),
                    "2026-07-02".into(),
                ),
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
            CapturedDelta::ReplayAddNodeClassificationById {
                node_id: a_id.to_string(),
                classification: NodeClassification {
                    scheme: ClassificationScheme::ContentKind,
                    value: "essay".into(),
                    label: Some("Essay".into()),
                    confidence: 0.6,
                    provenance: ClassificationProvenance::AgentSuggested,
                    status: ClassificationStatus::Suggested,
                    primary: false,
                },
            },
            CapturedDelta::ReplayAddNodeClassificationById {
                node_id: a_id.to_string(),
                classification: NodeClassification {
                    scheme: ClassificationScheme::ContentKind,
                    value: "draft".into(),
                    label: Some("Draft".into()),
                    confidence: 0.3,
                    provenance: ClassificationProvenance::AgentSuggested,
                    status: ClassificationStatus::Suggested,
                    primary: false,
                },
            },
            CapturedDelta::ReplaySetNodeClassificationStatusById {
                node_id: a_id.to_string(),
                scheme: ClassificationScheme::ContentKind,
                value: "article".into(),
                status: ClassificationStatus::Verified,
            },
            CapturedDelta::ReplaySetNodePrimaryClassificationById {
                node_id: a_id.to_string(),
                scheme: ClassificationScheme::ContentKind,
                value: "essay".into(),
            },
            CapturedDelta::ReplayRemoveNodeClassificationById {
                node_id: a_id.to_string(),
                scheme: ClassificationScheme::ContentKind,
                value: "draft".into(),
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
            CapturedDelta::ReplaySetImportRecords {
                import_records: vec![ImportRecord {
                    record_id: "import-record:seed".into(),
                    source_id: "import:seed".into(),
                    source_label: "Seed import".into(),
                    imported_at_secs: 1_763_500_800,
                    memberships: vec![
                        ImportRecordMembership {
                            node_id: a_id.to_string(),
                            suppressed: false,
                        },
                        ImportRecordMembership {
                            node_id: c_id.to_string(),
                            suppressed: true,
                        },
                    ],
                }],
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
        assert_eq!(node.preview(), Some(&ImageRef::new([7u8; 32], 1, 1)));
        assert_eq!(node.favicon(), Some(&ImageRef::new([9u8; 32], 1, 1)));
        assert_eq!(node.media_type.as_deref(), Some("text/html"));
        assert_eq!(graph.node_is_pinned(from), Some(true));
        assert_eq!(node.body.as_deref(), Some("body"));
        assert_eq!(
            graph
                .node_last_visited(from)
                .expect("last visited facet")
                .duration_since(std::time::UNIX_EPOCH)
                .expect("last visited since epoch")
                .as_millis() as u64,
            1_763_573_400_123
        );
        assert!(!node.tags.contains("research"));
        assert!(node.tags.contains("paper"));
        assert_eq!(
            graph
                .node_tag_presentation(from)
                .unwrap()
                .icon_overrides
                .get("paper"),
            Some(&BadgeIcon::Lucide("file-text".into()))
        );
        let properties = graph.node_properties(from).unwrap();
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].predicate, "https://schema.org/datePublished");
        assert_eq!(properties[0].value, "2026-07-02");
        let classifications = graph.node_classifications(from).unwrap();
        assert_eq!(classifications.len(), 2);
        assert!(classifications.iter().any(|classification| {
            classification.value == "article"
                && classification.status == ClassificationStatus::Verified
                && !classification.primary
        }));
        assert!(classifications.iter().any(|classification| {
            classification.value == "essay"
                && classification.status == ClassificationStatus::Suggested
                && classification.primary
        }));
        assert!(
            classifications
                .iter()
                .all(|classification| classification.value != "draft")
        );
        let derivations = graph.node_derivations(from).unwrap();
        assert_eq!(derivations.len(), 1);
        assert_eq!(derivations[0].sub_kind, ProvenanceSubKind::ExtractedFrom);
        assert_eq!(
            payload
                .semantic_data()
                .and_then(|data| data.predicate.as_deref()),
            Some("https://schema.org/author")
        );
        let history_node_key = graph.get_node_key_by_id(b_id).expect("history node key");
        let history_node = graph.get_node(history_node_key).expect("history node");
        assert_eq!(history_node.url(), "https://b.test/two");
        assert_eq!(graph.node_last_session_visited(history_node_key), Some(77));
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
        assert_eq!(graph.node_last_session_visited(branched_node_key), Some(77));
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
        assert!(field.is_active());
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
        let import_records = graph.import_records();
        assert_eq!(import_records.len(), 1);
        assert_eq!(import_records[0].record_id, "import-record:seed");
        assert_eq!(
            graph.import_record_member_keys("import-record:seed"),
            vec![from]
        );
        assert_eq!(
            graph
                .node_import_provenance(from)
                .expect("import provenance"),
            [NodeImportProvenance {
                source_id: "import:seed".into(),
                source_label: "Seed import".into(),
            }]
        );
        assert!(
            graph
                .node_import_provenance(branched_node_key)
                .expect("suppressed provenance")
                .is_empty()
        );
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
            GraphDelta::SetNodeImage {
                key: a,
                role: ImageRole::Preview,
                image: ImageRef::new([7u8; 32], 1, 1),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeImage {
                key: a,
                role: ImageRole::Favicon,
                image: ImageRef::new([9u8; 32], 1, 1),
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
            GraphDelta::SetNodePinned {
                key: a,
                is_pinned: true,
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
            GraphDelta::TouchNodeLastVisited { key: a },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::InsertNodeTag {
                key: a,
                tag: "paper".into(),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeTagIconOverride {
                key: a,
                tag: "paper".into(),
                icon: Some(BadgeIcon::Lucide("file-text".into())),
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
            GraphDelta::ActivateField { id: field_id },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::RetractCoupling { id: coupling_id },
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
            GraphDelta::AppendNodeProperty {
                key: a,
                property: NodeProperty::new(
                    "https://schema.org/datePublished".into(),
                    "2026-07-02".into(),
                ),
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
            GraphDelta::AddNodeClassification {
                key: a,
                classification: NodeClassification {
                    scheme: ClassificationScheme::ContentKind,
                    value: "essay".into(),
                    label: Some("Essay".into()),
                    confidence: 0.6,
                    provenance: ClassificationProvenance::AgentSuggested,
                    status: ClassificationStatus::Suggested,
                    primary: false,
                },
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::AddNodeClassification {
                key: a,
                classification: NodeClassification {
                    scheme: ClassificationScheme::ContentKind,
                    value: "draft".into(),
                    label: Some("Draft".into()),
                    confidence: 0.3,
                    provenance: ClassificationProvenance::AgentSuggested,
                    status: ClassificationStatus::Suggested,
                    primary: false,
                },
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeClassificationStatus {
                key: a,
                scheme: ClassificationScheme::ContentKind,
                value: "article".into(),
                status: ClassificationStatus::Verified,
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodePrimaryClassification {
                key: a,
                scheme: ClassificationScheme::ContentKind,
                value: "essay".into(),
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::RemoveNodeClassification {
                key: a,
                scheme: ClassificationScheme::ContentKind,
                value: "draft".into(),
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
            GraphDelta::SetNodeImportProvenance {
                key: a,
                import_provenance: vec![NodeImportProvenance {
                    source_id: "import:seed".into(),
                    source_label: "Seed import".into(),
                }],
            },
        );
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetImportRecordMembershipSuppressed {
                record_id: "import-record:import:seed".into(),
                key: a,
                suppressed: true,
            },
        );
        let c_node_id = graph.get_node(c).expect("node c").id.to_string();
        let _ = crate::graph::apply::apply_graph_delta(
            &mut graph,
            GraphDelta::SetImportRecords {
                import_records: vec![ImportRecord {
                    record_id: "import-record:seed-two".into(),
                    source_id: "import:seed-two".into(),
                    source_label: "Seed import two".into(),
                    imported_at_secs: 1_763_500_801,
                    memberships: vec![ImportRecordMembership {
                        node_id: c_node_id,
                        suppressed: false,
                    }],
                }],
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
        assert_eq!(out.len(), 52);
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
            CapturedDelta::ReplaySetNodeImageById { .. }
        ));
        assert!(matches!(
            out[8],
            CapturedDelta::ReplaySetNodeImageById { .. }
        ));
        assert!(matches!(
            out[9],
            CapturedDelta::ReplaySetNodeMimeHintById { .. }
        ));
        assert!(matches!(
            out[10],
            CapturedDelta::ReplaySetNodePinnedById { .. }
        ));
        assert!(matches!(
            out[11],
            CapturedDelta::ReplayInsertNodeTagById { .. }
        ));
        assert!(matches!(
            out[12],
            CapturedDelta::ReplayRemoveNodeTagById { .. }
        ));
        assert!(matches!(
            out[13],
            CapturedDelta::ReplaySetNodeBodyById { .. }
        ));
        assert!(matches!(
            out[14],
            CapturedDelta::ReplayTouchNodeLastVisitedById { .. }
        ));
        assert!(matches!(
            out[15],
            CapturedDelta::ReplayInsertNodeTagById { .. }
        ));
        assert!(matches!(
            out[16],
            CapturedDelta::ReplaySetNodeTagIconOverrideById { .. }
        ));
        assert!(matches!(
            out[17],
            CapturedDelta::ReplayNavigateNodeById { .. }
        ));
        assert!(matches!(
            out[18],
            CapturedDelta::ReplayNavigateNodeById { .. }
        ));
        assert!(matches!(
            out[19],
            CapturedDelta::ReplayNodeHistoryBackById { .. }
        ));
        assert!(matches!(
            out[20],
            CapturedDelta::ReplayNodeHistoryForwardById { .. }
        ));
        assert!(matches!(
            out[21],
            CapturedDelta::ReplayBranchHistoryByIds { .. }
        ));
        assert!(matches!(
            out[22],
            CapturedDelta::ReplayNavigateNodeById { .. }
        ));
        assert!(matches!(out[23], CapturedDelta::ReplayAddField { .. }));
        assert!(matches!(out[24], CapturedDelta::ReplayAddCoupling { .. }));
        assert!(matches!(
            out[25],
            CapturedDelta::ReplaySetFieldCouplingStrengthByFieldId { .. }
        ));
        assert!(matches!(
            out[26],
            CapturedDelta::ReplayRetireFieldById { .. }
        ));
        assert!(matches!(
            out[27],
            CapturedDelta::ReplayActivateFieldById { .. }
        ));
        assert!(matches!(
            out[28],
            CapturedDelta::ReplayRetractCouplingById { .. }
        ));
        assert!(matches!(out[29], CapturedDelta::ReplayAddCoupling { .. }));
        assert!(matches!(
            out[30],
            CapturedDelta::ReplaySetFieldCouplingStrengthByFieldId { .. }
        ));
        assert!(matches!(
            out[31],
            CapturedDelta::ReplayAppendNodePropertyById { .. }
        ));
        assert!(matches!(
            out[32],
            CapturedDelta::ReplayAddNodeClassificationById { .. }
        ));
        assert!(matches!(
            out[33],
            CapturedDelta::ReplayAddNodeClassificationById { .. }
        ));
        assert!(matches!(
            out[34],
            CapturedDelta::ReplayAddNodeClassificationById { .. }
        ));
        assert!(matches!(
            out[35],
            CapturedDelta::ReplaySetNodeClassificationStatusById { .. }
        ));
        assert!(matches!(
            out[36],
            CapturedDelta::ReplaySetNodePrimaryClassificationById { .. }
        ));
        assert!(matches!(
            out[37],
            CapturedDelta::ReplayRemoveNodeClassificationById { .. }
        ));
        assert!(matches!(
            out[38],
            CapturedDelta::ReplayRecordNodeDerivationById { .. }
        ));
        assert!(matches!(
            out[39],
            CapturedDelta::ReplaySetEdgeSemanticPredicateByIds { .. }
        ));
        assert!(matches!(
            out[40],
            CapturedDelta::ReplayAssertSemanticPredicateByIds { .. }
        ));
        assert!(matches!(
            out[41],
            CapturedDelta::ReplayAppendFrameLayoutHintById { .. }
        ));
        assert!(matches!(
            out[42],
            CapturedDelta::ReplayAppendFrameLayoutHintById { .. }
        ));
        assert!(matches!(
            out[43],
            CapturedDelta::ReplayMoveFrameLayoutHintById { .. }
        ));
        assert!(matches!(
            out[44],
            CapturedDelta::ReplayRemoveFrameLayoutHintById { .. }
        ));
        assert!(matches!(
            out[45],
            CapturedDelta::ReplaySetFrameSplitOfferSuppressedById { .. }
        ));
        assert!(matches!(
            out[46],
            CapturedDelta::ReplayUpdateNodeHistoryById { .. }
        ));
        assert!(matches!(
            out[47],
            CapturedDelta::ReplaySetImportRecords { .. }
        ));
        assert!(matches!(
            out[48],
            CapturedDelta::ReplaySetImportRecords { .. }
        ));
        assert!(matches!(
            out[49],
            CapturedDelta::ReplaySetImportRecords { .. }
        ));
        assert!(matches!(
            out[50],
            CapturedDelta::ReplayRetractRelationsByIds { .. }
        ));
        assert!(matches!(
            out[51],
            CapturedDelta::ReplayRemoveNodeById { .. }
        ));
    }
}
