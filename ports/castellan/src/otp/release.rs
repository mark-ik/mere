// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Participant-claimed OTP code release.
//!
//! A host petitions [`OtpReleaseGate`] with carrier-supplied participant facts
//! and a sealed-item handle. The gate exposes only secret-free pending requests.
//! The resident authority then explicitly approves or denies one; only approval
//! produces an [`OtpCodeTile`]. There is deliberately no remote wire here. This
//! local type validates presentation safety, expiry, and resident consent.
//! [`super::OtpAdmittedSession`] supplies the authenticated, session-bound path;
//! direct callers remain explicitly unverified.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    OtpCodeTile, OtpItem, OtpItemError, OtpItemId, OtpItemStore, OtpReleaseParticipantClaim,
    OtpReleaseParticipantProof,
};

const DEFAULT_REQUEST_TTL_SECS: u64 = 5 * 60;
const DEFAULT_MAX_PENDING: usize = 128;

type ReleaseClock = dyn Fn() -> Result<u64, OtpReleaseError> + Send + Sync;

/// Resident-configurable limits for pending OTP release petitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OtpReleasePolicy {
    request_ttl_secs: u64,
    max_pending: usize,
}

impl OtpReleasePolicy {
    /// Build a policy with explicit request lifetime and queue capacity.
    pub fn new(request_ttl_secs: u64, max_pending: usize) -> Result<Self, OtpReleaseError> {
        if request_ttl_secs == 0 {
            return Err(OtpReleaseError::InvalidPolicy(
                "request lifetime must be at least one second",
            ));
        }
        if max_pending == 0 {
            return Err(OtpReleaseError::InvalidPolicy(
                "pending-request capacity must be at least one",
            ));
        }
        Ok(Self {
            request_ttl_secs,
            max_pending,
        })
    }

    /// Seconds a resident may wait before a petition expires.
    pub fn request_ttl_secs(self) -> u64 {
        self.request_ttl_secs
    }

    /// Maximum number of live petitions held in memory.
    pub fn max_pending(self) -> usize {
        self.max_pending
    }
}

impl Default for OtpReleasePolicy {
    fn default() -> Self {
        Self {
            request_ttl_secs: DEFAULT_REQUEST_TTL_SECS,
            max_pending: DEFAULT_MAX_PENDING,
        }
    }
}

/// Stable identifier for one pending OTP release petition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OtpReleaseId(uuid::Uuid);

impl OtpReleaseId {
    fn mint() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl fmt::Display for OtpReleaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One code-release request before the resident has decided it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtpReleaseRequest {
    /// Stable release-petition identifier used for approval or denial.
    pub id: OtpReleaseId,
    /// Carrier-supplied recipient facts shown to the resident.
    pub participant: OtpReleaseParticipantClaim,
    /// Secret-free item metadata shown to the resident before release.
    pub item: OtpItem,
    /// Gate-recorded Unix time when the petition arrived.
    pub requested_at_unix_secs: u64,
    /// Gate-recorded Unix time after which approval is refused.
    pub expires_at_unix_secs: u64,
}

/// An explicit denial receipt. It deliberately contains no code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtpReleaseDenied {
    /// The petition the resident denied.
    pub request: OtpReleaseRequest,
}

/// One approved code associated with the request the carrier must serve.
pub struct OtpReleasedCode {
    request: OtpReleaseRequest,
    tile: OtpCodeTile,
}

impl fmt::Debug for OtpReleasedCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OtpReleasedCode")
            .field("request", &self.request)
            .field("tile", &self.tile)
            .finish()
    }
}

impl OtpReleasedCode {
    /// The approval request associated with this code.
    pub fn request(&self) -> &OtpReleaseRequest {
        &self.request
    }

    /// The code tile the future carrier adapter must deliver to this request.
    ///
    /// Association here is structural. Cryptographic session-bound delivery is
    /// a carrier responsibility and has not been implemented by this local seam.
    pub fn tile(&self) -> &OtpCodeTile {
        &self.tile
    }
}

/// Failure while submitting or resolving an OTP release petition.
#[derive(Debug)]
pub enum OtpReleaseError {
    /// The candidate participant or session fact was absent, too long, padded
    /// with whitespace, or unsafe to retain in a visible request.
    InvalidParticipant,
    /// A resident configured an unusable queue policy.
    InvalidPolicy(&'static str),
    /// The host clock could not be represented as Unix time.
    ClockBeforeUnixEpoch,
    /// The pending-request queue reached its configured capacity.
    TooManyPending {
        /// Configured capacity at the time of refusal.
        limit: usize,
    },
    /// The petition reached its gate-recorded expiry before approval.
    Expired(OtpReleaseId),
    /// The sealed OTP item could not be read or exercised.
    Item(OtpItemError),
    /// The release had already been approved, denied, or was never pending.
    NotPending(OtpReleaseId),
    /// An admission-derived petition must be resolved through its live session.
    SessionBoundApprovalRequired(OtpReleaseId),
}

impl fmt::Display for OtpReleaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OtpReleaseError::InvalidParticipant => f.write_str(
                "release participant facts must be trimmed printable text of at most 256 characters",
            ),
            OtpReleaseError::InvalidPolicy(reason) => {
                write!(f, "invalid OTP release policy: {reason}")
            }
            OtpReleaseError::ClockBeforeUnixEpoch => f.write_str("the host clock is before 1970"),
            OtpReleaseError::TooManyPending { limit } => {
                write!(f, "the OTP release queue has reached its {limit}-request limit")
            }
            OtpReleaseError::Expired(id) => write!(f, "OTP release {id} has expired"),
            OtpReleaseError::Item(error) => write!(f, "OTP item: {error}"),
            OtpReleaseError::NotPending(id) => write!(f, "OTP release {id} is not pending"),
            OtpReleaseError::SessionBoundApprovalRequired(id) => write!(
                f,
                "OTP release {id} must be approved through its admitted session"
            ),
        }
    }
}

impl std::error::Error for OtpReleaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OtpReleaseError::Item(error) => Some(error),
            OtpReleaseError::InvalidParticipant
            | OtpReleaseError::InvalidPolicy(_)
            | OtpReleaseError::ClockBeforeUnixEpoch
            | OtpReleaseError::TooManyPending { .. }
            | OtpReleaseError::Expired(_)
            | OtpReleaseError::NotPending(_)
            | OtpReleaseError::SessionBoundApprovalRequired(_) => None,
        }
    }
}

impl From<OtpItemError> for OtpReleaseError {
    fn from(error: OtpItemError) -> Self {
        Self::Item(error)
    }
}

/// Resident approval authority for one persona's sealed OTP item store.
///
/// The gate owns the only code-bearing operation for sealed OTP items. HOTP
/// serialization lives below it in the sealed-store update transaction, so
/// independent gates over clones of one opened store cannot issue one counter.
#[derive(Clone)]
pub struct OtpReleaseGate {
    store: OtpItemStore,
    pending: Arc<Mutex<BTreeMap<OtpReleaseId, OtpReleaseRequest>>>,
    policy: OtpReleasePolicy,
    clock: Arc<ReleaseClock>,
}

impl OtpReleaseGate {
    /// Build a resident release gate with the default queue policy.
    pub fn new(store: OtpItemStore) -> Self {
        Self::with_clock(
            store,
            OtpReleasePolicy::default(),
            Arc::new(system_unix_secs),
        )
    }

    /// Build a resident release gate with explicit queue limits.
    pub fn with_policy(store: OtpItemStore, policy: OtpReleasePolicy) -> Self {
        Self::with_clock(store, policy, Arc::new(system_unix_secs))
    }

    /// The persona namespace whose sealed OTP items this gate may exercise.
    pub fn persona(&self) -> personae::PersonaId {
        self.store.persona()
    }

    fn with_clock(store: OtpItemStore, policy: OtpReleasePolicy, clock: Arc<ReleaseClock>) -> Self {
        Self {
            store,
            pending: Arc::new(Mutex::new(BTreeMap::new())),
            policy,
            clock,
        }
    }

    /// Submit carrier-supplied participant claims for resident approval.
    pub fn petition(
        &self,
        item_id: OtpItemId,
        participant: OtpReleaseParticipantClaim,
    ) -> Result<OtpReleaseRequest, OtpReleaseError> {
        let now = (self.clock)()?;
        let item = self
            .store
            .get(item_id)?
            .ok_or(OtpItemError::NotFound(item_id))?;
        let mut pending = lock_pending(&self.pending);
        retain_live(&mut pending, now);
        if pending.len() >= self.policy.max_pending {
            return Err(OtpReleaseError::TooManyPending {
                limit: self.policy.max_pending,
            });
        }
        let request = OtpReleaseRequest {
            id: OtpReleaseId::mint(),
            participant,
            item,
            requested_at_unix_secs: now,
            expires_at_unix_secs: now.saturating_add(self.policy.request_ttl_secs),
        };
        pending.insert(request.id, request.clone());
        Ok(request)
    }

    /// Snapshot live requests that still need a resident decision.
    pub fn pending(&self) -> Result<Vec<OtpReleaseRequest>, OtpReleaseError> {
        let now = (self.clock)()?;
        let mut pending = lock_pending(&self.pending);
        retain_live(&mut pending, now);
        let mut requests: Vec<_> = pending.values().cloned().collect();
        requests.sort_by_key(|request| (request.requested_at_unix_secs, request.id));
        Ok(requests)
    }

    /// Approve one live, explicitly unverified local petition.
    ///
    /// Admission-derived petitions must use [`super::OtpAdmittedSession`],
    /// which rechecks the retained authority before exercising the item.
    pub fn approve(&self, id: OtpReleaseId) -> Result<OtpReleasedCode, OtpReleaseError> {
        self.approve_with_proof(id, OtpReleaseParticipantProof::Unverified)
    }

    pub(super) fn approve_admitted(
        &self,
        id: OtpReleaseId,
    ) -> Result<OtpReleasedCode, OtpReleaseError> {
        self.approve_with_proof(id, OtpReleaseParticipantProof::AdmittedSession)
    }

    fn approve_with_proof(
        &self,
        id: OtpReleaseId,
        expected_proof: OtpReleaseParticipantProof,
    ) -> Result<OtpReleasedCode, OtpReleaseError> {
        let now = (self.clock)()?;
        let mut pending = lock_pending(&self.pending);
        let request = pending
            .get(&id)
            .cloned()
            .ok_or(OtpReleaseError::NotPending(id))?;
        if request.participant.proof() != expected_proof {
            return Err(OtpReleaseError::SessionBoundApprovalRequired(id));
        }
        if now >= request.expires_at_unix_secs {
            pending.remove(&id);
            return Err(OtpReleaseError::Expired(id));
        }
        let tile = self.store.release_tile_at_unix_time(request.item.id, now)?;
        pending.remove(&id);
        Ok(OtpReleasedCode { request, tile })
    }

    /// Deny one pending petition without exercising its OTP item.
    pub fn deny(&self, id: OtpReleaseId) -> Result<OtpReleaseDenied, OtpReleaseError> {
        let request = lock_pending(&self.pending)
            .remove(&id)
            .ok_or(OtpReleaseError::NotPending(id))?;
        Ok(OtpReleaseDenied { request })
    }
}

fn lock_pending(
    pending: &Mutex<BTreeMap<OtpReleaseId, OtpReleaseRequest>>,
) -> MutexGuard<'_, BTreeMap<OtpReleaseId, OtpReleaseRequest>> {
    pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn retain_live(pending: &mut BTreeMap<OtpReleaseId, OtpReleaseRequest>, now: u64) {
    pending.retain(|_, request| now < request.expires_at_unix_secs);
}

fn system_unix_secs() -> Result<u64, OtpReleaseError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| OtpReleaseError::ClockBeforeUnixEpoch)
}

#[cfg(test)]
#[path = "release_concurrency_tests.rs"]
mod concurrency_tests;

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use tempfile::tempdir;

    use super::*;
    use personae::{PersonaId, SealedRecordStorage};

    const RFC4226_SECRET_BASE32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[derive(Clone)]
    struct ManualClock(Arc<AtomicU64>);

    impl ManualClock {
        fn new(now: u64) -> Self {
            Self(Arc::new(AtomicU64::new(now)))
        }

        fn set(&self, now: u64) {
            self.0.store(now, Ordering::SeqCst);
        }

        fn function(&self) -> Arc<ReleaseClock> {
            let state = Arc::clone(&self.0);
            Arc::new(move || Ok(state.load(Ordering::SeqCst)))
        }
    }

    fn store(root: &std::path::Path, persona: PersonaId) -> OtpItemStore {
        OtpItemStore::new(
            SealedRecordStorage::open_with_key(root, [0x62; 32]),
            persona,
        )
    }

    fn test_gate(items: OtpItemStore, clock: &ManualClock) -> OtpReleaseGate {
        OtpReleaseGate::with_clock(items, OtpReleasePolicy::default(), clock.function())
    }

    fn participant() -> OtpReleaseParticipantClaim {
        OtpReleaseParticipantClaim::unverified("device:q-pc", "session:carrier-proof").unwrap()
    }

    #[test]
    fn approval_uses_gate_time_and_releases_a_live_tile() {
        let dir = tempdir().unwrap();
        let items = store(dir.path(), PersonaId::new());
        let item = items
            .import_otpauth_uri(&format!(
                "otpauth://totp/Merely:mark?secret={RFC4226_SECRET_BASE32}&issuer=Merely&digits=8"
            ))
            .unwrap();
        let clock = ManualClock::new(58);
        let gate = test_gate(items, &clock);
        let request = gate.petition(item.id, participant()).unwrap();

        assert_eq!(gate.pending().unwrap(), vec![request.clone()]);
        clock.set(59);
        let released = gate.approve(request.id).unwrap();

        assert_eq!(released.request(), &request);
        assert_eq!(released.tile().code_at_unix_time(59), Some("94287082"));
        let ring = released.tile().time_ring().unwrap();
        assert_eq!(ring.seconds_remaining_at(59), 1);
        assert_eq!(ring.expires_at_unix_secs, 60);
        assert!(gate.pending().unwrap().is_empty());
        assert!(matches!(
            gate.approve(request.id),
            Err(OtpReleaseError::NotPending(id)) if id == request.id
        ));
        let debug = format!("{released:?}");
        assert!(!debug.contains("94287082"));
        assert!(!debug.contains("session:carrier-proof"));
    }

    #[test]
    fn denial_removes_the_petition_without_consuming_hotp() {
        let dir = tempdir().unwrap();
        let items = store(dir.path(), PersonaId::new());
        let item = items
            .import_otpauth_uri(&format!(
                "otpauth://hotp/Merely:mark?secret={RFC4226_SECRET_BASE32}&issuer=Merely&counter=0"
            ))
            .unwrap();
        let clock = ManualClock::new(1);
        let gate = test_gate(items, &clock);
        let denied = gate.petition(item.id, participant()).unwrap();

        assert_eq!(gate.deny(denied.id).unwrap().request, denied);
        clock.set(2);
        let approved = gate.petition(item.id, participant()).unwrap();
        assert_eq!(
            gate.approve(approved.id)
                .unwrap()
                .tile()
                .code_at_unix_time(2),
            Some("755224")
        );
    }

    #[test]
    fn approved_hotp_counter_survives_gate_reopen() {
        let dir = tempdir().unwrap();
        let persona = PersonaId::new();
        let items = store(dir.path(), persona);
        let item = items
            .import_otpauth_uri(&format!(
                "otpauth://hotp/Merely:mark?secret={RFC4226_SECRET_BASE32}&issuer=Merely&counter=0"
            ))
            .unwrap();
        let clock = ManualClock::new(1);
        let gate = test_gate(items, &clock);

        let first = gate.petition(item.id, participant()).unwrap();
        assert_eq!(
            gate.approve(first.id).unwrap().tile().code_at_unix_time(1),
            Some("755224")
        );

        clock.set(2);
        let reopened = test_gate(store(dir.path(), persona), &clock);
        let second = reopened.petition(item.id, participant()).unwrap();
        assert_eq!(
            reopened
                .approve(second.id)
                .unwrap()
                .tile()
                .code_at_unix_time(2),
            Some("287082")
        );
    }

    #[test]
    fn expired_petition_cannot_release_a_code() {
        let dir = tempdir().unwrap();
        let items = store(dir.path(), PersonaId::new());
        let item = items
            .import_otpauth_uri(&format!(
                "otpauth://totp/Merely:mark?secret={RFC4226_SECRET_BASE32}&issuer=Merely"
            ))
            .unwrap();
        let clock = ManualClock::new(10);
        let policy = OtpReleasePolicy::new(5, 2).unwrap();
        let gate = OtpReleaseGate::with_clock(items, policy, clock.function());
        let request = gate.petition(item.id, participant()).unwrap();

        clock.set(15);
        assert!(matches!(
            gate.approve(request.id),
            Err(OtpReleaseError::Expired(id)) if id == request.id
        ));
    }

    #[test]
    fn pending_capacity_is_configurable_and_bounded() {
        let dir = tempdir().unwrap();
        let items = store(dir.path(), PersonaId::new());
        let item = items
            .import_otpauth_uri(&format!(
                "otpauth://totp/Merely:mark?secret={RFC4226_SECRET_BASE32}&issuer=Merely"
            ))
            .unwrap();
        let clock = ManualClock::new(10);
        let policy = OtpReleasePolicy::new(60, 1).unwrap();
        let gate = OtpReleaseGate::with_clock(items, policy, clock.function());
        gate.petition(item.id, participant()).unwrap();

        assert!(matches!(
            gate.petition(item.id, participant()),
            Err(OtpReleaseError::TooManyPending { limit: 1 })
        ));
    }

    #[test]
    fn participant_facts_must_be_presentable_and_session_debug_is_redacted() {
        assert!(matches!(
            OtpReleaseParticipantClaim::unverified("", "session"),
            Err(OtpReleaseError::InvalidParticipant)
        ));
        assert!(matches!(
            OtpReleaseParticipantClaim::unverified("device", "session\nsecret"),
            Err(OtpReleaseError::InvalidParticipant)
        ));
        assert!(matches!(
            OtpReleaseParticipantClaim::unverified(" device", "session"),
            Err(OtpReleaseError::InvalidParticipant)
        ));
        assert!(!format!("{:?}", participant()).contains("session:carrier-proof"));
    }
}
