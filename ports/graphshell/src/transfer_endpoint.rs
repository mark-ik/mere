// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read-only Graphshell projection that discloses one prepared transfer.
//!
//! The carrier vocabulary already has every operation this needs: an admitted
//! intent begins disclosure, a snapshot advertises a portable card, and
//! content-addressed resources carry the manifest and blobs independently.

use std::collections::BTreeMap;

use chirograph::{
    AdvertisedAction, BoundsRelationship, CachePolicy, CacheRetention, CardValueV1, ContentHash,
    EndpointDescriptor, IntentEffect, IntentInvocation, IntentReference, IntentResult,
    PortableCardV1, PresentationBinding, PresentationCapability, PresentationCodec,
    PresentationKey, PresentationManifest, PresentationOffer, PresentationSemantics, ProjectionAck,
    ProjectionOffer, ProjectionRequest, ProjectionSession, ProjectionSnapshot, ProtocolVersion,
    ResourceRequest, ResourceResponse, ResumeReply, ResumeRequest, SemanticRole,
};
use graphshell_endpoint::{
    IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource, ResumableProjectionSource,
};
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Representation, Scene, Score, Size2,
    SourceRef, Transform2,
};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::transfer::{TransferManifestV1, verify_manifest};

pub const TRANSFER_BEGIN_INTENT: &str = "graphshell.transfer.begin";
pub const TRANSFER_BEGIN_SCHEMA: &str = "graphshell.transfer.begin/v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferBeginV1 {
    pub transfer_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferSourceError {
    WrongSession,
    DisclosureRequired,
    MissingResource,
    InvalidManifest(String),
}

impl std::fmt::Display for TransferSourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSession => formatter.write_str("request names another transfer session"),
            Self::DisclosureRequired => {
                formatter.write_str("transfer disclosure has not been accepted")
            }
            Self::MissingResource => formatter.write_str("transfer resource is unavailable"),
            Self::InvalidManifest(error) => write!(formatter, "invalid transfer source: {error}"),
        }
    }
}

impl std::error::Error for TransferSourceError {}

/// One prepared transfer projected as a card plus independently fetchable
/// manifest and blob resources.
pub struct TransferSourceEndpoint {
    manifest: TransferManifestV1,
    session: ProjectionSession,
    snapshot: ProjectionSnapshot,
    resources: BTreeMap<ContentHash, Vec<u8>>,
    disclosed: bool,
}

impl TransferSourceEndpoint {
    pub fn new(
        manifest: TransferManifestV1,
        blobs: impl IntoIterator<Item = (eidetic::Hash, Vec<u8>)>,
    ) -> Result<Self, TransferSourceError> {
        let product = verify_manifest(&manifest)
            .map_err(|error| TransferSourceError::InvalidManifest(error.to_string()))?;
        let session = ProjectionSession(format!("graphshell:transfer:{}", manifest.transfer_id));
        let manifest_bytes = serde_json::to_vec(&manifest)
            .map_err(|error| TransferSourceError::InvalidManifest(error.to_string()))?;
        let manifest_resource = ContentHash::of(&manifest_bytes);
        let mut supplied: Vec<_> = blobs.into_iter().collect();
        let mut resources = BTreeMap::new();
        resources.insert(manifest_resource, manifest_bytes);
        let mut media = vec![manifest_resource];

        for descriptor in &manifest.blobs {
            let supplied_index = supplied
                .iter()
                .position(|(hash, _)| hash == &descriptor.content_hash)
                .ok_or_else(|| {
                    TransferSourceError::InvalidManifest(format!(
                        "blob {} was not supplied",
                        descriptor.content_hash
                    ))
                })?;
            let (_, bytes) = supplied.swap_remove(supplied_index);
            let resource = ContentHash::of(&bytes);
            if resource.0 != *descriptor.content_hash.as_bytes()
                || bytes.len() as u64 != descriptor.byte_len
            {
                return Err(TransferSourceError::InvalidManifest(format!(
                    "blob {} does not match its descriptor",
                    descriptor.content_hash
                )));
            }
            resources.insert(resource, bytes);
            media.push(resource);
        }
        if !supplied.is_empty() {
            return Err(TransferSourceError::InvalidManifest(
                "unreferenced blobs were supplied".to_string(),
            ));
        }

        let card = PortableCardV1 {
            title: "Device transfer".to_string(),
            values: vec![
                CardValueV1 {
                    label: "Operation".to_string(),
                    value: format!("{:?}", manifest.operation).to_lowercase(),
                },
                CardValueV1 {
                    label: "Objects".to_string(),
                    value: product.graph.nodes.len().to_string(),
                },
                CardValueV1 {
                    label: "Blobs".to_string(),
                    value: manifest.blobs.len().to_string(),
                },
            ],
            badges: vec!["transfer".to_string(), "content-addressed".to_string()],
            media,
        };
        let card_bytes = serde_json::to_vec(&card)
            .map_err(|error| TransferSourceError::InvalidManifest(error.to_string()))?;
        let card_resource = ContentHash::of(&card_bytes);
        resources.insert(card_resource, card_bytes);

        let mut scene = Scene::new();
        let source = scene.intern_source(SourceRef::new(
            "graphshell.transfer",
            manifest.transfer_id.to_string(),
        ));
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(0.0, 0.0),
            footprint: Footprint::Rect {
                size: Size2::new(320.0, 160.0),
            },
            representation: Representation::Card,
            layer: 0,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        let scene = SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), scene)
            .map_err(|error| TransferSourceError::InvalidManifest(format!("{error:?}")))?;
        let key = PresentationKey(format!("transfer:{}", manifest.transfer_id));
        let mut presentation = PresentationManifest {
            bindings: vec![PresentationBinding {
                instance: InstanceId(0),
                key: key.clone(),
            }],
            ..PresentationManifest::default()
        };
        presentation.offers.insert(
            key,
            vec![PresentationOffer {
                codec: PresentationCodec::PortableCardV1,
                resource: card_resource,
                byte_size: resources[&card_resource].len() as u64,
                requires: PresentationCapability::PortableCard,
                semantics: PresentationSemantics {
                    label: "Transfer selection".to_string(),
                    role: SemanticRole::Article,
                    bounds: BoundsRelationship::FillFootprint,
                    actions: vec![AdvertisedAction {
                        intent: IntentReference(TRANSFER_BEGIN_INTENT.to_string()),
                        label: "Receive transfer".to_string(),
                        explanation:
                            "Disclose this prepared selection and its addressed content blobs."
                                .to_string(),
                        payload_schema: TRANSFER_BEGIN_SCHEMA.to_string(),
                        input_form: None,
                        effect: IntentEffect::ExternalEffect,
                    }],
                },
            }],
        );
        let snapshot = ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: session.clone(),
            scene,
            presentation,
            cache_policy: CachePolicy {
                retention: CacheRetention::Exportable,
                expires_at_ms: None,
                purge_on_revocation: true,
            },
        };

        Ok(Self {
            manifest,
            session,
            snapshot,
            resources,
            disclosed: false,
        })
    }

    pub fn session(&self) -> &ProjectionSession {
        &self.session
    }

    pub fn request(&self) -> ProjectionRequest {
        ProjectionRequest {
            version: ProtocolVersion::V1,
            session: self.session.clone(),
            score: Score::new(Arrangement::Spiral(Default::default())),
        }
    }

    pub fn begin_intent(&self) -> IntentInvocation {
        IntentInvocation {
            session: self.session.clone(),
            target: InstanceId(0),
            observed_epoch: self.snapshot.scene.epoch,
            observed_revision: self.snapshot.scene.revision,
            intent: TRANSFER_BEGIN_INTENT.to_string(),
            payload: serde_json::to_vec(&TransferBeginV1 {
                transfer_id: self.manifest.transfer_id,
            })
            .expect("TransferBeginV1 always serializes"),
        }
    }
}

impl ProjectionCatalog for TransferSourceEndpoint {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Graphshell transfer source".to_string(),
            projections: vec![ProjectionOffer {
                label: "Prepared device transfer".to_string(),
                request: self.request(),
            }],
        }
    }
}

impl IntentSink for TransferSourceEndpoint {
    type Error = TransferSourceError;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        if intent.session != self.session {
            return Err(TransferSourceError::WrongSession);
        }
        if intent.observed_epoch != self.snapshot.scene.epoch
            || intent.observed_revision != self.snapshot.scene.revision
        {
            return Ok(IntentResult::Stale {
                current_epoch: self.snapshot.scene.epoch,
                current_revision: self.snapshot.scene.revision,
            });
        }
        if intent.intent != TRANSFER_BEGIN_INTENT || intent.target != InstanceId(0) {
            return Ok(IntentResult::Rejected {
                reason: "the transfer source is read-only".to_string(),
            });
        }
        let begin: TransferBeginV1 = serde_json::from_slice(&intent.payload)
            .map_err(|error| TransferSourceError::InvalidManifest(error.to_string()))?;
        if begin.transfer_id != self.manifest.transfer_id {
            return Ok(IntentResult::Rejected {
                reason: "intent names another transfer".to_string(),
            });
        }
        self.disclosed = true;
        Ok(IntentResult::Accepted)
    }
}

impl ProjectionSource for TransferSourceEndpoint {
    type Error = TransferSourceError;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session != self.session {
            return Err(TransferSourceError::WrongSession);
        }
        Ok(self.snapshot.clone())
    }
}

impl PresentationSource for TransferSourceEndpoint {
    type Error = TransferSourceError;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        if request.session != self.session {
            return Err(TransferSourceError::WrongSession);
        }
        if !self.disclosed {
            return Err(TransferSourceError::DisclosureRequired);
        }
        let bytes = self
            .resources
            .get(&request.resource)
            .cloned()
            .ok_or(TransferSourceError::MissingResource)?;
        Ok(ResourceResponse {
            session: self.session.clone(),
            resource: request.resource,
            bytes,
        })
    }
}

impl ResumableProjectionSource for TransferSourceEndpoint {
    type Error = TransferSourceError;

    fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, Self::Error> {
        if request.session != self.session {
            return Err(TransferSourceError::WrongSession);
        }
        if !self.disclosed {
            return Err(TransferSourceError::DisclosureRequired);
        }
        if request.epoch == self.snapshot.scene.epoch
            && request.revision == self.snapshot.scene.revision
        {
            return Ok(ResumeReply::Current(ProjectionAck {
                session: self.session.clone(),
                epoch: self.snapshot.scene.epoch,
                revision: self.snapshot.scene.revision,
            }));
        }
        Ok(ResumeReply::Snapshot(Box::new(self.snapshot.clone())))
    }
}

#[cfg(test)]
mod tests {
    use eidetic::{
        Codicil, Hash, ManifestId, ModerationState, PrivacyClass, ProvenanceOrigin,
        ProvenanceRecord, SchemaRef, TimeBounds, Timestamp, TrustEnvelope, TrustLevel,
    };
    use graphshell_endpoint::{
        IntentSink, PresentationSource, ProjectionSource, ResumableProjectionSource,
    };
    use mere::kernel::geometry::PortablePoint;
    use mere::kernel::graph::apply::add_node;
    use mere::kernel::graph::{Graph, NodeFacetStore};

    use super::*;
    use crate::product::{PRODUCT_CODICIL_SCHEMA, ProductCodicilV2, TransferScope};
    use crate::transfer::{
        AccessTransferPolicy, TransferBlobV1, TransferEndpointV1, TransferOperation,
        TransferRouteV1, product_codicil_schema,
    };

    fn manifest(bytes: &[u8]) -> TransferManifestV1 {
        let transfer_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let mut graph = Graph::new();
        add_node(
            &mut graph,
            Some(node_id),
            "https://example.test/h6-endpoint".to_string(),
            PortablePoint::new(0.0, 0.0),
        );
        let product = ProductCodicilV2 {
            schema: PRODUCT_CODICIL_SCHEMA.to_string(),
            scope: TransferScope::ObjectOnly,
            exported_at_ms: 1,
            graph: graph.to_snapshot(),
            facets: NodeFacetStore::new(),
            scene: None,
        };
        let selection_schema = product_codicil_schema();
        let schema_bytes = serde_json::to_vec(&selection_schema).unwrap();
        TransferManifestV1 {
            schema: crate::transfer::TRANSFER_MANIFEST_SCHEMA.to_string(),
            transfer_id,
            operation: TransferOperation::Replicate,
            source: TransferEndpointV1 {
                graph: "graph:a".to_string(),
                persona: "persona:a".to_string(),
                device: "device:a".to_string(),
            },
            destination: TransferEndpointV1 {
                graph: "graph:b".to_string(),
                persona: "persona:a".to_string(),
                device: "device:b".to_string(),
            },
            route: TransferRouteV1 {
                carrier: "test".to_string(),
                peer: "peer:b".to_string(),
            },
            selection_schema,
            selection: Codicil::new(
                SchemaRef::from_id(ManifestId::from_hash(Hash::of(&schema_bytes))),
                serde_json::to_vec(&product).unwrap(),
                PrivacyClass::TrustedPeersOnly,
                ProvenanceRecord {
                    origin: ProvenanceOrigin::Generated,
                    upstream: Vec::new(),
                    tooling: None,
                    generated_at: Timestamp(1),
                },
                TrustEnvelope {
                    level: TrustLevel::SelfAsserted,
                    signatures: Vec::new(),
                    moderation_state: ModerationState::Unreviewed,
                },
                TimeBounds::at(Timestamp(1)),
            ),
            blobs: vec![TransferBlobV1 {
                node_id,
                role: "primary".to_string(),
                media_type: "application/octet-stream".to_string(),
                content_hash: Hash::of(bytes),
                byte_len: bytes.len() as u64,
            }],
            access_policy: AccessTransferPolicy::ExcludeSourceHistory,
            access_records: Vec::new(),
        }
    }

    #[test]
    fn transfer_disclosure_is_intent_gated_and_resumable() {
        let bytes = b"portable file bytes".to_vec();
        let manifest = manifest(&bytes);
        let hash = manifest.blobs[0].content_hash;
        let mut endpoint = TransferSourceEndpoint::new(manifest, [(hash, bytes.clone())]).unwrap();

        let snapshot = endpoint.snapshot(endpoint.request()).unwrap();
        let card_offer = &snapshot.presentation.offers_for(InstanceId(0)).unwrap()[0];
        assert!(matches!(
            endpoint.resource(ResourceRequest {
                session: endpoint.session().clone(),
                resource: card_offer.resource,
            }),
            Err(TransferSourceError::DisclosureRequired)
        ));
        assert_eq!(
            endpoint.invoke(endpoint.begin_intent()).unwrap(),
            IntentResult::Accepted
        );
        let card_response = endpoint
            .resource(ResourceRequest {
                session: endpoint.session().clone(),
                resource: card_offer.resource,
            })
            .unwrap();
        assert!(card_response.has_valid_address());
        let card: PortableCardV1 = serde_json::from_slice(&card_response.bytes).unwrap();
        assert_eq!(card.media.len(), 2, "manifest followed by one blob");

        let blob = endpoint
            .resource(ResourceRequest {
                session: endpoint.session().clone(),
                resource: card.media[1],
            })
            .unwrap();
        assert_eq!(blob.bytes, bytes);
        assert!(blob.has_valid_address());
        assert!(matches!(
            endpoint
                .resume(ResumeRequest {
                    session: endpoint.session().clone(),
                    epoch: SceneEpoch(1),
                    revision: Revision(1),
                })
                .unwrap(),
            ResumeReply::Current(_)
        ));
    }
}
