// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Lease-bound authority for a remote compute session.
//!
//! Burn Remote treats its authorization bytes as opaque and asks its
//! application authorizer once, when a session opens. This module owns the
//! transport-neutral meaning of those bytes. It deliberately knows neither
//! Burn nor Iroh types: the adapter converts authenticated endpoint ids to the
//! same 32-byte public-key form the mesh directory records, then asks
//! [`RemoteSessionClaim::authorize`].

use std::collections::BTreeSet;

use identity::{Ed25519Keypair, Ed25519PublicKey, Ed25519Signature};
use serde::{Deserialize, Serialize};

use crate::{JobBoard, JobId, LeaseId, LeasePhase, LeasePolicy, ResourceId};

/// Current credential wire version.
pub const REMOTE_SESSION_CLAIM_VERSION: u8 = 1;

/// Largest opaque credential this decoder accepts.
///
/// The v1 credential is under 300 bytes. The wider ceiling leaves room for a
/// later field without letting an admission request allocate arbitrary input.
pub const MAX_REMOTE_SESSION_CREDENTIAL_BYTES: usize = 1_024;

const SIGNING_CONTEXT: &[u8] = b"mere/mesh/remote-session-claim/v1\0";

/// Signed authority presented by a job poster to the device holding its lease.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSessionClaim {
    version: u8,
    mesh_id: [u8; 32],
    job: JobId,
    lease: LeaseId,
    epoch: u32,
    client: [u8; 32],
    server_peer: [u8; 32],
    device_index: u32,
    signature: Vec<u8>,
}

/// The live facts an admitting remote server must bind to the signed claim.
pub struct RemoteAdmission<'a> {
    /// Mesh whose board is being projected.
    pub mesh_id: [u8; 32],
    /// Current folded board.
    pub board: &'a JobBoard,
    /// This server's mesh author key.
    pub server_author: [u8; 32],
    /// This server's authenticated transport key.
    pub server_peer: [u8; 32],
    /// Authenticated transport key on the incoming connection.
    pub connected_peer: [u8; 32],
    /// Device index requested from the remote protocol.
    pub requested_device: u32,
    /// Device indices this server has deliberately offered.
    pub offered_devices: &'a BTreeSet<u32>,
    /// Exact resource whose active lease may open this session.
    pub expected_resource: &'a ResourceId,
    /// Server clock reading used for the live lease projection.
    pub now_ms: u64,
    /// Server lease-time policy.
    pub lease_policy: &'a LeasePolicy,
}

/// Why a remote session claim was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RemoteClaimError {
    /// The opaque credential is too large to inspect safely.
    #[error("remote-session credential exceeds {MAX_REMOTE_SESSION_CREDENTIAL_BYTES} bytes")]
    CredentialTooLarge,
    /// The credential is not the canonical claim shape.
    #[error("invalid remote-session credential: {0}")]
    Decode(String),
    /// A claim from another wire generation cannot be interpreted here.
    #[error("unsupported remote-session claim version {0}")]
    UnsupportedVersion(u8),
    /// Signature bytes did not have the Ed25519 width.
    #[error("remote-session signature has the wrong length")]
    SignatureLength,
    /// The client key did not sign the complete claim.
    #[error("remote-session signature is invalid")]
    Signature,
    /// The claim belongs to another mesh.
    #[error("remote-session claim belongs to another mesh")]
    WrongMesh,
    /// The claim names another server transport identity.
    #[error("remote-session claim names another server")]
    WrongServer,
    /// This server's mesh author does not resolve to its transport identity.
    #[error("remote-session server has no matching device attestation")]
    UnattestedServer,
    /// The requested device differs from the signed device.
    #[error("remote-session claim names another device index")]
    WrongDevice,
    /// The signed device is not offered by this server.
    #[error("remote-session device index is not offered")]
    DeviceNotOffered,
    /// No such job exists on the current board.
    #[error("remote-session job is unknown")]
    UnknownJob,
    /// The signer is a ring member, but not the author who posted this job.
    #[error("remote-session client did not post this job")]
    NotJobPoster,
    /// The connecting transport identity is not the signer's attested device.
    #[error("remote-session connection does not belong to the client")]
    WrongClientPeer,
    /// The job leases a different resource.
    #[error("remote-session job names another resource")]
    WrongResource,
    /// The job is not in the exact live lease named by the claim.
    #[error("remote-session lease is not live on this server")]
    LeaseNotHeld,
}

impl RemoteSessionClaim {
    /// Sign a claim as the job poster's mesh author.
    #[allow(clippy::too_many_arguments)]
    pub fn signed(
        signer: &Ed25519Keypair,
        mesh_id: [u8; 32],
        job: JobId,
        lease: LeaseId,
        epoch: u32,
        server_peer: [u8; 32],
        device_index: u32,
    ) -> Self {
        let mut claim = Self {
            version: REMOTE_SESSION_CLAIM_VERSION,
            mesh_id,
            job,
            lease,
            epoch,
            client: signer.public_key().to_bytes(),
            server_peer,
            device_index,
            signature: Vec::new(),
        };
        claim.signature = signer.sign(&claim.signing_bytes()).to_bytes().to_vec();
        claim
    }

    /// Encode for Burn Remote's opaque authorization field.
    pub fn encode(&self) -> Result<Vec<u8>, RemoteClaimError> {
        p2panda_core::cbor::encode_cbor(self)
            .map_err(|error| RemoteClaimError::Decode(error.to_string()))
    }

    /// Decode a bounded opaque authorization field.
    pub fn decode(bytes: &[u8]) -> Result<Self, RemoteClaimError> {
        if bytes.len() > MAX_REMOTE_SESSION_CREDENTIAL_BYTES {
            return Err(RemoteClaimError::CredentialTooLarge);
        }
        p2panda_core::cbor::decode_cbor(bytes)
            .map_err(|error| RemoteClaimError::Decode(error.to_string()))
    }

    /// Job this session exercises.
    pub fn job(&self) -> JobId {
        self.job
    }

    /// Lease this session is bound to.
    pub fn lease(&self) -> LeaseId {
        self.lease
    }

    /// Lease epoch this session is bound to.
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    /// Mesh author that signed the claim.
    pub fn client(&self) -> [u8; 32] {
        self.client
    }

    /// Transport identity the claim may be presented to.
    pub fn server_peer(&self) -> [u8; 32] {
        self.server_peer
    }

    /// Offered compute-device index the claim selects.
    pub fn device_index(&self) -> u32 {
        self.device_index
    }

    /// Decide admission from the claim, authenticated connection, and current
    /// board projection.
    pub fn authorize(&self, admission: &RemoteAdmission<'_>) -> Result<(), RemoteClaimError> {
        self.verify_signature()?;
        if self.version != REMOTE_SESSION_CLAIM_VERSION {
            return Err(RemoteClaimError::UnsupportedVersion(self.version));
        }
        if self.mesh_id != admission.mesh_id {
            return Err(RemoteClaimError::WrongMesh);
        }
        if self.server_peer != admission.server_peer {
            return Err(RemoteClaimError::WrongServer);
        }
        if admission
            .board
            .devices()
            .master_of(&admission.server_author)
            != Some(admission.server_peer)
        {
            return Err(RemoteClaimError::UnattestedServer);
        }
        if self.device_index != admission.requested_device {
            return Err(RemoteClaimError::WrongDevice);
        }
        if !admission.offered_devices.contains(&self.device_index) {
            return Err(RemoteClaimError::DeviceNotOffered);
        }

        let job = admission
            .board
            .job(self.job)
            .ok_or(RemoteClaimError::UnknownJob)?;
        if job.posted_by != self.client {
            return Err(RemoteClaimError::NotJobPoster);
        }
        if admission.board.devices().master_of(&self.client) != Some(admission.connected_peer) {
            return Err(RemoteClaimError::WrongClientPeer);
        }
        if job.spec.as_deref().map(|spec| &spec.resource) != Some(admission.expected_resource) {
            return Err(RemoteClaimError::WrongResource);
        }
        if !matches!(
            job.lease_at(admission.now_ms, admission.lease_policy),
            LeasePhase::Held {
                epoch,
                lease,
                holder,
                ..
            } if epoch == self.epoch
                && lease == self.lease
                && holder == admission.server_author
        ) {
            return Err(RemoteClaimError::LeaseNotHeld);
        }
        Ok(())
    }

    fn verify_signature(&self) -> Result<(), RemoteClaimError> {
        let bytes: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| RemoteClaimError::SignatureLength)?;
        let key =
            Ed25519PublicKey::from_bytes(&self.client).map_err(|_| RemoteClaimError::Signature)?;
        let signature = Ed25519Signature::from_bytes(&bytes);
        if key.verify(&self.signing_bytes(), &signature) {
            Ok(())
        } else {
            Err(RemoteClaimError::Signature)
        }
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(SIGNING_CONTEXT.len() + 32 * 5 + 1 + 4 + 4);
        bytes.extend_from_slice(SIGNING_CONTEXT);
        bytes.push(self.version);
        bytes.extend_from_slice(&self.mesh_id);
        bytes.extend_from_slice(&self.job.0);
        bytes.extend_from_slice(&self.lease.0);
        bytes.extend_from_slice(&self.epoch.to_le_bytes());
        bytes.extend_from_slice(&self.client);
        bytes.extend_from_slice(&self.server_peer);
        bytes.extend_from_slice(&self.device_index.to_le_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DeterminismClass, LeaseTerms, MESH_AUTHOR_SALT, MeshEvent, ResourceId, to_operation,
    };
    use identity::{IdentityProvider, InMemoryProvider};
    use proofs::BlobRef;

    const MESH: [u8; 32] = [0x6d; 32];
    const NOW: u64 = 5_000;
    const EXACT: LeasePolicy = LeasePolicy { max_skew_ms: 0 };

    struct Device {
        provider: InMemoryProvider,
        author: Ed25519Keypair,
        seq: u32,
        backlink: Option<[u8; 32]>,
    }

    impl Device {
        fn new(seed: u8) -> Self {
            let provider = InMemoryProvider::from_seed([seed; 32]);
            let author = provider.derive_keypair(MESH_AUTHOR_SALT).unwrap();
            Self {
                provider,
                author,
                seq: 0,
                backlink: None,
            }
        }

        fn author_id(&self) -> [u8; 32] {
            self.author.public_key().to_bytes()
        }

        fn peer_id(&self) -> [u8; 32] {
            self.provider.master_public_key().to_bytes()
        }

        fn op(&mut self, event: &MeshEvent) -> p2panda_core::Operation<crate::MeshExt> {
            let op = to_operation(&self.author, MESH, event, self.seq, self.backlink);
            self.seq += 1;
            self.backlink = Some(*op.hash.as_bytes());
            op
        }

        fn attestation(&mut self) -> p2panda_core::Operation<crate::MeshExt> {
            let attestation = self.provider.attest_derived_key(MESH_AUTHOR_SALT).unwrap();
            self.op(&MeshEvent::DeviceAttested {
                attestation: Box::new(attestation),
            })
        }
    }

    struct Fixture {
        poster: Device,
        server: Device,
        stranger: Device,
        board: JobBoard,
        job: JobId,
        lease: LeaseId,
        resource: ResourceId,
        offered: BTreeSet<u32>,
    }

    impl Fixture {
        fn new() -> Self {
            let mut poster = Device::new(1);
            let mut server = Device::new(2);
            let mut stranger = Device::new(3);
            let resource = ResourceId::parse("esp.remote.burn/v1").unwrap();
            let spec = crate::JobSpec::simple(
                resource.clone(),
                "request",
                BlobRef::blake3(b"remote session request"),
                "receipt",
                1_024,
                DeterminismClass::Observed,
            )
            .leased(LeaseTerms::new(60_000, 10_000));
            let post = poster.op(&MeshEvent::JobPostedV2 {
                spec: Box::new(spec),
                nonce: 0,
                at_ms: 1_000,
            });
            let job = JobId(*post.hash.as_bytes());
            let claim = server.op(&MeshEvent::JobClaimed {
                job: job.0,
                at_ms: 2_000,
            });
            let grant = server.op(&MeshEvent::LeaseGranted {
                job: job.0,
                epoch: 0,
                granted_at_ms: 3_000,
                expires_at_ms: 63_000,
            });
            let lease = LeaseId(*grant.hash.as_bytes());
            let poster_attestation = poster.attestation();
            let server_attestation = server.attestation();
            let stranger_attestation = stranger.attestation();
            let board = JobBoard::fold(
                MESH,
                [
                    &post,
                    &claim,
                    &grant,
                    &poster_attestation,
                    &server_attestation,
                    &stranger_attestation,
                ],
            );
            Self {
                poster,
                server,
                stranger,
                board,
                job,
                lease,
                resource,
                offered: BTreeSet::from([0, 2]),
            }
        }

        fn signed_by(
            &self,
            signer: &Ed25519Keypair,
            mesh: [u8; 32],
            lease: LeaseId,
            epoch: u32,
            server_peer: [u8; 32],
            device_index: u32,
        ) -> RemoteSessionClaim {
            RemoteSessionClaim::signed(
                signer,
                mesh,
                self.job,
                lease,
                epoch,
                server_peer,
                device_index,
            )
        }

        fn claim(&self) -> RemoteSessionClaim {
            self.signed_by(
                &self.poster.author,
                MESH,
                self.lease,
                0,
                self.server.peer_id(),
                2,
            )
        }

        fn admission<'a>(&'a self, connected_peer: [u8; 32]) -> RemoteAdmission<'a> {
            RemoteAdmission {
                mesh_id: MESH,
                board: &self.board,
                server_author: self.server.author_id(),
                server_peer: self.server.peer_id(),
                connected_peer,
                requested_device: 2,
                offered_devices: &self.offered,
                expected_resource: &self.resource,
                now_ms: NOW,
                lease_policy: &EXACT,
            }
        }
    }

    #[test]
    fn a_poster_opens_the_exact_device_under_the_servers_live_lease() {
        let fixture = Fixture::new();
        let claim = fixture.claim();
        assert_eq!(claim.job(), fixture.job);
        assert_eq!(claim.lease(), fixture.lease);
        assert_eq!(claim.epoch(), 0);
        assert_eq!(claim.device_index(), 2);
        assert_eq!(claim.client(), fixture.poster.author_id());
        assert_eq!(claim.server_peer(), fixture.server.peer_id());
        assert_eq!(
            claim.authorize(&fixture.admission(fixture.poster.peer_id())),
            Ok(())
        );

        let encoded = claim.encode().unwrap();
        assert!(encoded.len() < MAX_REMOTE_SESSION_CREDENTIAL_BYTES);
        let decoded = RemoteSessionClaim::decode(&encoded).unwrap();
        assert_eq!(decoded, claim);
        assert_eq!(
            decoded.authorize(&fixture.admission(fixture.poster.peer_id())),
            Ok(())
        );
    }

    #[test]
    fn another_ring_member_cannot_hijack_the_posters_lease() {
        let fixture = Fixture::new();
        let claim = fixture.signed_by(
            &fixture.stranger.author,
            MESH,
            fixture.lease,
            0,
            fixture.server.peer_id(),
            2,
        );
        assert_eq!(
            claim.authorize(&fixture.admission(fixture.stranger.peer_id())),
            Err(RemoteClaimError::NotJobPoster)
        );
    }

    #[test]
    fn authenticated_client_and_server_transport_identities_are_bound() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture
                .claim()
                .authorize(&fixture.admission(fixture.stranger.peer_id())),
            Err(RemoteClaimError::WrongClientPeer)
        );

        let elsewhere = fixture.signed_by(
            &fixture.poster.author,
            MESH,
            fixture.lease,
            0,
            fixture.stranger.peer_id(),
            2,
        );
        assert_eq!(
            elsewhere.authorize(&fixture.admission(fixture.poster.peer_id())),
            Err(RemoteClaimError::WrongServer)
        );

        let mut wrong_server = fixture.admission(fixture.poster.peer_id());
        wrong_server.server_author = fixture.stranger.author_id();
        assert_eq!(
            fixture.claim().authorize(&wrong_server),
            Err(RemoteClaimError::UnattestedServer)
        );
    }

    #[test]
    fn mesh_lease_epoch_resource_and_device_are_all_exact() {
        let fixture = Fixture::new();
        let cases = [
            (
                fixture.signed_by(
                    &fixture.poster.author,
                    [9; 32],
                    fixture.lease,
                    0,
                    fixture.server.peer_id(),
                    2,
                ),
                RemoteClaimError::WrongMesh,
            ),
            (
                fixture.signed_by(
                    &fixture.poster.author,
                    MESH,
                    LeaseId([7; 32]),
                    0,
                    fixture.server.peer_id(),
                    2,
                ),
                RemoteClaimError::LeaseNotHeld,
            ),
            (
                fixture.signed_by(
                    &fixture.poster.author,
                    MESH,
                    fixture.lease,
                    1,
                    fixture.server.peer_id(),
                    2,
                ),
                RemoteClaimError::LeaseNotHeld,
            ),
            (
                fixture.signed_by(
                    &fixture.poster.author,
                    MESH,
                    fixture.lease,
                    0,
                    fixture.server.peer_id(),
                    1,
                ),
                RemoteClaimError::WrongDevice,
            ),
        ];
        for (claim, expected) in cases {
            assert_eq!(
                claim.authorize(&fixture.admission(fixture.poster.peer_id())),
                Err(expected)
            );
        }

        let mut unoffered = fixture.admission(fixture.poster.peer_id());
        unoffered.requested_device = 1;
        let claim = fixture.signed_by(
            &fixture.poster.author,
            MESH,
            fixture.lease,
            0,
            fixture.server.peer_id(),
            1,
        );
        assert_eq!(
            claim.authorize(&unoffered),
            Err(RemoteClaimError::DeviceNotOffered)
        );

        let other_resource = ResourceId::parse("esp.remote.other/v1").unwrap();
        let mut wrong_resource = fixture.admission(fixture.poster.peer_id());
        wrong_resource.expected_resource = &other_resource;
        assert_eq!(
            fixture.claim().authorize(&wrong_resource),
            Err(RemoteClaimError::WrongResource)
        );

        let mut expired = fixture.admission(fixture.poster.peer_id());
        expired.now_ms = 63_000;
        assert_eq!(
            fixture.claim().authorize(&expired),
            Err(RemoteClaimError::LeaseNotHeld)
        );
    }

    #[test]
    fn tampering_and_unbounded_credentials_are_refused() {
        let fixture = Fixture::new();
        let mut claim = fixture.claim();
        claim.device_index = 0;
        assert_eq!(
            claim.authorize(&fixture.admission(fixture.poster.peer_id())),
            Err(RemoteClaimError::Signature)
        );
        assert_eq!(
            RemoteSessionClaim::decode(&vec![0; MAX_REMOTE_SESSION_CREDENTIAL_BYTES + 1]),
            Err(RemoteClaimError::CredentialTooLarge)
        );
    }
}
