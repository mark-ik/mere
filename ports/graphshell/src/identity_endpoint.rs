//! Graphshell endpoint adapter for the resident Personae authority.
//!
//! The browser-facing side sees ordinary portable cards and typed intents.
//! This adapter remains native because it holds the in-process authority. A
//! carrier must admit a session before passing this endpoint to
//! `serve_admitted_session`; nothing here invents a second principal field.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource};
use graphshell_protocol::{
    AdvertisedAction, BoundsRelationship, CachePolicy, ContentHash, EndpointDescriptor,
    IntentEffect, IntentInvocation, IntentReference, IntentResult, PresentationBinding,
    PresentationCapability, PresentationCodec, PresentationKey, PresentationManifest,
    PresentationOffer, PresentationSemantics, ProjectionOffer, ProjectionRequest,
    ProjectionSession, ProjectionSnapshot, ProtocolVersion, ResourceRequest, ResourceResponse,
    SemanticRole,
};
use personae::IdentityStorage;
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Rect, Representation, Scene, Score, Size2,
    SourceRef, Transform2, Vec2,
};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};

use crate::identity::IdentitySurfaceSnapshot;
use crate::identity_projection::{
    IdentityProjectionAction, SIGNING_APPROVE_IDLE_INTENT, SIGNING_APPROVE_ONCE_INTENT,
    SIGNING_DENY_INTENT, project_identity,
};
use crate::native::personae_host::PersonaeHost;

pub const IDENTITY_SESSION: &str = "native:personae";

#[derive(Debug, thiserror::Error)]
pub enum IdentityEndpointError {
    #[error("identity authority read failed: {0}")]
    Read(#[from] std::io::Error),
    #[error("identity projection serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("identity projection snapshot is invalid: {0}")]
    InvalidSnapshot(String),
    #[error("request names another projection session")]
    WrongSession,
    #[error("identity resource was not disclosed by this session")]
    MissingResource,
}

/// Native identity authority exposed through Graphshell's ordinary endpoint
/// vocabulary.
pub struct IdentityEndpoint<S: IdentityStorage> {
    host: Arc<PersonaeHost<S>>,
    session: ProjectionSession,
    epoch: u64,
    revision: u64,
    last_public_snapshot: Option<Vec<u8>>,
    resources: BTreeMap<ContentHash, Vec<u8>>,
    instance_actions: Vec<BTreeSet<String>>,
}

impl<S: IdentityStorage + 'static> IdentityEndpoint<S> {
    pub fn new(host: Arc<PersonaeHost<S>>) -> Self {
        Self::with_session(host, ProjectionSession(IDENTITY_SESSION.to_string()))
    }

    /// Bind the projection to the transcript-derived session retained after
    /// carrier admission.
    pub fn for_admitted(
        host: Arc<PersonaeHost<S>>,
        authority: &crate::lifecycle::SessionAuthority,
    ) -> Self {
        Self::with_session(host, authority.session().clone())
    }

    fn with_session(host: Arc<PersonaeHost<S>>, session: ProjectionSession) -> Self {
        Self {
            host,
            session,
            epoch: 1,
            revision: 1,
            last_public_snapshot: None,
            resources: BTreeMap::new(),
            instance_actions: Vec::new(),
        }
    }

    pub fn host(&self) -> &Arc<PersonaeHost<S>> {
        &self.host
    }

    pub fn session(&self) -> ProjectionSession {
        self.session.clone()
    }

    pub fn request(&self) -> ProjectionRequest {
        ProjectionRequest {
            version: ProtocolVersion::V1,
            session: self.session(),
            score: Score::new(Arrangement::Spiral(Default::default())),
        }
    }

    fn observe(&mut self) -> Result<IdentitySurfaceSnapshot, IdentityEndpointError> {
        let snapshot = self.host.snapshot()?;
        let public = serde_json::to_vec(&snapshot)?;
        if self
            .last_public_snapshot
            .as_ref()
            .is_some_and(|previous| previous != &public)
        {
            self.revision = self.revision.saturating_add(1);
        }
        self.last_public_snapshot = Some(public);
        Ok(snapshot)
    }

    fn build_snapshot(&mut self) -> Result<ProjectionSnapshot, IdentityEndpointError> {
        let snapshot = self.observe()?;
        let cards = project_identity(&snapshot);
        let mut scene = Scene::new();
        let mut presentation = PresentationManifest::default();
        let mut resources = BTreeMap::new();
        let mut instance_actions = Vec::with_capacity(cards.len());

        const WIDTH: f32 = 260.0;
        const HEIGHT: f32 = 136.0;
        const GAP_X: f32 = 28.0;
        const GAP_Y: f32 = 24.0;
        const COLUMNS: usize = 3;

        for (index, projected) in cards.into_iter().enumerate() {
            let instance = InstanceId(index as u32);
            let column = (index % COLUMNS) as f32;
            let row = (index / COLUMNS) as f32;
            let x = column * (WIDTH + GAP_X);
            let y = row * (HEIGHT + GAP_Y);
            let source = scene.intern_source(SourceRef::new("personae.public", projected.key));
            scene.items.push(ProjectedItem {
                source,
                space: Scene::WORLD,
                transform: Transform2::translation(x, y),
                footprint: Footprint::Rect {
                    size: Size2::new(WIDTH, HEIGHT),
                },
                representation: Representation::Card,
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            });

            let bytes = serde_json::to_vec(&projected.card)?;
            let resource = ContentHash::of(&bytes);
            let key = PresentationKey(format!("personae:card:{index}"));
            let actions = projected
                .actions
                .iter()
                .map(advertised_action)
                .collect::<Vec<_>>();
            instance_actions.push(
                projected
                    .actions
                    .into_iter()
                    .map(|action| action.intent.to_string())
                    .collect(),
            );
            presentation.bindings.push(PresentationBinding {
                instance,
                key: key.clone(),
            });
            presentation.offers.insert(
                key,
                vec![PresentationOffer {
                    codec: PresentationCodec::PortableCardV1,
                    resource,
                    byte_size: bytes.len() as u64,
                    requires: PresentationCapability::PortableCard,
                    semantics: PresentationSemantics {
                        label: projected.card.title,
                        role: SemanticRole::Article,
                        bounds: BoundsRelationship::FillFootprint,
                        actions,
                    },
                }],
            );
            resources.insert(resource, bytes);
        }

        let rows = scene.items.len().div_ceil(COLUMNS);
        let columns = scene.items.len().min(COLUMNS);
        scene.bounds = if scene.items.is_empty() {
            Rect::new(Vec2::new(0.0, 0.0), Size2::new(0.0, 0.0))
        } else {
            Rect::new(
                Vec2::new(0.0, 0.0),
                Size2::new(
                    columns as f32 * WIDTH + columns.saturating_sub(1) as f32 * GAP_X,
                    rows as f32 * HEIGHT + rows.saturating_sub(1) as f32 * GAP_Y,
                ),
            )
        };
        scene.generation = self.revision;
        let scene =
            SceneSnapshot::from_dense(SceneEpoch(self.epoch), Revision(self.revision), scene)
                .map_err(|error| IdentityEndpointError::InvalidSnapshot(format!("{error:?}")))?;

        self.resources = resources;
        self.instance_actions = instance_actions;
        Ok(ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: self.session(),
            scene,
            presentation,
            cache_policy: CachePolicy::default(),
        })
    }

    fn refresh_revision(&mut self) -> Result<(), IdentityEndpointError> {
        self.observe().map(drop)
    }

    fn mark_changed(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.last_public_snapshot = None;
    }
}

fn advertised_action(action: &IdentityProjectionAction) -> AdvertisedAction {
    let signing_decision = matches!(
        action.intent,
        SIGNING_APPROVE_ONCE_INTENT | SIGNING_APPROVE_IDLE_INTENT | SIGNING_DENY_INTENT
    );
    AdvertisedAction {
        intent: IntentReference(action.intent.to_string()),
        label: action.label.to_string(),
        explanation: if action.native_only {
            "Runs in the native Personae authority through the admitted device session.".to_string()
        } else {
            "Runs in the disclosing identity authority.".to_string()
        },
        payload_schema: action.schema.to_string(),
        effect: if signing_decision {
            IntentEffect::ExternalEffect
        } else {
            IntentEffect::DomainTruth
        },
    }
}

impl<S: IdentityStorage + 'static> ProjectionCatalog for IdentityEndpoint<S> {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Local identity authority".to_string(),
            projections: vec![ProjectionOffer {
                label: "Identity".to_string(),
                request: self.request(),
            }],
        }
    }
}

impl<S: IdentityStorage + 'static> ProjectionSource for IdentityEndpoint<S> {
    type Error = IdentityEndpointError;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session != self.session() || request.version.major != ProtocolVersion::V1.major {
            return Err(IdentityEndpointError::WrongSession);
        }
        self.build_snapshot()
    }
}

impl<S: IdentityStorage + 'static> PresentationSource for IdentityEndpoint<S> {
    type Error = IdentityEndpointError;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        if request.session != self.session() {
            return Err(IdentityEndpointError::WrongSession);
        }
        let bytes = self
            .resources
            .get(&request.resource)
            .cloned()
            .ok_or(IdentityEndpointError::MissingResource)?;
        Ok(ResourceResponse {
            session: request.session,
            resource: request.resource,
            bytes,
        })
    }
}

impl<S: IdentityStorage + 'static> IntentSink for IdentityEndpoint<S> {
    type Error = IdentityEndpointError;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        if intent.session != self.session() {
            return Err(IdentityEndpointError::WrongSession);
        }
        self.refresh_revision()?;
        if intent.observed_epoch != SceneEpoch(self.epoch)
            || intent.observed_revision != Revision(self.revision)
        {
            return Ok(IntentResult::Stale {
                current_epoch: SceneEpoch(self.epoch),
                current_revision: Revision(self.revision),
            });
        }
        let Some(actions) = self.instance_actions.get(intent.target.0 as usize) else {
            return Ok(IntentResult::Rejected {
                reason: "intent target is not in the disclosed identity scene".to_string(),
            });
        };
        if !actions.contains(&intent.intent) {
            return Ok(IntentResult::Rejected {
                reason: "intent was not advertised for the selected identity card".to_string(),
            });
        }

        match self.host.apply_intent(&intent.intent, &intent.payload) {
            Ok(_) => {
                self.mark_changed();
                Ok(IntentResult::Accepted)
            }
            Err(error) => Ok(IntentResult::Rejected {
                reason: error.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use graphshell_client::{ClientState, PresentationResolution, ResolvedContent};
    use personae::{Ed25519Keypair, IdentityVault, InMemoryStorage, Profile, ProfileId};
    use ssh_key::{Algorithm, LineEnding};

    use super::*;
    use crate::identity::VaultProtectionView;
    use crate::identity_projection::{
        GenerateSshKeyIntentV1, SSH_GENERATE_INTENT, SshUnlockPolicyIntentV1,
    };

    fn endpoint_with_private_sentinel() -> (IdentityEndpoint<InMemoryStorage>, String) {
        let mut private =
            ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        private.set_comment("endpoint-receipt");
        let private_openssh = private.to_openssh(LineEnding::LF).unwrap().to_string();
        let mut profile = Profile::new(
            ProfileId("research".to_string()),
            "Research",
            Ed25519Keypair::from_seed([0x6b; 32]),
        );
        profile.slots.insert(
            personae::ssh_slot::protocol_key_for(&private),
            personae::ssh_slot::slot_for(&private, personae::UnlockTier::PerUse).unwrap(),
        );
        let host = Arc::new(PersonaeHost::new(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            None,
            VaultProtectionView::Ephemeral,
        ));
        (IdentityEndpoint::new(host), private_openssh)
    }

    #[test]
    fn portable_client_mounts_only_public_identity_resources() {
        let (mut endpoint, private_openssh) = endpoint_with_private_sentinel();
        let snapshot = endpoint.snapshot(endpoint.request()).unwrap();
        let resources = snapshot
            .presentation
            .offers
            .values()
            .flatten()
            .map(|offer| offer.resource)
            .collect::<Vec<_>>();
        let session = snapshot.session.clone();
        let mut client = ClientState::default();
        client.apply_snapshot(snapshot).unwrap();
        for resource in resources {
            let response = endpoint
                .resource(ResourceRequest {
                    session: session.clone(),
                    resource,
                })
                .unwrap();
            let text = String::from_utf8(response.bytes.clone()).unwrap();
            assert!(!text.contains(&private_openssh));
            assert!(!text.contains("BEGIN OPENSSH PRIVATE KEY"));
            client.apply_resource(response).unwrap();
        }

        let mounted = client.mounted(&session).unwrap();
        for instance in mounted
            .scene
            .active_items_in_order()
            .into_iter()
            .map(|(id, _)| id)
        {
            assert!(matches!(
                client.resolve(
                    &session,
                    instance,
                    &graphshell_protocol::CapabilityProfile::new([
                        PresentationCapability::PortableCard,
                    ]),
                ),
                Ok(PresentationResolution::Ready(resolved))
                    if matches!(resolved.content, ResolvedContent::PortableCard(_))
            ));
        }
    }

    #[test]
    fn only_an_advertised_target_can_generate_a_key() {
        let (mut endpoint, _) = endpoint_with_private_sentinel();
        let snapshot = endpoint.snapshot(endpoint.request()).unwrap();
        let vault = snapshot
            .presentation
            .bindings
            .iter()
            .find(|binding| {
                snapshot.presentation.offers.get(&binding.key).unwrap()[0]
                    .semantics
                    .actions
                    .iter()
                    .any(|action| action.intent.0 == SSH_GENERATE_INTENT)
            })
            .unwrap()
            .instance;
        let payload = serde_json::to_vec(&GenerateSshKeyIntentV1 {
            comment: "generated through endpoint".to_string(),
            unlock_policy: SshUnlockPolicyIntentV1::Session,
        })
        .unwrap();

        let rejected = endpoint
            .invoke(IntentInvocation {
                session: snapshot.session.clone(),
                target: InstanceId(vault.0 + 1),
                observed_epoch: snapshot.scene.epoch,
                observed_revision: snapshot.scene.revision,
                intent: SSH_GENERATE_INTENT.to_string(),
                payload: payload.clone(),
            })
            .unwrap();
        assert!(matches!(rejected, IntentResult::Rejected { .. }));

        let accepted = endpoint
            .invoke(IntentInvocation {
                session: snapshot.session,
                target: vault,
                observed_epoch: snapshot.scene.epoch,
                observed_revision: snapshot.scene.revision,
                intent: SSH_GENERATE_INTENT.to_string(),
                payload,
            })
            .unwrap();
        assert_eq!(accepted, IntentResult::Accepted);
        assert_eq!(endpoint.host().snapshot().unwrap().ssh_keys.len(), 2);
    }
}
