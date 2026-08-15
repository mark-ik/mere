//! Graphshell's reference-host composition over local and remote projections.

use graphshell_client::{ClientState, PresentationResolution, ResolvedContent};
use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionSource};
use chirograph::{
    CacheRetention, CapabilityProfile, ContentHash, IntentInvocation, IntentResult,
    PresentationCapability, ProjectionSession, ProjectionSnapshot, ResourceRequest,
};
use muniment::Backend;
use sceno::InstanceId;

use crate::access::AccessContext;
use crate::handlers::{OpenAddressV1, intent_id};
use crate::mere_host::{
    FIXTURE_DEVICE_TWO_ADDRESS, FIXTURE_PERSONA_ADDRESS, HOST_SLOT, MereHost, MereHostError,
    SelectedPersonaRef, fixture_handlers,
};
use crate::product::{
    PINNED_PROJECTION_FACET, PinnedProjectionAuthorityV1, PinnedProjectionCardV1,
};

#[derive(Debug)]
pub enum AppError {
    Host(MereHostError),
    Client(String),
    LocalProjectionMissing,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(error) => write!(formatter, "{error}"),
            Self::Client(error) => write!(formatter, "Graphshell client: {error}"),
            Self::LocalProjectionMissing => {
                write!(formatter, "local Mere projection is not mounted")
            }
        }
    }
}

impl std::error::Error for AppError {}

impl From<MereHostError> for AppError {
    fn from(value: MereHostError) -> Self {
        Self::Host(value)
    }
}

/// One Graphshell process: a local Mere endpoint and the same portable client
/// state used for remote endpoints.
pub struct GraphshellApp<B> {
    pub host: MereHost<B>,
    pub client: ClientState,
}

impl<B: Backend> GraphshellApp<B> {
    pub fn new(host: MereHost<B>) -> Self {
        Self {
            host,
            client: ClientState::default(),
        }
    }

    pub fn fixture(backend: B, selected_persona: SelectedPersonaRef) -> Result<Self, AppError> {
        Ok(Self::new(MereHost::fixture(
            backend,
            selected_persona,
            fixture_handlers(),
        )?))
    }

    /// Reopen Graphshell's durable browser document, seeding the reference
    /// fixture only when this backend has never stored one.
    pub async fn open_or_fixture(
        backend: B,
        selected_persona: SelectedPersonaRef,
    ) -> Result<Self, AppError> {
        if backend
            .get(HOST_SLOT)
            .await
            .map_err(MereHostError::from)?
            .is_none()
        {
            return Self::fixture(backend, selected_persona);
        }
        let host = MereHost::open(
            backend,
            selected_persona.clone(),
            fixture_handlers(),
            AccessContext {
                persona: selected_persona.persona,
                device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                at_ms: 3_000,
            },
        )
        .await?;
        Ok(Self::new(host))
    }

    /// Mount the local Mere endpoint through the same Graphshell client path as
    /// any remote scene, then fetch its portable card resources.
    pub fn mount_local(&mut self) -> Result<ProjectionSession, AppError> {
        let request = self.host.local_request();
        let session = request.session.clone();
        let snapshot = self.host.snapshot(request)?;
        let resources: Vec<_> = snapshot
            .presentation
            .offers
            .values()
            .flatten()
            .map(|offer| offer.resource)
            .collect();
        self.client
            .apply_snapshot(snapshot)
            .map_err(|error| AppError::Client(format!("{error:?}")))?;
        for resource in resources {
            let response = self.host.resource(ResourceRequest {
                session: session.clone(),
                resource,
            })?;
            self.client
                .apply_resource(response)
                .map_err(|error| AppError::Client(format!("{error:?}")))?;
        }
        Ok(session)
    }

    /// Mount an independently supplied endpoint snapshot.
    pub fn mount_remote(
        &mut self,
        snapshot: ProjectionSnapshot,
    ) -> Result<ProjectionSession, AppError> {
        let session = snapshot.session.clone();
        self.client
            .apply_snapshot(snapshot)
            .map_err(|error| AppError::Client(format!("{error:?}")))?;
        Ok(session)
    }

    /// Mount the deterministic G1 endpoint as H1's remote peer.
    pub fn mount_fixture_remote(&mut self) -> Result<ProjectionSession, AppError> {
        let mut remote = crate::canary::FixtureEndpoint::new();
        let request = remote.request();
        let snapshot = remote
            .snapshot(request)
            .map_err(|error| AppError::Client(error.to_string()))?;
        self.mount_remote(snapshot)
    }

    /// Copy one user-selected exportable portable card into the local Mere
    /// graph as an explicitly non-authoritative projection.
    pub fn pin_portable_card(
        &mut self,
        session: &ProjectionSession,
        instance: InstanceId,
    ) -> Result<uuid::Uuid, AppError> {
        let (source, epoch, revision) = {
            let mounted = self
                .client
                .mounted(session)
                .ok_or_else(|| AppError::Client("projection is not mounted".to_string()))?;
            if mounted.cache_policy.retention != CacheRetention::Exportable {
                return Err(AppError::Client(
                    "projection does not permit export into the local graph".to_string(),
                ));
            }
            let item = mounted
                .scene
                .active_item(instance)
                .ok_or_else(|| AppError::Client("projection item is not active".to_string()))?;
            let source = mounted
                .scene
                .tables
                .sources
                .get(item.source.0 as usize)
                .and_then(Option::as_ref)
                .cloned()
                .ok_or_else(|| AppError::Client("projection source is unavailable".to_string()))?;
            (source, mounted.scene.epoch.0, mounted.scene.revision.0)
        };
        let resolved = self
            .client
            .resolve(
                session,
                instance,
                &CapabilityProfile::new([PresentationCapability::PortableCard]),
            )
            .map_err(|error| AppError::Client(format!("{error:?}")))?;
        let PresentationResolution::Ready(resolved) = resolved else {
            return Err(AppError::Client(
                "portable card bytes have not arrived".to_string(),
            ));
        };
        let ResolvedContent::PortableCard(card) = resolved.content else {
            return Err(AppError::Client(
                "projection item has no portable card".to_string(),
            ));
        };

        let source_bytes =
            serde_json::to_vec(&source).expect("projection source references always serialize");
        let address = format!("graphshell://projection/{}", ContentHash::of(&source_bytes));
        let id = self
            .host
            .create_address(&address, &card.title)
            .map_err(|error| AppError::Client(error.to_string()))?;
        let mut tags = card.badges.clone();
        tags.push("projection".to_string());
        self.host
            .edit_node(id, &card.title, tags)
            .map_err(|error| AppError::Client(error.to_string()))?;
        let pinned = PinnedProjectionCardV1 {
            source,
            observed_session: session.0.clone(),
            observed_epoch: epoch,
            observed_revision: revision,
            authority: PinnedProjectionAuthorityV1::SourceOwned,
            card,
        };
        self.host
            .set_product_facet(
                id,
                PINNED_PROJECTION_FACET,
                &serde_json::to_string(&pinned).expect("pinned projection cards always serialize"),
            )
            .map_err(|error| AppError::Client(error.to_string()))?;
        Ok(id)
    }

    /// Invoke a typed local open intent and refresh the mounted local scene when
    /// its access record changes.
    pub fn open_address(&mut self, address: &str, handler: &str) -> Result<IntentResult, AppError> {
        let session = self.host.session();
        let mounted = self
            .client
            .mounted(&session)
            .ok_or(AppError::LocalProjectionMissing)?;
        let target = self
            .host
            .instance_for_address(address)
            .ok_or(AppError::LocalProjectionMissing)?;
        let payload = serde_json::to_vec(&OpenAddressV1 {
            address: address.to_string(),
            handler: handler.to_string(),
        })
        .expect("OpenAddressV1 always serializes");
        let result = self.host.invoke(IntentInvocation {
            session: session.clone(),
            target,
            observed_epoch: mounted.scene.epoch,
            observed_revision: mounted.scene.revision,
            intent: intent_id(handler),
            payload,
        })?;
        if result == IntentResult::Accepted {
            self.mount_local()?;
        }
        Ok(result)
    }

    pub fn use_fixture_phone(&mut self, at_ms: u64) {
        self.host.set_access_context(AccessContext {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
            at_ms,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chirograph::IntentResult;
    use mere::kernel::address::AddressKind;
    use mere::kernel::graph::{EdgeFamily, RelationKind};
    use muniment::{Backend, MemoryBackend};
    use serde_json::json;

    use super::*;
    use crate::mere_host::{
        FIXTURE_FILE_ADDRESS, FIXTURE_GRANT_ADDRESS, FIXTURE_KEY_ADDRESS, FIXTURE_NON_WEB_ADDRESS,
        FIXTURE_PERSONA_ADDRESS, FIXTURE_RECEIPT_ADDRESS, FIXTURE_REMOTE_ADDRESS,
        FIXTURE_SCENE_ADDRESS, FIXTURE_WEB_ADDRESS, HOST_SLOT, UNKNOWN_FIXTURE_FACET,
        fixture_handlers,
    };
    use crate::product::{
        PINNED_PROJECTION_FACET, PinnedProjectionAuthorityV1, PinnedProjectionCardV1, SavedSceneV1,
    };

    const SAVED_AT_SECS: u64 = 1_700_000_000;

    fn selected() -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            profile: "profile:graphshell-h1".to_string(),
        }
    }

    fn access_context() -> AccessContext {
        AccessContext {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
            at_ms: 3_000,
        }
    }

    #[test]
    fn browser_host_seeds_once_then_reopens_the_stored_graph() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut seeded = GraphshellApp::open_or_fixture(backend.clone(), selected())
                .await
                .expect("seed browser host");
            assert!(!seeded.host.was_reopened());
            assert_eq!(seeded.host.graph().node_count(), 11);
            seeded.host.persist(SAVED_AT_SECS).await.expect("persist");

            let reopened = GraphshellApp::open_or_fixture(backend, selected())
                .await
                .expect("reopen browser host");
            assert!(reopened.host.was_reopened());
            assert_eq!(reopened.host.graph().node_count(), 11);
        });
    }

    #[test]
    fn h1_fixture_projects_mutates_persists_and_reopens_byte_equivalently() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut app = GraphshellApp::fixture(backend.clone(), selected()).expect("fixture");
            let local = app.mount_local().expect("mount local");
            let remote = app.mount_fixture_remote().expect("mount remote");

            assert!(app.client.mounted(&local).is_some());
            assert!(app.client.mounted(&remote).is_some());
            assert_eq!(app.host.selected_persona(), &selected());
            assert_eq!(app.host.graph().node_count(), 11);

            let web_kind = app
                .host
                .graph()
                .get_node_by_url(FIXTURE_WEB_ADDRESS)
                .unwrap()
                .1
                .primary_address()
                .address_kind();
            let custom_kind = app
                .host
                .graph()
                .get_node_by_url(FIXTURE_NON_WEB_ADDRESS)
                .unwrap()
                .1
                .primary_address()
                .address_kind();
            assert_eq!(web_kind, AddressKind::Http);
            assert_eq!(custom_kind, AddressKind::Unknown);
            assert!(
                app.host
                    .graph()
                    .get_node_by_url(FIXTURE_FILE_ADDRESS)
                    .is_some()
            );
            assert!(
                app.host
                    .graph()
                    .get_node_by_url(FIXTURE_SCENE_ADDRESS)
                    .is_some()
            );
            assert!(
                app.host
                    .graph()
                    .get_node_by_url(FIXTURE_REMOTE_ADDRESS)
                    .is_some()
            );

            let families: BTreeSet<_> = app
                .host
                .graph()
                .relations()
                .map(|relation| match relation.kind {
                    RelationKind::Semantic(_) => EdgeFamily::Semantic,
                    RelationKind::Traversal => EdgeFamily::Traversal,
                    RelationKind::Containment(_) => EdgeFamily::Containment,
                    RelationKind::Arrangement(_) => EdgeFamily::Arrangement,
                    RelationKind::Imported(_) => EdgeFamily::Imported,
                    RelationKind::Provenance(_) => EdgeFamily::Provenance,
                })
                .collect();
            assert!(families.contains(&EdgeFamily::Semantic));
            assert!(families.contains(&EdgeFamily::Containment));
            assert!(families.contains(&EdgeFamily::Arrangement));
            assert!(families.contains(&EdgeFamily::Provenance));

            assert_eq!(
                app.host
                    .access_history_for(FIXTURE_WEB_ADDRESS)
                    .unwrap()
                    .records
                    .len(),
                2,
                "the fixture begins with accesses from two devices"
            );
            for (address, facet) in [
                (FIXTURE_PERSONA_ADDRESS, "personae.public-persona/v1"),
                (FIXTURE_KEY_ADDRESS, "personae.public-key-reference/v1"),
                (FIXTURE_GRANT_ADDRESS, "personae.public-grant/v1"),
                (FIXTURE_RECEIPT_ADDRESS, "personae.signing-receipt/v1"),
            ] {
                assert!(
                    app.host.facet_value(address, facet).is_some(),
                    "missing identity projection {facet}"
                );
            }
            assert_eq!(
                app.host
                    .facet_value(FIXTURE_FILE_ADDRESS, UNKNOWN_FIXTURE_FACET),
                Some(&json!({
                    "carrier": "future",
                    "facets": ["opaque", "preserve-me"],
                    "version": 7
                }))
            );

            let before_revision = app.host.projection_revision();
            app.use_fixture_phone(3_000);
            assert_eq!(
                app.open_address(FIXTURE_WEB_ADDRESS, "system.default")
                    .expect("typed open"),
                IntentResult::Accepted
            );
            assert!(app.host.projection_revision() > before_revision);
            let accesses = app.host.access_history_for(FIXTURE_WEB_ADDRESS).unwrap();
            assert_eq!(accesses.records.len(), 3);
            assert_eq!(accesses.records.last().unwrap().handler, "system.default");

            app.host.persist(SAVED_AT_SECS).await.expect("persist");
            let first_bytes = backend
                .get(HOST_SLOT)
                .await
                .expect("backend read")
                .expect("host document");

            let host = MereHost::open(
                backend.clone(),
                selected(),
                fixture_handlers(),
                access_context(),
            )
            .await
            .expect("reopen");
            let mut reopened = GraphshellApp::new(host);
            let reopened_local = reopened.mount_local().expect("remount local");
            let reopened_remote = reopened.mount_fixture_remote().expect("remount remote");
            assert!(reopened.client.mounted(&reopened_local).is_some());
            assert!(reopened.client.mounted(&reopened_remote).is_some());
            assert_eq!(
                reopened
                    .host
                    .access_history_for(FIXTURE_WEB_ADDRESS)
                    .unwrap(),
                accesses
            );
            assert_eq!(
                reopened
                    .host
                    .facet_value(FIXTURE_FILE_ADDRESS, UNKNOWN_FIXTURE_FACET),
                app.host
                    .facet_value(FIXTURE_FILE_ADDRESS, UNKNOWN_FIXTURE_FACET)
            );

            reopened
                .host
                .persist(SAVED_AT_SECS)
                .await
                .expect("re-persist");
            let reopened_bytes = backend
                .get(HOST_SLOT)
                .await
                .expect("backend read")
                .expect("host document");
            assert_eq!(
                reopened_bytes, first_bytes,
                "graph and facet truth re-encode byte-equivalently after reopen"
            );
        });
    }

    #[cfg(feature = "native")]
    #[tokio::test]
    async fn h4_exportable_identity_cards_and_access_survive_scene_reopen_as_projections() {
        use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionSource};
        use mere::canvas::CartographyGeometry;
        use personae::ssh_slot::{protocol_key_for, slot_for};
        use personae::{
            Ed25519Keypair, IdentityVault, InMemoryStorage, Profile, ProfileId, UnlockTier,
        };
        use pandect::{
            DeviceExposure, DeviceId, DevicePublicKey, PersonaId, RemoteAuthGrantSpec,
            ensure_wallet_state, issue_remote_auth_device_grant, load_device_roster,
        };
        use ssh_agent_lib::agent::Session;
        use ssh_agent_lib::proto::SignRequest;
        use ssh_key::Algorithm;

        use crate::identity::VaultProtectionView;
        use crate::identity_endpoint::IdentityEndpoint;
        use crate::identity_projection::{DEVICE_REVOKE_INTENT, RevokeDeviceIntentV1};
        use crate::native::personae_host::PersonaeHost;

        let carry_root = std::env::temp_dir().join(format!(
            "graphshell-h4-carry-scene-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let persona = PersonaId::default_persona();
        ensure_wallet_state(&carry_root, persona, "Graphshell workstation")
            .expect("bootstrap carry authority");
        let device_id = DeviceId::new();
        let delegatee = Ed25519Keypair::from_seed([0x45; 32]);
        issue_remote_auth_device_grant(
            &carry_root,
            &RemoteAuthGrantSpec {
                device_id,
                delegatee_pubkey: DevicePublicKey::from(delegatee.public_key()),
                label: "Pocket relay".to_string(),
                exposure: DeviceExposure::HiddenClient,
                issued_at_ms: 1_700_000_000_000,
                expires_at_ms: Some(1_800_000_000_000),
                personas: vec![persona],
                scopes: vec!["identity.act".to_string()],
                attenuations: vec!["no-subdelegation".to_string()],
                wrapped_private_epochs: Vec::new(),
            },
        )
        .expect("issue live device grant");

        let mut private =
            ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        private.set_comment("mixed-scene-key");
        let public = ssh_key::PublicKey::from(&private);
        let mut profile = Profile::new(
            ProfileId("mixed-scene".to_string()),
            "Mixed scene",
            Ed25519Keypair::from_seed([0x46; 32]),
        );
        profile.slots.insert(
            protocol_key_for(&private),
            slot_for(&private, UnlockTier::Session).unwrap(),
        );
        let authority = std::sync::Arc::new(PersonaeHost::new(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            Some(carry_root.clone()),
            VaultProtectionView::Ephemeral,
        ));
        let mut agent = authority.agent_session();
        let signature = agent
            .sign(SignRequest {
                credential: public.key_data().clone().into(),
                data: b"graphshell mixed scene".to_vec(),
                flags: 0,
            })
            .await
            .expect("produce real signing history");
        assert!(!signature.as_bytes().is_empty());

        let mut endpoint = IdentityEndpoint::new(authority);
        let before = endpoint
            .snapshot(endpoint.request())
            .expect("identity projection before revocation");
        let device_source = format!("identity:device:{}", device_id.as_uuid());
        let device_instance = before
            .scene
            .active_items_in_order()
            .into_iter()
            .find_map(|(instance, item)| {
                before.scene.tables.sources[item.source.0 as usize]
                    .as_ref()
                    .filter(|source| source.id == device_source)
                    .map(|_| instance)
            })
            .expect("projected device");
        let unconfirmed = endpoint
            .invoke(IntentInvocation {
                session: before.session.clone(),
                target: device_instance,
                observed_epoch: before.scene.epoch,
                observed_revision: before.scene.revision,
                intent: DEVICE_REVOKE_INTENT.to_string(),
                payload: serde_json::to_vec(&RevokeDeviceIntentV1 {
                    device_id: *device_id.as_uuid(),
                    confirmed: false,
                })
                .unwrap(),
            })
            .expect("bounded rejection");
        assert!(matches!(unconfirmed, IntentResult::Rejected { .. }));
        let revoked = endpoint
            .invoke(IntentInvocation {
                session: before.session,
                target: device_instance,
                observed_epoch: before.scene.epoch,
                observed_revision: before.scene.revision,
                intent: DEVICE_REVOKE_INTENT.to_string(),
                payload: serde_json::to_vec(&RevokeDeviceIntentV1 {
                    device_id: *device_id.as_uuid(),
                    confirmed: true,
                })
                .unwrap(),
            })
            .expect("typed revocation");
        assert_eq!(revoked, IntentResult::Accepted);
        assert!(
            load_device_roster(&carry_root)
                .unwrap()
                .unwrap()
                .revoked
                .contains(&device_id),
            "the pandect roster remains mutation authority"
        );

        let snapshot = endpoint
            .snapshot(endpoint.request())
            .expect("identity projection after revocation");
        assert_eq!(snapshot.cache_policy.retention, CacheRetention::Exportable);
        let resources = snapshot
            .presentation
            .offers
            .values()
            .flatten()
            .map(|offer| offer.resource)
            .collect::<Vec<_>>();
        let identity_session = snapshot.session.clone();
        let wanted_sources = [
            "identity:profile:mixed-scene".to_string(),
            device_source,
            format!("identity:grant:{}", device_id.as_uuid()),
        ];
        let signing_source = snapshot
            .scene
            .active_items_in_order()
            .into_iter()
            .filter_map(|(_, item)| {
                snapshot.scene.tables.sources[item.source.0 as usize]
                    .as_ref()
                    .map(|source| source.id.clone())
            })
            .find(|source| source.starts_with("identity:history:"))
            .expect("projected signing history");

        let backend = MemoryBackend::new();
        let mut app = GraphshellApp::fixture(backend.clone(), selected()).expect("fixture");
        app.mount_remote(snapshot).expect("mount identity");
        for resource in resources {
            app.client
                .apply_resource(
                    endpoint
                        .resource(ResourceRequest {
                            session: identity_session.clone(),
                            resource,
                        })
                        .expect("identity card resource"),
                )
                .expect("cache identity card");
        }

        let pinned_instances = {
            let mounted = app.client.mounted(&identity_session).unwrap();
            wanted_sources
                .iter()
                .chain(std::iter::once(&signing_source))
                .map(|source_id| {
                    mounted
                        .scene
                        .active_items_in_order()
                        .into_iter()
                        .find_map(|(instance, item)| {
                            mounted.scene.tables.sources[item.source.0 as usize]
                                .as_ref()
                                .filter(|source| source.id == source_id.as_str())
                                .map(|_| instance)
                        })
                        .unwrap()
                })
                .collect::<Vec<_>>()
        };
        let mut selected_ids = Vec::new();
        for instance in pinned_instances {
            selected_ids.push(
                app.pin_portable_card(&identity_session, instance)
                    .expect("pin public identity card"),
            );
        }
        let access = app
            .host
            .graph()
            .get_node_by_url(FIXTURE_WEB_ADDRESS)
            .unwrap()
            .1
            .id;
        selected_ids.push(access);
        let scene = SavedSceneV1 {
            name: "Identity and access".to_string(),
            selected: selected_ids.clone(),
            layout_strategy: Some("grid.default".to_string()),
            physics_paused: true,
            physics_damping: 0.7,
            arrangement_pull: 0.4,
            camera_offset: (0.0, 0.0),
            camera_zoom: 1.0,
            default_handler: "system.default".to_string(),
            cartography: CartographyGeometry::from_positions(
                selected_ids
                    .iter()
                    .enumerate()
                    .map(|(index, id)| (*id, (index as f32 * 30.0, 0.0))),
            ),
        };
        app.host
            .save_product_scene("mere://scene/h4-identity-access", &scene)
            .expect("save mixed scene");
        app.host
            .persist(SAVED_AT_SECS)
            .await
            .expect("persist scene");

        let reopened = MereHost::open(backend, selected(), fixture_handlers(), access_context())
            .await
            .expect("reopen local graph");
        assert_eq!(
            reopened
                .product_scene("mere://scene/h4-identity-access")
                .expect("reopen mixed scene"),
            scene
        );
        assert_eq!(
            reopened
                .access_history_for(FIXTURE_WEB_ADDRESS)
                .unwrap()
                .records
                .len(),
            2
        );
        for id in selected_ids.iter().take(4) {
            let (_, node) = reopened.graph().get_node_by_id(*id).expect("pinned node");
            let value = reopened
                .graph()
                .facets()
                .get(&node.id, &chartulary::FacetId::new(PINNED_PROJECTION_FACET))
                .expect("pinned projection facet");
            let pinned: PinnedProjectionCardV1 =
                serde_json::from_value(value.clone()).expect("typed pinned projection");
            assert_eq!(pinned.authority, PinnedProjectionAuthorityV1::SourceOwned);
            assert_eq!(pinned.source.adapter, "personae.public");
            let json = value.to_string();
            assert!(!json.contains("\"actions\""));
            assert!(!json.contains("\"intent\""));
            assert!(!json.contains("BEGIN OPENSSH PRIVATE KEY"));
        }

        std::fs::remove_dir_all(carry_root).expect("remove isolated carry fixture");
    }
}
