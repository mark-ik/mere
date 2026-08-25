// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Burn Remote as a lease-bound Distillery resource.

use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use burn_backend::tensor::Device;
use burn_ir::BackendIr;
use burn_remote::BURN_REMOTE_ALPN;
use burn_remote::server::{AuthorizationRequest, IrohRemoteProtocol, PeerAuthorizer};
use burn_remote::telemetry::TelemetryProbe;
use mesh::{
    ComputeClass, ImplementationId, JobBoard, JobControl, JobId, JobNamespaceView, LeaseId,
    LeasePolicy, MeshResource, Prepared, RemoteAdmission, RemoteSessionClaim, ResourceDescriptor,
    ResourceError, ResourceId, ResourceRequirements, RunContext, VerificationClass,
};
use mesh_host::Clock;
use transport::P2pandaTransport;

use crate::authority::RemoteSessionProjection;

/// Mesh resource implemented by the Burn Remote session lane.
pub const BURN_REMOTE_RESOURCE: &str = "esp.remote.burn/v1";

const BURN_REMOTE_IMPLEMENTATION: &str = "distillery.burn-remote-pre2/v1";
const RECEIPT_CONTEXT: &[u8] = b"mere/distillery/remote-session-receipt/v1\0";

/// Owner-selected behavior of a remote compute offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteSessionSettings {
    /// Host capability the offered Burn devices require.
    pub requires: ResourceRequirements,
    /// Named job input whose presence activates the session offer.
    pub request_input: String,
    /// How often the resource observes session and cancellation state.
    pub poll_every: Duration,
}

impl RemoteSessionSettings {
    /// A GPU-backed remote lane with an explicit memory floor.
    pub fn gpu(memory_mib: u32, poll_every: Duration) -> Self {
        Self {
            requires: ResourceRequirements {
                memory_mib,
                compute: ComputeClass::Gpu,
            },
            request_input: "request".to_string(),
            poll_every,
        }
    }

    fn validate(&self) -> Result<(), RemoteSessionError> {
        if self.poll_every.is_zero() {
            return Err(RemoteSessionError::InvalidSettings(
                "remote session poll cadence must be greater than zero",
            ));
        }
        if self.request_input.is_empty() {
            return Err(RemoteSessionError::InvalidSettings(
                "remote session request input must be named",
            ));
        }
        Ok(())
    }
}

/// Failure while composing the Burn protocol with a resident transport.
#[derive(Debug, thiserror::Error)]
pub enum RemoteSessionError {
    /// No compute device was offered.
    #[error("remote session service requires at least one Burn device")]
    NoDevices,
    /// Too many devices were offered for Burn's `u32` device index.
    #[error("remote session service has more devices than the protocol can index")]
    TooManyDevices,
    /// An owner setting cannot safely drive the service.
    #[error("remote session settings: {0}")]
    InvalidSettings(&'static str),
    /// The shared endpoint could not be inspected or extended.
    #[error("remote session endpoint: {0}")]
    Endpoint(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RunKey {
    job: JobId,
    lease: LeaseId,
}

#[derive(Default)]
struct AdmissionState {
    board: Option<JobBoard>,
    active: BTreeSet<RunKey>,
}

struct RemoteAuthorizer {
    state: Arc<RwLock<AdmissionState>>,
    mesh_id: [u8; 32],
    server_author: [u8; 32],
    server_peer: [u8; 32],
    offered_devices: BTreeSet<u32>,
    expected_resource: ResourceId,
    clock: Arc<dyn Clock>,
    lease_policy: LeasePolicy,
}

impl PeerAuthorizer for RemoteAuthorizer {
    fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), String> {
        let claim =
            RemoteSessionClaim::decode(request.credential).map_err(|error| error.to_string())?;
        let state = self
            .state
            .read()
            .map_err(|_| "remote-session admission state is poisoned".to_string())?;
        let board = state
            .board
            .as_ref()
            .ok_or_else(|| "remote-session board has not been observed".to_string())?;
        let key = RunKey {
            job: claim.job(),
            lease: claim.lease(),
        };
        if !state.active.contains(&key) {
            return Err("remote-session lease is not active in this host".to_string());
        }
        claim
            .authorize(&RemoteAdmission {
                mesh_id: self.mesh_id,
                board,
                server_author: self.server_author,
                server_peer: self.server_peer,
                connected_peer: *request.peer.as_bytes(),
                requested_device: request.device_index,
                offered_devices: &self.offered_devices,
                expected_resource: &self.expected_resource,
                now_ms: self.clock.now_ms(),
                lease_policy: &self.lease_policy,
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

/// Mounted Burn protocol and its lease admission projection.
pub struct RemoteSessionService<B: BackendIr> {
    protocol: IrohRemoteProtocol<B>,
    state: Arc<RwLock<AdmissionState>>,
    server_peer: [u8; 32],
    descriptor: ResourceDescriptor,
    settings: RemoteSessionSettings,
}

impl<B: BackendIr> RemoteSessionService<B> {
    /// Mount Burn on the transport's existing endpoint and return the service.
    #[allow(clippy::too_many_arguments)]
    pub async fn mount(
        transport: &P2pandaTransport,
        devices: Vec<Device<B>>,
        mesh_id: [u8; 32],
        server_author: [u8; 32],
        clock: Arc<dyn Clock>,
        lease_policy: LeasePolicy,
        settings: RemoteSessionSettings,
    ) -> Result<Arc<Self>, RemoteSessionError> {
        settings.validate()?;
        if devices.is_empty() {
            return Err(RemoteSessionError::NoDevices);
        }
        let device_count =
            u32::try_from(devices.len()).map_err(|_| RemoteSessionError::TooManyDevices)?;
        let endpoint = transport.protocol_endpoint();
        let raw = endpoint
            .endpoint()
            .await
            .map_err(|error| RemoteSessionError::Endpoint(error.to_string()))?;
        let server_peer = *raw.id().as_bytes();
        let state = Arc::new(RwLock::new(AdmissionState::default()));
        let resource = ResourceId::parse(BURN_REMOTE_RESOURCE).expect("static resource id");
        let authorizer = Arc::new(RemoteAuthorizer {
            state: state.clone(),
            mesh_id,
            server_author,
            server_peer,
            offered_devices: (0..device_count).collect(),
            expected_resource: resource.clone(),
            clock,
            lease_policy,
        });
        let protocol = IrohRemoteProtocol::new(
            raw,
            devices,
            authorizer,
            TelemetryProbe::disabled(),
            Default::default(),
        );
        endpoint
            .accept_raw(BURN_REMOTE_ALPN, protocol.clone())
            .await
            .map_err(|error| RemoteSessionError::Endpoint(error.to_string()))?;

        Ok(Arc::new(Self {
            protocol,
            state,
            server_peer,
            descriptor: ResourceDescriptor {
                resource,
                implementation: ImplementationId::parse(BURN_REMOTE_IMPLEMENTATION)
                    .expect("static implementation id"),
                requires: settings.requires,
                verification: VerificationClass::ProducerOnly,
            },
            settings,
        }))
    }

    /// Resource adapter to register before constructing the mesh host.
    pub fn resource(self: &Arc<Self>) -> Arc<dyn MeshResource> {
        Arc::new(RemoteBurnResource {
            service: self.clone(),
        })
    }

    /// The server transport identity clients bind into signed claims.
    pub fn server_peer(&self) -> [u8; 32] {
        self.server_peer
    }

    /// Whether the mesh host currently supervises this exact remote lease.
    pub fn is_active(&self, job: JobId, lease: LeaseId) -> bool {
        self.state
            .read()
            .is_ok_and(|state| state.active.contains(&RunKey { job, lease }))
    }

    /// Number of live Burn sessions bound to this exact remote lease.
    pub async fn session_count(&self, job: JobId, lease: LeaseId) -> usize {
        self.matching_sessions(RunKey { job, lease }).await
    }

    async fn close_run(&self, key: RunKey) -> Result<(), String> {
        if let Ok(mut state) = self.state.write() {
            state.active.remove(&key);
        }
        loop {
            let sessions: Vec<_> = self
                .protocol
                .sessions()
                .await
                .into_iter()
                .filter(|session| {
                    RemoteSessionClaim::decode(&session.credential)
                        .is_ok_and(|claim| claim.job() == key.job && claim.lease() == key.lease)
                })
                .collect();
            if sessions.is_empty() {
                break;
            }
            for session in sessions {
                self.protocol.close_session(session.id).await?;
            }
        }
        Ok(())
    }

    async fn matching_sessions(&self, key: RunKey) -> usize {
        self.protocol
            .sessions()
            .await
            .into_iter()
            .filter(|session| {
                RemoteSessionClaim::decode(&session.credential)
                    .is_ok_and(|claim| claim.job() == key.job && claim.lease() == key.lease)
            })
            .count()
    }
}

impl<B: BackendIr> RemoteSessionProjection for RemoteSessionService<B> {
    fn refresh(&self, board: JobBoard) {
        if let Ok(mut state) = self.state.write() {
            state.board = Some(board);
        }
    }
}

/// Mesh resource whose lifetime is one lease-bound Burn session.
pub struct RemoteBurnResource<B: BackendIr> {
    service: Arc<RemoteSessionService<B>>,
}

impl<B: BackendIr> MeshResource for RemoteBurnResource<B> {
    fn descriptor(&self) -> &ResourceDescriptor {
        &self.service.descriptor
    }

    fn prepare<'a>(
        &'a self,
        namespace: &'a JobNamespaceView<'a>,
    ) -> mesh::namespace::BoxFuture<'a, Result<Prepared, ResourceError>> {
        Box::pin(async move {
            namespace.read(&self.service.settings.request_input).await?;
            Ok(Prepared::new(()))
        })
    }

    fn execute<'a>(
        &'a self,
        _prepared: Prepared,
        _control: &'a JobControl,
    ) -> mesh::namespace::BoxFuture<'a, Result<Vec<u8>, ResourceError>> {
        Box::pin(async {
            Err(ResourceError::Backend(
                "Burn Remote requires a supervised run context".to_string(),
            ))
        })
    }

    fn execute_for<'a>(
        &'a self,
        prepared: Prepared,
        control: &'a JobControl,
        context: RunContext,
    ) -> mesh::namespace::BoxFuture<'a, Result<Vec<u8>, ResourceError>> {
        Box::pin(async move {
            prepared.take::<()>()?;
            let lease = context.lease.ok_or_else(|| {
                ResourceError::Backend("Burn Remote requires a live lease".to_string())
            })?;
            let key = RunKey {
                job: context.job,
                lease,
            };
            {
                let mut state = self.service.state.write().map_err(|_| {
                    ResourceError::Backend("remote-session state is poisoned".into())
                })?;
                if !state.active.insert(key) {
                    return Err(ResourceError::Backend(
                        "remote-session run is already active".to_string(),
                    ));
                }
            }

            let mut observed = false;
            loop {
                if control.is_cancelled() {
                    self.service
                        .close_run(key)
                        .await
                        .map_err(ResourceError::Backend)?;
                    return Err(mesh::Cancelled.into());
                }
                let sessions = self.service.matching_sessions(key).await;
                if sessions != 0 {
                    observed = true;
                    control.report(1, 1);
                } else if observed {
                    if let Ok(mut state) = self.service.state.write() {
                        state.active.remove(&key);
                    }
                    let mut receipt = Vec::with_capacity(RECEIPT_CONTEXT.len() + 64);
                    receipt.extend_from_slice(RECEIPT_CONTEXT);
                    receipt.extend_from_slice(&key.job.0);
                    receipt.extend_from_slice(&key.lease.0);
                    return Ok(receipt);
                }
                tokio::time::sleep(self.service.settings.poll_every).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

    use burn_backend::{TensorData, ops::FloatTensorOps};
    use burn_flex::Flex;
    use burn_remote::{RemoteBackend, RemoteDevice};
    use identity::{IdentityProvider, InMemoryProvider};
    use mesh::{
        DeterminismClass, DeviceConditions, HostFacts, JobSpec, LeaseTerms, MESH_AUTHOR_SALT,
        MemoryBlobSpace, MeshEvent, MeshStore, ResourceRegistry, RunError, SyncedMesh, run_job_for,
        to_operation,
    };
    use mesh_host::{HostConfig, ManualClock, MeshHost, ObservedConditions, Step};

    use crate::{BlobCustody, Distillery, RetentionSettings};

    const MESH: [u8; 32] = [0x72; 32];
    const NOW: u64 = 5_000;

    struct NoCustody;

    impl BlobCustody for NoCustody {
        fn collect<'a>(
            &'a self,
            _blobs: &'a [mesh::BlobRef],
        ) -> Pin<Box<dyn Future<Output = Result<u64, String>> + Send + 'a>> {
            Box::pin(async { Ok(0) })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_live_lease_runs_on_the_shared_endpoint_and_reclaim_ends_the_client() {
        let poster_provider = InMemoryProvider::from_seed([21; 32]);
        let server_provider = InMemoryProvider::from_seed([22; 32]);
        let poster_key = poster_provider.derive_keypair(MESH_AUTHOR_SALT).unwrap();
        let server_key = server_provider.derive_keypair(MESH_AUTHOR_SALT).unwrap();

        let server_transport = P2pandaTransport::builder(server_provider.master_keypair())
            .bind()
            .await
            .unwrap();
        let client_transport = P2pandaTransport::builder(poster_provider.master_keypair())
            .bind()
            .await
            .unwrap();
        let clock = Arc::new(ManualClock::at(NOW));
        let service = RemoteSessionService::<Flex>::mount(
            &server_transport,
            vec![Default::default()],
            MESH,
            server_key.public_key().to_bytes(),
            clock,
            LeasePolicy { max_skew_ms: 0 },
            RemoteSessionSettings::gpu(0, Duration::from_millis(5)),
        )
        .await
        .unwrap();

        let space = Arc::new(MemoryBlobSpace::in_memory());
        let request = space.put(b"one raw Burn session").await.unwrap();
        let spec = JobSpec::simple(
            ResourceId::parse(BURN_REMOTE_RESOURCE).unwrap(),
            "request",
            request,
            "receipt",
            256,
            DeterminismClass::Observed,
        )
        .leased(LeaseTerms::new(60_000, 10_000));
        let post = to_operation(
            &poster_key,
            MESH,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(spec.clone()),
                nonce: 0,
                at_ms: 1_000,
            },
            0,
            None,
        );
        let job = JobId(*post.hash.as_bytes());
        let holder_claim = to_operation(
            &server_key,
            MESH,
            &MeshEvent::JobClaimed {
                job: job.0,
                at_ms: 2_000,
            },
            0,
            None,
        );
        let grant = to_operation(
            &server_key,
            MESH,
            &MeshEvent::LeaseGranted {
                job: job.0,
                epoch: 0,
                granted_at_ms: 3_000,
                expires_at_ms: 63_000,
            },
            1,
            Some(*holder_claim.hash.as_bytes()),
        );
        let lease = LeaseId(*grant.hash.as_bytes());
        let poster_attestation = to_operation(
            &poster_key,
            MESH,
            &MeshEvent::DeviceAttested {
                attestation: Box::new(
                    poster_provider
                        .attest_derived_key(MESH_AUTHOR_SALT)
                        .unwrap(),
                ),
            },
            1,
            Some(*post.hash.as_bytes()),
        );
        let server_attestation = to_operation(
            &server_key,
            MESH,
            &MeshEvent::DeviceAttested {
                attestation: Box::new(
                    server_provider
                        .attest_derived_key(MESH_AUTHOR_SALT)
                        .unwrap(),
                ),
            },
            2,
            Some(*grant.hash.as_bytes()),
        );
        service.refresh(JobBoard::fold(
            MESH,
            [
                &post,
                &holder_claim,
                &grant,
                &poster_attestation,
                &server_attestation,
            ],
        ));

        let mut registry = ResourceRegistry::new();
        registry.register(service.resource()).unwrap();
        let (handle, control) = mesh::JobControl::new();
        let run_space = space.clone();
        let run = tokio::spawn(async move {
            run_job_for(
                &registry,
                RunContext {
                    job,
                    lease: Some(lease),
                },
                &spec,
                &*run_space,
                &*run_space,
                &control,
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !service.is_active(job, lease) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("resource activation");

        let credential =
            RemoteSessionClaim::signed(&poster_key, MESH, job, lease, 0, service.server_peer(), 0)
                .encode()
                .unwrap();
        let client_endpoint = client_transport
            .protocol_endpoint()
            .endpoint()
            .await
            .unwrap();
        let remote = RemoteDevice::iroh_authorized(
            &client_endpoint,
            server_transport.endpoint_addr().await.unwrap(),
            0,
            credential,
        );
        let input = <RemoteBackend as FloatTensorOps<RemoteBackend>>::float_from_data(
            TensorData::from([1.0f32, 2.0, 3.0]),
            &remote,
        );
        let data = tokio::time::timeout(
            Duration::from_secs(10),
            <RemoteBackend as FloatTensorOps<RemoteBackend>>::float_into_data(input),
        )
        .await
        .expect("remote tensor operation did not hang")
        .unwrap();
        assert_eq!(data.to_vec::<f32>().unwrap(), vec![1.0, 2.0, 3.0]);
        assert_eq!(service.session_count(job, lease).await, 1);

        handle.cancel();
        let stopped = tokio::time::timeout(Duration::from_secs(10), run)
            .await
            .expect("resource did not stop on reclaim")
            .unwrap();
        assert!(matches!(
            stopped,
            Err(RunError::Resource(ResourceError::Cancelled(_)))
        ));
        assert_eq!(service.session_count(job, lease).await, 0);

        let next = <RemoteBackend as FloatTensorOps<RemoteBackend>>::float_from_data(
            TensorData::from([4.0f32, 5.0, 6.0]),
            &remote,
        );
        let ended = tokio::time::timeout(
            Duration::from_secs(10),
            <RemoteBackend as FloatTensorOps<RemoteBackend>>::float_into_data(next),
        )
        .await
        .expect("the revoked client hung instead of observing termination");
        assert!(ended.is_err(), "the revoked client still accepted work");

        client_transport.close().await.unwrap();
        server_transport.close().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn distillery_closes_the_session_before_authoring_owner_reclaim() {
        let poster_provider = InMemoryProvider::from_seed([31; 32]);
        let server_provider = InMemoryProvider::from_seed([32; 32]);
        let poster_key = poster_provider.derive_keypair(MESH_AUTHOR_SALT).unwrap();
        let server_key = server_provider.derive_keypair(MESH_AUTHOR_SALT).unwrap();
        let server_transport = Arc::new(
            P2pandaTransport::builder(server_provider.master_keypair())
                .gossip()
                .bind()
                .await
                .unwrap(),
        );
        let client_transport = P2pandaTransport::builder(poster_provider.master_keypair())
            .bind()
            .await
            .unwrap();
        let clock = Arc::new(ManualClock::at(NOW));
        let service = RemoteSessionService::<Flex>::mount(
            &server_transport,
            vec![Default::default()],
            MESH,
            server_key.public_key().to_bytes(),
            clock.clone(),
            LeasePolicy { max_skew_ms: 0 },
            RemoteSessionSettings::gpu(0, Duration::from_millis(5)),
        )
        .await
        .unwrap();

        let (endpoint, gossip) = server_transport.sync_parts().unwrap();
        let synced = SyncedMesh::join(endpoint, gossip, MeshStore::in_memory(), MESH)
            .await
            .unwrap();
        let space = Arc::new(MemoryBlobSpace::in_memory());
        let request = space.put(b"supervised Burn session").await.unwrap();
        let conditions = Arc::new(ObservedConditions::spare());
        let mut registry = ResourceRegistry::new();
        registry.register(service.resource()).unwrap();
        let mut config = HostConfig::supervised(space.clone());
        config.registry = registry;
        config.clock = clock;
        config.conditions = conditions.clone();
        config.facts = HostFacts {
            memory_mib: 8_192,
            gpu: true,
        };
        config.policy = mesh::DevicePolicy::conservative();
        config.lease = LeasePolicy { max_skew_ms: 0 };
        let host = MeshHost::new(synced, server_key.clone(), config);
        let mut distillery =
            Distillery::new(host, Arc::new(NoCustody), RetentionSettings::default());
        distillery.attach_remote_sessions(service.clone());

        distillery
            .host()
            .synced()
            .author(
                &poster_key,
                &MeshEvent::DeviceAttested {
                    attestation: Box::new(
                        poster_provider
                            .attest_derived_key(MESH_AUTHOR_SALT)
                            .unwrap(),
                    ),
                },
            )
            .await
            .unwrap();
        distillery
            .host()
            .synced()
            .author(
                &server_key,
                &MeshEvent::DeviceAttested {
                    attestation: Box::new(
                        server_provider
                            .attest_derived_key(MESH_AUTHOR_SALT)
                            .unwrap(),
                    ),
                },
            )
            .await
            .unwrap();
        let posted = distillery
            .host()
            .synced()
            .author(
                &poster_key,
                &MeshEvent::JobPostedV2 {
                    spec: Box::new(
                        JobSpec::simple(
                            ResourceId::parse(BURN_REMOTE_RESOURCE).unwrap(),
                            "request",
                            request,
                            "receipt",
                            256,
                            DeterminismClass::Observed,
                        )
                        .leased(LeaseTerms::new(60_000, 10_000)),
                    ),
                    nonce: 1,
                    at_ms: NOW,
                },
            )
            .await
            .unwrap();
        let job = JobId(*posted.hash.as_bytes());

        let mut observed = Vec::new();
        let mut granted = None;
        for _ in 0..100 {
            let steps = distillery.tick().await.unwrap();
            observed.extend(steps.iter().cloned());
            granted = granted.or_else(|| {
                steps.iter().find_map(|step| match step {
                    Step::Granted { job: id, lease, .. } if *id == job => Some(*lease),
                    _ => None,
                })
            });
            if granted.is_some_and(|lease| service.is_active(job, lease)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let lease = granted.unwrap_or_else(|| panic!("remote host never granted: {observed:#?}"));
        assert!(
            service.is_active(job, lease),
            "remote host never started: {observed:#?}"
        );

        let credential =
            RemoteSessionClaim::signed(&poster_key, MESH, job, lease, 0, service.server_peer(), 0)
                .encode()
                .unwrap();
        let client_endpoint = client_transport
            .protocol_endpoint()
            .endpoint()
            .await
            .unwrap();
        let remote = RemoteDevice::iroh_authorized(
            &client_endpoint,
            server_transport.endpoint_addr().await.unwrap(),
            0,
            credential,
        );
        let input = <RemoteBackend as FloatTensorOps<RemoteBackend>>::float_from_data(
            TensorData::from([8.0f32, 13.0, 21.0]),
            &remote,
        );
        let data = tokio::time::timeout(
            Duration::from_secs(10),
            <RemoteBackend as FloatTensorOps<RemoteBackend>>::float_into_data(input),
        )
        .await
        .expect("live leased session responds")
        .unwrap();
        assert_eq!(data.to_vec::<f32>().unwrap(), vec![8.0, 13.0, 21.0]);
        assert_eq!(service.session_count(job, lease).await, 1);

        conditions.set(DeviceConditions::spare().in_use());
        let first = distillery.tick().await.unwrap();
        assert!(
            first
                .iter()
                .any(|step| matches!(step, Step::AwaitingStop { job: id } if *id == job))
        );
        assert!(
            !first
                .iter()
                .any(|step| matches!(step, Step::Reclaimed { job: id, .. } if *id == job)),
            "the reclaim fact cannot share the cancellation turn"
        );

        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let steps = distillery.tick().await.unwrap();
                if steps.iter().any(|step| {
                    matches!(step, Step::Reclaimed { job: id, lease: held, .. } if *id == job && *held == lease)
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("reclaim completes after the resource stops");
        assert_eq!(service.session_count(job, lease).await, 0);
        assert!(!service.is_active(job, lease));
        assert!(matches!(
            distillery
                .host()
                .synced()
                .board()
                .await
                .unwrap()
                .job(job)
                .unwrap()
                .lease_at(NOW, &LeasePolicy { max_skew_ms: 0 }),
            mesh::LeasePhase::Reclaimed { lease: held, .. } if held == lease
        ));

        let next = <RemoteBackend as FloatTensorOps<RemoteBackend>>::float_from_data(
            TensorData::from([34.0f32]),
            &remote,
        );
        let ended = tokio::time::timeout(
            Duration::from_secs(10),
            <RemoteBackend as FloatTensorOps<RemoteBackend>>::float_into_data(next),
        )
        .await
        .expect("the revoked client hung");
        assert!(ended.is_err(), "the reclaimed client still accepted work");

        distillery.shutdown().await.unwrap();
        client_transport.close().await.unwrap();
        server_transport.close().await.unwrap();
    }
}
