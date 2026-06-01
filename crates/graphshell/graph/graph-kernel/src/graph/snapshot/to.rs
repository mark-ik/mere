/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `Graph::to_snapshot` — serialize the runtime graph into a
//! [`GraphSnapshot`] for persistence.
//!
//! Extracted from `graph/snapshot.rs` per the 2026-05-11 kernel
//! decomposition pass (the original file was 724 LOC).

use petgraph::visit::EdgeRef;

use super::super::*;
use crate::persistence::{
    GraphSnapshot, PersistedAddress, PersistedArrangementEdgeData, PersistedArrangementSubKind,
    PersistedContainmentEdgeData, PersistedContainmentSubKind, PersistedCoupling,
    PersistedCouplingResponse, PersistedEdge, PersistedEdgeFamily, PersistedField,
    PersistedFieldExtent, PersistedFieldLifecycle, PersistedImportedEdgeData,
    PersistedImportedSubKind, PersistedNavigationTrigger, PersistedNode, PersistedNodeSelector,
    PersistedNodeSessionState, PersistedProvenanceEdgeData, PersistedProvenanceSubKind,
    PersistedSemanticEdgeData, PersistedSemanticSubKind, PersistedTraversalEdgeData,
    PersistedTraversalMetrics, PersistedTraversalRecord,
};
use crate::types::format_imported_at_secs;

impl Graph {
    pub fn to_snapshot(&self) -> GraphSnapshot {
        let nodes = self
            .nodes()
            .map(|(_, node)| PersistedNode {
                node_id: node.id.to_string(),
                cached_host: node.cached_host.clone(),
                title: node.title.clone(),
                position_x: node.committed_position.x,
                position_y: node.committed_position.y,
                tags: {
                    let mut tags = node.tags.iter().cloned().collect::<Vec<_>>();
                    tags.sort();
                    tags
                },
                tag_presentation: node.tag_presentation.clone(),
                import_provenance: node.import_provenance.clone(),
                is_pinned: node.is_pinned,
                navigation_memory: node.navigation_memory.clone(),
                thumbnail_png: node.thumbnail_png.clone(),
                thumbnail_width: node.thumbnail_width,
                thumbnail_height: node.thumbnail_height,
                favicon_rgba: node.favicon_rgba.clone(),
                favicon_width: node.favicon_width,
                favicon_height: node.favicon_height,
                session_state: Some(PersistedNodeSessionState {
                    scroll_x: node.session_scroll.map(|(x, _)| x),
                    scroll_y: node.session_scroll.map(|(_, y)| y),
                    form_draft: node.session_form_draft.clone(),
                }),
                address: match node.primary_address() {
                    Address::Http(s) => PersistedAddress::Http(s.clone()),
                    Address::File(s) => PersistedAddress::File(s.clone()),
                    Address::Data(s) => PersistedAddress::Data(s.clone()),
                    Address::Clip(s) => PersistedAddress::Clip(s.clone()),
                    Address::Directory(s) => PersistedAddress::Directory(s.clone()),
                    Address::Custom(s) => PersistedAddress::Custom(s.clone()),
                },
                // Written for backward compat: pre-Stage C.2 readers use this field.
                url: node.primary_address().as_url_str().to_string(),
                classifications: node.classifications.clone(),
                mime_hint: node.mime_hint.clone(),
                frame_layout_hints: node.frame_layout_hints.clone(),
                frame_split_offer_suppressed: node.frame_split_offer_suppressed,
            })
            .collect();

        let edges = self
            .inner
            .edge_references()
            .map(|edge| {
                let from_node_id = self
                    .get_node(edge.source())
                    .map(|n| n.id.to_string())
                    .unwrap_or_default();
                let to_node_id = self
                    .get_node(edge.target())
                    .map(|n| n.id.to_string())
                    .unwrap_or_default();
                let payload = edge.weight();
                PersistedEdge {
                    from_node_id,
                    to_node_id,
                    families: payload
                        .families()
                        .iter()
                        .map(|family| match family {
                            EdgeFamily::Semantic => PersistedEdgeFamily::Semantic,
                            EdgeFamily::Traversal => PersistedEdgeFamily::Traversal,
                            EdgeFamily::Containment => PersistedEdgeFamily::Containment,
                            EdgeFamily::Arrangement => PersistedEdgeFamily::Arrangement,
                            EdgeFamily::Imported => PersistedEdgeFamily::Imported,
                            EdgeFamily::Provenance => PersistedEdgeFamily::Provenance,
                        })
                        .collect(),
                    semantic: Some(PersistedSemanticEdgeData {
                        sub_kinds: payload
                            .semantic_data()
                            .map(|data| {
                                data.sub_kinds
                                    .iter()
                                    .copied()
                                    .map(|sub_kind| match sub_kind {
                                        SemanticSubKind::Hyperlink => {
                                            PersistedSemanticSubKind::Hyperlink
                                        }
                                        SemanticSubKind::UserGrouped => {
                                            PersistedSemanticSubKind::UserGrouped
                                        }
                                        SemanticSubKind::AgentDerived => {
                                            PersistedSemanticSubKind::AgentDerived
                                        }
                                        SemanticSubKind::Cites => PersistedSemanticSubKind::Cites,
                                        SemanticSubKind::Quotes => PersistedSemanticSubKind::Quotes,
                                        SemanticSubKind::Summarizes => {
                                            PersistedSemanticSubKind::Summarizes
                                        }
                                        SemanticSubKind::Elaborates => {
                                            PersistedSemanticSubKind::Elaborates
                                        }
                                        SemanticSubKind::ExampleOf => {
                                            PersistedSemanticSubKind::ExampleOf
                                        }
                                        SemanticSubKind::Supports => {
                                            PersistedSemanticSubKind::Supports
                                        }
                                        SemanticSubKind::Contradicts => {
                                            PersistedSemanticSubKind::Contradicts
                                        }
                                        SemanticSubKind::Questions => {
                                            PersistedSemanticSubKind::Questions
                                        }
                                        SemanticSubKind::SameEntityAs => {
                                            PersistedSemanticSubKind::SameEntityAs
                                        }
                                        SemanticSubKind::DuplicateOf => {
                                            PersistedSemanticSubKind::DuplicateOf
                                        }
                                        SemanticSubKind::CanonicalMirrorOf => {
                                            PersistedSemanticSubKind::CanonicalMirrorOf
                                        }
                                        SemanticSubKind::DependsOn => {
                                            PersistedSemanticSubKind::DependsOn
                                        }
                                        SemanticSubKind::Blocks => PersistedSemanticSubKind::Blocks,
                                        SemanticSubKind::NextStep => {
                                            PersistedSemanticSubKind::NextStep
                                        }
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        label: payload.semantic_data().and_then(|data| data.label.clone()),
                        agent_decay_progress: payload
                            .has_relation(RelationSelector::Semantic(SemanticSubKind::AgentDerived))
                            .then_some(0.0),
                    })
                    .filter(|data| !data.sub_kinds.is_empty() || data.label.is_some()),
                    traversal: payload
                        .traversal_data()
                        .map(|data| PersistedTraversalEdgeData {
                            traversals: data
                                .traversals
                                .iter()
                                .map(|traversal| PersistedTraversalRecord {
                                    timestamp_ms: traversal.timestamp_ms,
                                    trigger: match traversal.trigger {
                                        NavigationTrigger::Unknown => {
                                            PersistedNavigationTrigger::Unknown
                                        }
                                        NavigationTrigger::LinkClick => {
                                            PersistedNavigationTrigger::LinkClick
                                        }
                                        NavigationTrigger::Back => PersistedNavigationTrigger::Back,
                                        NavigationTrigger::Forward => {
                                            PersistedNavigationTrigger::Forward
                                        }
                                        NavigationTrigger::AddressBarEntry => {
                                            PersistedNavigationTrigger::AddressBarEntry
                                        }
                                        NavigationTrigger::PanePromotion => {
                                            PersistedNavigationTrigger::PanePromotion
                                        }
                                        NavigationTrigger::Programmatic => {
                                            PersistedNavigationTrigger::Programmatic
                                        }
                                        NavigationTrigger::Redirect => {
                                            PersistedNavigationTrigger::Redirect
                                        }
                                        NavigationTrigger::ReopenSession => {
                                            PersistedNavigationTrigger::ReopenSession
                                        }
                                        NavigationTrigger::JumpAnchor => {
                                            PersistedNavigationTrigger::JumpAnchor
                                        }
                                        NavigationTrigger::InPageSearchJump => {
                                            PersistedNavigationTrigger::InPageSearchJump
                                        }
                                        NavigationTrigger::ImportedHistory => {
                                            PersistedNavigationTrigger::ImportedHistory
                                        }
                                    },
                                })
                                .collect(),
                            metrics: PersistedTraversalMetrics {
                                total_navigations: data.metrics.total_navigations,
                                forward_navigations: data.metrics.forward_navigations,
                                backward_navigations: data.metrics.backward_navigations,
                                last_navigated_at: data.metrics.last_navigated_at,
                            },
                        }),
                    containment: payload.containment_data().map(|data| {
                        PersistedContainmentEdgeData {
                            sub_kinds: data
                                .sub_kinds
                                .iter()
                                .map(|sub_kind| match sub_kind {
                                    ContainmentSubKind::UrlPath => {
                                        PersistedContainmentSubKind::UrlPath
                                    }
                                    ContainmentSubKind::Domain => {
                                        PersistedContainmentSubKind::Domain
                                    }
                                    ContainmentSubKind::FileSystem => {
                                        PersistedContainmentSubKind::FileSystem
                                    }
                                    ContainmentSubKind::UserFolder => {
                                        PersistedContainmentSubKind::UserFolder
                                    }
                                    ContainmentSubKind::ClipSource => {
                                        PersistedContainmentSubKind::ClipSource
                                    }
                                    ContainmentSubKind::NotebookSection => {
                                        PersistedContainmentSubKind::NotebookSection
                                    }
                                    ContainmentSubKind::CollectionMember => {
                                        PersistedContainmentSubKind::CollectionMember
                                    }
                                })
                                .collect(),
                        }
                    }),
                    arrangement: payload.arrangement_data().map(|data| {
                        PersistedArrangementEdgeData {
                            sub_kinds: data
                                .sub_kinds
                                .iter()
                                .copied()
                                .filter(|sub_kind| {
                                    sub_kind.durability() == RelationDurability::Durable
                                })
                                .map(|sub_kind| match sub_kind {
                                    ArrangementSubKind::FrameMember => {
                                        PersistedArrangementSubKind::FrameMember
                                    }
                                    ArrangementSubKind::TileGroup => {
                                        PersistedArrangementSubKind::TileGroup
                                    }
                                    ArrangementSubKind::SplitPair => {
                                        PersistedArrangementSubKind::SplitPair
                                    }
                                })
                                .collect(),
                        }
                    }),
                    imported: payload
                        .imported_data()
                        .map(|data| PersistedImportedEdgeData {
                            sub_kinds: data
                                .sub_kinds
                                .iter()
                                .map(|sub_kind| match sub_kind {
                                    ImportedSubKind::BookmarkFolder => {
                                        PersistedImportedSubKind::BookmarkFolder
                                    }
                                    ImportedSubKind::HistoryImport => {
                                        PersistedImportedSubKind::HistoryImport
                                    }
                                    ImportedSubKind::SessionImport => {
                                        PersistedImportedSubKind::SessionImport
                                    }
                                    ImportedSubKind::RssMembership => {
                                        PersistedImportedSubKind::RssMembership
                                    }
                                    ImportedSubKind::FileSystemImport => {
                                        PersistedImportedSubKind::FileSystemImport
                                    }
                                    ImportedSubKind::ArchiveMembership => {
                                        PersistedImportedSubKind::ArchiveMembership
                                    }
                                    ImportedSubKind::SharedCollection => {
                                        PersistedImportedSubKind::SharedCollection
                                    }
                                })
                                .collect(),
                        }),
                    provenance: payload
                        .provenance_data()
                        .map(|data| PersistedProvenanceEdgeData {
                            sub_kinds: data
                                .sub_kinds
                                .iter()
                                .map(|sub_kind| match sub_kind {
                                    ProvenanceSubKind::ClippedFrom => {
                                        PersistedProvenanceSubKind::ClippedFrom
                                    }
                                    ProvenanceSubKind::ExcerptedFrom => {
                                        PersistedProvenanceSubKind::ExcerptedFrom
                                    }
                                    ProvenanceSubKind::SummarizedFrom => {
                                        PersistedProvenanceSubKind::SummarizedFrom
                                    }
                                    ProvenanceSubKind::TranslatedFrom => {
                                        PersistedProvenanceSubKind::TranslatedFrom
                                    }
                                    ProvenanceSubKind::RewrittenFrom => {
                                        PersistedProvenanceSubKind::RewrittenFrom
                                    }
                                    ProvenanceSubKind::GeneratedFrom => {
                                        PersistedProvenanceSubKind::GeneratedFrom
                                    }
                                    ProvenanceSubKind::ExtractedFrom => {
                                        PersistedProvenanceSubKind::ExtractedFrom
                                    }
                                    ProvenanceSubKind::ImportedFromSource => {
                                        PersistedProvenanceSubKind::ImportedFromSource
                                    }
                                })
                                .collect(),
                        }),
                }
            })
            .collect();

        let fields = self
            .fields()
            .map(|f| PersistedField {
                id: f.id.as_uuid().to_string(),
                name: f.name.clone(),
                // The recursive AST rides as a JSON blob (see persistence_fields).
                definition_json: serde_json::to_string(&f.definition).unwrap_or_default(),
                extent: match &f.extent {
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
                    FieldExtent::AttachedToNode(id) => {
                        PersistedFieldExtent::AttachedToNode(id.to_string())
                    }
                },
                lifecycle: match f.lifecycle {
                    FieldLifecycle::Active => PersistedFieldLifecycle::Active,
                    FieldLifecycle::Retired => PersistedFieldLifecycle::Retired,
                },
            })
            .collect();

        let couplings = self
            .couplings()
            .map(|c| PersistedCoupling {
                id: c.id.as_uuid().to_string(),
                field_id: c.field.as_uuid().to_string(),
                selector: match &c.selector {
                    NodeSelector::All => PersistedNodeSelector::All,
                    NodeSelector::Tagged(t) => PersistedNodeSelector::Tagged(t.clone()),
                    NodeSelector::Kind(k) => PersistedNodeSelector::Kind(k.clone()),
                    NodeSelector::NotTagged(t) => PersistedNodeSelector::NotTagged(t.clone()),
                },
                response: match c.response {
                    CouplingResponse::AttractToMin => PersistedCouplingResponse::AttractToMin,
                    CouplingResponse::RepelFromMax => PersistedCouplingResponse::RepelFromMax,
                    CouplingResponse::AlignVelocity => PersistedCouplingResponse::AlignVelocity,
                    CouplingResponse::FlowAdvect => PersistedCouplingResponse::FlowAdvect,
                    CouplingResponse::DampenInside { factor } => {
                        PersistedCouplingResponse::DampenInside { factor }
                    }
                    CouplingResponse::ContainmentWall => PersistedCouplingResponse::ContainmentWall,
                },
                strength: c.strength,
            })
            .collect();

        let timestamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        GraphSnapshot {
            nodes,
            edges,
            import_records: self.import_records.clone(),
            timestamp_secs,
            fields,
            couplings,
        }
    }
}
