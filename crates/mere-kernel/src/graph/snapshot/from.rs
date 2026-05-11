/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `Graph::from_snapshot` — reconstruct the runtime graph from a
//! persisted [`GraphSnapshot`].
//!
//! Extracted from `graph/snapshot.rs` per the 2026-05-11 kernel
//! decomposition pass.

use std::collections::HashMap;
use std::time::SystemTime;

use euclid::default::{Point2D, Vector2D};
use uuid::Uuid;

use super::super::*;
use super::containment_parent_url;
use crate::address::{address_from_url, cached_host_from_url, detect_mime};
use crate::persistence::{
    GraphSnapshot, PersistedArrangementSubKind, PersistedContainmentSubKind, PersistedEdgeFamily,
    PersistedImportedSubKind, PersistedNavigationTrigger, PersistedNodeSessionState,
    PersistedProvenanceSubKind, PersistedSemanticSubKind,
};

impl Graph {
    pub fn from_snapshot(snapshot: &GraphSnapshot) -> Self {
        let mut graph = Graph::new();

        for pnode in &snapshot.nodes {
            let Ok(node_id) = Uuid::parse_str(&pnode.node_id) else {
                continue;
            };
            // Prefer the typed address field (Stage C.2+); fall back to the legacy
            // `url` string field for snapshots written before Stage C.2.
            let node_url = {
                let from_address = pnode.address.as_url_str();
                if from_address.is_empty() {
                    pnode.url.clone()
                } else {
                    from_address.to_string()
                }
            };
            let key = graph.add_node_with_id(
                node_id,
                node_url,
                Point2D::new(pnode.position_x, pnode.position_y),
            );
            let mut restore_url_from_session: Option<String> = None;
            if let Some(node) = graph.inner.node_weight_mut(key) {
                node.title = pnode.title.clone();
                node.cached_host = pnode
                    .cached_host
                    .clone()
                    .or_else(|| cached_host_from_url(node.address.as_url_str()));
                node.tags = pnode.tags.iter().cloned().collect();
                node.tag_presentation = pnode.tag_presentation.clone();
                node.import_provenance = pnode.import_provenance.clone();
                node.classifications = pnode.classifications.clone();
                node.is_pinned = pnode.is_pinned;
                node.navigation_memory = pnode.navigation_memory.clone();
                node.thumbnail_png = pnode.thumbnail_png.clone();
                node.thumbnail_width = pnode.thumbnail_width;
                node.thumbnail_height = pnode.thumbnail_height;
                node.favicon_rgba = pnode.favicon_rgba.clone();
                node.favicon_width = pnode.favicon_width;
                node.favicon_height = pnode.favicon_height;
                node.mime_hint = pnode.mime_hint.clone();
                // address was already set by add_node_with_id from pnode.url; no re-derivation needed.
                node.frame_layout_hints = pnode.frame_layout_hints.clone();
                node.frame_split_offer_suppressed = pnode.frame_split_offer_suppressed;
                restore_url_from_session = node.current_history_url();
                if let Some(session) = &pnode.session_state {
                    node.session_scroll = session.scroll_x.zip(session.scroll_y);
                    node.session_form_draft = session.form_draft.clone();
                }
            }
            if let Some(current_url) = restore_url_from_session
                && !current_url.is_empty()
            {
                let preserve_route_identity = graph
                    .inner
                    .node_weight(key)
                    .is_some_and(|node| node.address.address_kind() == AddressKind::GraphshellClip);
                if !preserve_route_identity {
                    // Recompute MIME hint and address from the restored URL.
                    if let Some(node) = graph.inner.node_weight_mut(key) {
                        node.mime_hint = detect_mime(&current_url, None);
                        node.address = address_from_url(&current_url);
                    }
                    let _ = graph.update_node_url(key, current_url);
                }
            }
        }

        for pedge in &snapshot.edges {
            let from_key = Uuid::parse_str(&pedge.from_node_id)
                .ok()
                .and_then(|id| graph.get_node_key_by_id(id));
            let to_key = Uuid::parse_str(&pedge.to_node_id)
                .ok()
                .and_then(|id| graph.get_node_key_by_id(id));
            if let (Some(from), Some(to)) = (from_key, to_key) {
                if let Some(semantic) = &pedge.semantic {
                    for sub_kind in &semantic.sub_kinds {
                        let assertion = EdgeAssertion::Semantic {
                            sub_kind: match sub_kind {
                                PersistedSemanticSubKind::Hyperlink => SemanticSubKind::Hyperlink,
                                PersistedSemanticSubKind::UserGrouped => {
                                    SemanticSubKind::UserGrouped
                                }
                                PersistedSemanticSubKind::AgentDerived => {
                                    SemanticSubKind::AgentDerived
                                }
                                PersistedSemanticSubKind::Cites => SemanticSubKind::Cites,
                                PersistedSemanticSubKind::Quotes => SemanticSubKind::Quotes,
                                PersistedSemanticSubKind::Summarizes => SemanticSubKind::Summarizes,
                                PersistedSemanticSubKind::Elaborates => SemanticSubKind::Elaborates,
                                PersistedSemanticSubKind::ExampleOf => SemanticSubKind::ExampleOf,
                                PersistedSemanticSubKind::Supports => SemanticSubKind::Supports,
                                PersistedSemanticSubKind::Contradicts => {
                                    SemanticSubKind::Contradicts
                                }
                                PersistedSemanticSubKind::Questions => SemanticSubKind::Questions,
                                PersistedSemanticSubKind::SameEntityAs => {
                                    SemanticSubKind::SameEntityAs
                                }
                                PersistedSemanticSubKind::DuplicateOf => {
                                    SemanticSubKind::DuplicateOf
                                }
                                PersistedSemanticSubKind::CanonicalMirrorOf => {
                                    SemanticSubKind::CanonicalMirrorOf
                                }
                                PersistedSemanticSubKind::DependsOn => SemanticSubKind::DependsOn,
                                PersistedSemanticSubKind::Blocks => SemanticSubKind::Blocks,
                                PersistedSemanticSubKind::NextStep => SemanticSubKind::NextStep,
                            },
                            label: semantic.label.clone(),
                            decay_progress: semantic.agent_decay_progress,
                        };
                        let _ = graph.assert_relation(from, to, assertion);
                    }
                }
                if let Some(arrangement) = &pedge.arrangement {
                    for sub_kind in &arrangement.sub_kinds {
                        let edge_type = match sub_kind {
                            PersistedArrangementSubKind::FrameMember => {
                                EdgeType::ArrangementRelation(ArrangementSubKind::FrameMember)
                            }
                            PersistedArrangementSubKind::TileGroup => {
                                EdgeType::ArrangementRelation(ArrangementSubKind::TileGroup)
                            }
                            PersistedArrangementSubKind::SplitPair => {
                                EdgeType::ArrangementRelation(ArrangementSubKind::SplitPair)
                            }
                            PersistedArrangementSubKind::TabNeighbor
                            | PersistedArrangementSubKind::ActiveTab
                            | PersistedArrangementSubKind::PinnedInFrame => continue,
                        };
                        let _ = graph.add_edge(from, to, edge_type, None);
                    }
                }
                if let Some(containment) = &pedge.containment {
                    for sub_kind in &containment.sub_kinds {
                        let edge_type = match sub_kind {
                            PersistedContainmentSubKind::UrlPath => {
                                EdgeType::ContainmentRelation(ContainmentSubKind::UrlPath)
                            }
                            PersistedContainmentSubKind::Domain => {
                                EdgeType::ContainmentRelation(ContainmentSubKind::Domain)
                            }
                            _ => continue,
                        };
                        let _ = graph.add_edge(from, to, edge_type, None);
                    }
                }
                if let Some(imported) = &pedge.imported {
                    for sub_kind in &imported.sub_kinds {
                        let assertion = match sub_kind {
                            PersistedImportedSubKind::BookmarkFolder => EdgeAssertion::Imported {
                                sub_kind: ImportedSubKind::BookmarkFolder,
                            },
                            PersistedImportedSubKind::HistoryImport => EdgeAssertion::Imported {
                                sub_kind: ImportedSubKind::HistoryImport,
                            },
                            PersistedImportedSubKind::SessionImport => EdgeAssertion::Imported {
                                sub_kind: ImportedSubKind::SessionImport,
                            },
                            PersistedImportedSubKind::RssMembership => EdgeAssertion::Imported {
                                sub_kind: ImportedSubKind::RssMembership,
                            },
                            PersistedImportedSubKind::FileSystemImport => EdgeAssertion::Imported {
                                sub_kind: ImportedSubKind::FileSystemImport,
                            },
                            PersistedImportedSubKind::ArchiveMembership => {
                                EdgeAssertion::Imported {
                                    sub_kind: ImportedSubKind::ArchiveMembership,
                                }
                            }
                            PersistedImportedSubKind::SharedCollection => EdgeAssertion::Imported {
                                sub_kind: ImportedSubKind::SharedCollection,
                            },
                        };
                        let _ = graph.assert_relation(from, to, assertion);
                    }
                }
                if let Some(provenance) = &pedge.provenance {
                    for sub_kind in &provenance.sub_kinds {
                        let assertion = match sub_kind {
                            PersistedProvenanceSubKind::ClippedFrom => EdgeAssertion::Provenance {
                                sub_kind: ProvenanceSubKind::ClippedFrom,
                            },
                            PersistedProvenanceSubKind::ExcerptedFrom => {
                                EdgeAssertion::Provenance {
                                    sub_kind: ProvenanceSubKind::ExcerptedFrom,
                                }
                            }
                            PersistedProvenanceSubKind::SummarizedFrom => {
                                EdgeAssertion::Provenance {
                                    sub_kind: ProvenanceSubKind::SummarizedFrom,
                                }
                            }
                            PersistedProvenanceSubKind::TranslatedFrom => {
                                EdgeAssertion::Provenance {
                                    sub_kind: ProvenanceSubKind::TranslatedFrom,
                                }
                            }
                            PersistedProvenanceSubKind::RewrittenFrom => {
                                EdgeAssertion::Provenance {
                                    sub_kind: ProvenanceSubKind::RewrittenFrom,
                                }
                            }
                            PersistedProvenanceSubKind::GeneratedFrom => {
                                EdgeAssertion::Provenance {
                                    sub_kind: ProvenanceSubKind::GeneratedFrom,
                                }
                            }
                            PersistedProvenanceSubKind::ExtractedFrom => {
                                EdgeAssertion::Provenance {
                                    sub_kind: ProvenanceSubKind::ExtractedFrom,
                                }
                            }
                            PersistedProvenanceSubKind::ImportedFromSource => {
                                EdgeAssertion::Provenance {
                                    sub_kind: ProvenanceSubKind::ImportedFromSource,
                                }
                            }
                        };
                        let _ = graph.assert_relation(from, to, assertion);
                    }
                }
                if let Some(traversal) = &pedge.traversal {
                    let _ = graph.add_edge(from, to, EdgeType::History, None);
                    if let Some(edge_key) = graph.find_edge_key(from, to)
                        && let Some(payload) = graph.inner.edge_weight_mut(edge_key)
                        && let Some(data) = payload.traversal.as_mut()
                    {
                        data.traversals = traversal
                            .traversals
                            .iter()
                            .map(|record| Traversal {
                                timestamp_ms: record.timestamp_ms,
                                trigger: match record.trigger {
                                    PersistedNavigationTrigger::Unknown => {
                                        NavigationTrigger::Unknown
                                    }
                                    PersistedNavigationTrigger::LinkClick => {
                                        NavigationTrigger::LinkClick
                                    }
                                    PersistedNavigationTrigger::Back => NavigationTrigger::Back,
                                    PersistedNavigationTrigger::Forward => {
                                        NavigationTrigger::Forward
                                    }
                                    PersistedNavigationTrigger::AddressBarEntry => {
                                        NavigationTrigger::AddressBarEntry
                                    }
                                    PersistedNavigationTrigger::PanePromotion => {
                                        NavigationTrigger::PanePromotion
                                    }
                                    PersistedNavigationTrigger::Programmatic => {
                                        NavigationTrigger::Programmatic
                                    }
                                },
                            })
                            .collect();
                        data.metrics = EdgeMetrics {
                            total_navigations: traversal.metrics.total_navigations,
                            forward_navigations: traversal.metrics.forward_navigations,
                            backward_navigations: traversal.metrics.backward_navigations,
                            last_navigated_at: traversal.metrics.last_navigated_at,
                        };
                    }
                }
            }
        }

        if snapshot.import_records.is_empty() {
            graph.rebuild_import_records_from_node_provenance(snapshot.timestamp_secs);
        } else {
            graph.import_records = snapshot.import_records.clone();
            normalize_import_records(&mut graph.import_records);
            graph.sync_node_import_provenance_from_records();
        }

        graph.rebuild_derived_containment_relations();

        graph
    }

}
