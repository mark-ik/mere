// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Participant-gated OTP code release.
//!
//! A host petitions [`OtpReleaseGate`] with carrier-authenticated participant
//! facts and a sealed-item handle. The gate exposes only secret-free pending
//! requests. The resident authority then explicitly approves or denies one;
//! only approval produces an [`OtpCodeTile`]. There is deliberately no remote
//! wire here. A carrier must prove its participant and session before a wire
//! vocabulary would be worth fixing.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use super::{OtpCodeTile, OtpItem, OtpItemError, OtpItemId, OtpItemStore};

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

/// Carrier-authenticated participant facts attached to a release petition.
///
/// `principal` identifies the participant the carrier authenticated, not a
/// mutable display label. `session_binding` proves the request belongs to one
/// live session. The gate retains both for the visible approval surface and
/// does not attempt to invent transport authentication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtpReleaseParticipant {
    principal: String,
    session_binding: String,
}

impl OtpReleaseParticipant {
    /// Validate carrier-authenticated participant and session facts.
    pub fn new(
        principal: impl Into<String>,
        session_binding: impl Into<String>,
    ) -> Result<Self, OtpReleaseError> {
        let principal = validate_participant_fact(principal.into())?;
        let session_binding = validate_participant_fact(session_binding.into())?;
        Ok(Self {
            principal,
            session_binding,
        })
    }

    /// Stable identity of the participant the carrier authenticated.
    pub fn principal(&self) -> &str {
        &self.principal
    }

    /// Opaque binding of the petition to the participant's live session.
    pub fn session_binding(&self) -> &str {
        &self.session_binding
    }
}

/// One code-release request before the resident has decided it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtpReleaseRequest {
    /// Stable release-petition identifier used for approval or denial.
    pub id: OtpReleaseId,
    /// Carrier-authenticated recipient facts.
    pub participant: OtpReleaseParticipant,
    /// Secret-free item metadata shown to the resident before release.
    pub item: OtpItem,
    /// Carrier-recorded Unix time when the petition arrived.
    pub requested_at_unix_secs: u64,
}

/// An explicit denial receipt. It deliberately contains no code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtpReleaseDenied {
    /// The petition the resident denied.
    pub request: OtpReleaseRequest,
}

/// One approved code, bound to the request whose participant may receive it.
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
    /// The participant-bound approval that authorized this code.
    pub fn request(&self) -> &OtpReleaseRequest {
        &self.request
    }

    /// The code tile that may be delivered only to this request's participant.
    pub fn tile(&self) -> &OtpCodeTile {
        &self.tile
    }
}

/// Failure while submitting or resolving an OTP release petition.
#[derive(Debug)]
pub enum OtpReleaseError {
    /// The candidate participant or session fact was absent, too long, or not
    /// safe to retain in the visible pending-request model.
    InvalidParticipant,
    /// The sealed OTP item could not be read or exercised.
    Item(OtpItemError),
    /// The release had already been approved, denied, or was never pending.
    NotPending(OtpReleaseId),
}

impl fmt::Display for OtpReleaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OtpReleaseError::InvalidParticipant => {
                f.write_str("release participant facts must be printable nonempty text")
            }
            OtpReleaseError::Item(error) => write!(f, "OTP item: {error}"),
            OtpReleaseError::NotPending(id) => write!(f, "OTP release {id} is not pending"),
        }
    }
}

impl std::error::Error for OtpReleaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OtpReleaseError::Item(error) => Some(error),
            OtpReleaseError::InvalidParticipant | OtpReleaseError::NotPending(_) => None,
        }
    }
}

impl From<OtpItemError> for OtpReleaseError {
    fn from(error: OtpItemError) -> Self {
        Self::Item(error)
    }
}

/// Resident authority for one persona's sealed OTP item store.
///
/// The gate owns the only code-bearing store operation. Its mutex covers
/// resolution through a durable HOTP counter update, so two approvals cannot
/// issue the same counter even when they arrive on different host threads.
#[derive(Clone)]
pub struct OtpReleaseGate {
    store: OtpItemStore,
    pending: Arc<Mutex<BTreeMap<OtpReleaseId, OtpReleaseRequest>>>,
}

impl OtpReleaseGate {
    /// Build the resident release gate over an already-unlocked item store.
    pub fn new(store: OtpItemStore) -> Self {
        Self {
            store,
            pending: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Submit a carrier-authenticated petition for one item.
    pub fn petition(
        &self,
        item_id: OtpItemId,
        participant: OtpReleaseParticipant,
        requested_at_unix_secs: u64,
    ) -> Result<OtpReleaseRequest, OtpReleaseError> {
        let item = self
            .store
            .get(item_id)?
            .ok_or(OtpItemError::NotFound(item_id))?;
        let request = OtpReleaseRequest {
            id: OtpReleaseId::mint(),
            participant,
            item,
            requested_at_unix_secs,
        };
        self.pending
            .lock()
            .unwrap()
            .insert(request.id, request.clone());
        Ok(request)
    }

    /// Snapshot the requests that still need a resident decision.
    pub fn pending(&self) -> Vec<OtpReleaseRequest> {
        let mut pending: Vec<_> = self.pending.lock().unwrap().values().cloned().collect();
        pending.sort_by_key(|request| (request.requested_at_unix_secs, request.id));
        pending
    }

    /// Approve one pending petition and issue its participant-bound code tile.
    pub fn approve(
        &self,
        id: OtpReleaseId,
        released_at_unix_secs: u64,
    ) -> Result<OtpReleasedCode, OtpReleaseError> {
        let mut pending = self.pending.lock().unwrap();
        let request = pending
            .get(&id)
            .cloned()
            .ok_or(OtpReleaseError::NotPending(id))?;
        let tile = self
            .store
            .release_tile_at_unix_time(request.item.id, released_at_unix_secs)?;
        pending.remove(&id);
        Ok(OtpReleasedCode { request, tile })
    }

    /// Deny one pending petition without exercising its OTP item.
    pub fn deny(&self, id: OtpReleaseId) -> Result<OtpReleaseDenied, OtpReleaseError> {
        let request = self
            .pending
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or(OtpReleaseError::NotPending(id))?;
        Ok(OtpReleaseDenied { request })
    }
}

fn validate_participant_fact(value: String) -> Result<String, OtpReleaseError> {
    if value.is_empty()
        || value.trim() != value
        || value.chars().count() > 256
        || value.chars().any(char::is_control)
    {
        return Err(OtpReleaseError::InvalidParticipant);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use personae::{PersonaId, SealedRecordStorage};

    const RFC4226_SECRET_BASE32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn store(root: &std::path::Path, persona: PersonaId) -> OtpItemStore {
        OtpItemStore::new(
            SealedRecordStorage::open_with_key(root, [0x62; 32]),
            persona,
        )
    }

    fn participant() -> OtpReleaseParticipant {
        OtpReleaseParticipant::new("device:q-pc", "session:carrier-proof").unwrap()
    }

    #[test]
    fn approval_releases_a_tile_only_to_the_pending_participant() {
        let dir = tempdir().unwrap();
        let items = store(dir.path(), PersonaId::new());
        let item = items
            .import_otpauth_uri(&format!(
                "otpauth://totp/Merely:mark?secret={RFC4226_SECRET_BASE32}&issuer=Merely&digits=8"
            ))
            .unwrap();
        let gate = OtpReleaseGate::new(items);
        let request = gate.petition(item.id, participant(), 58).unwrap();

        assert_eq!(gate.pending(), vec![request.clone()]);
        let released = gate.approve(request.id, 59).unwrap();

        assert_eq!(released.request(), &request);
        assert_eq!(released.tile().code(), "94287082");
        assert_eq!(released.tile().time_ring().unwrap().seconds_remaining, 1);
        assert!(gate.pending().is_empty());
        assert!(matches!(
            gate.approve(request.id, 59),
            Err(OtpReleaseError::NotPending(id)) if id == request.id
        ));
        assert!(!format!("{released:?}").contains("94287082"));
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
        let gate = OtpReleaseGate::new(items);
        let denied = gate.petition(item.id, participant(), 1).unwrap();

        assert_eq!(gate.deny(denied.id).unwrap().request, denied);
        let approved = gate.petition(item.id, participant(), 2).unwrap();
        assert_eq!(
            gate.approve(approved.id, 2).unwrap().tile().code(),
            "755224"
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
        let gate = OtpReleaseGate::new(items);

        let first = gate.petition(item.id, participant(), 1).unwrap();
        assert_eq!(gate.approve(first.id, 1).unwrap().tile().code(), "755224");

        let reopened = OtpReleaseGate::new(store(dir.path(), persona));
        let second = reopened.petition(item.id, participant(), 2).unwrap();
        assert_eq!(
            reopened.approve(second.id, 2).unwrap().tile().code(),
            "287082"
        );
    }

    #[test]
    fn participant_facts_must_be_presentable_but_not_free_form_logs() {
        assert!(matches!(
            OtpReleaseParticipant::new("", "session"),
            Err(OtpReleaseError::InvalidParticipant)
        ));
        assert!(matches!(
            OtpReleaseParticipant::new("device", "session\nsecret"),
            Err(OtpReleaseError::InvalidParticipant)
        ));
        assert!(matches!(
            OtpReleaseParticipant::new(" device", "session"),
            Err(OtpReleaseError::InvalidParticipant)
        ));
    }
}
