//! Graphshell's reference-host composition over local and remote projections.

use graphshell_client::ClientState;
use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionSource};
use graphshell_protocol::{
    IntentInvocation, IntentResult, ProjectionSession, ProjectionSnapshot, ResourceRequest,
};
use muniment::Backend;

use crate::access::AccessContext;
use crate::handlers::{OpenAddressV1, intent_id};
use crate::mere_host::{
    FIXTURE_DEVICE_TWO_ADDRESS, FIXTURE_PERSONA_ADDRESS, MereHost, MereHostError,
    SelectedPersonaRef, fixture_handlers,
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

    use graphshell_protocol::IntentResult;
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
}
