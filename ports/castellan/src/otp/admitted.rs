// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Session-bound OTP release over Notochord admission.
//!
//! This adapter deliberately supplies no byte codec. A host gives Notochord an
//! authenticated carrier, then gives the resulting [`AdmittedSession`] to
//! [`OtpAdmittedSession`]. Castellan owns the identity, item scope, approval,
//! and final session binding. The host only receives the original carrier and
//! the approved tile together, at the point where its existing protocol writes
//! the result.

use std::collections::BTreeSet;
use std::fmt;

use notochord::{
    AdmittedSession, AuthorityLapse, LocalNetworkPolicy, NetworkId, ProfileRef, RetainedAuthority,
    RevocationLedger, ServiceAccess, ServiceRule, TrustedRoot,
};
use personae::PersonaId;

use super::{
    OtpCodeTile, OtpItemId, OtpReleaseDenied, OtpReleaseError, OtpReleaseGate, OtpReleaseId,
    OtpReleaseParticipantClaim, OtpReleaseRequest, OtpReleasedCode,
};

/// Notochord application domain for code release.
pub const OTP_RELEASE_DOMAIN: &str = "mere.castellan";
/// Structural root for persona-scoped OTP release items.
pub const OTP_RELEASE_SERVICE: &str = "/services/castellan/otp";
/// Capability action required to exercise one OTP item.
pub const OTP_RELEASE_ACTION: &str = "release";

/// The exact Notochord resource path for one persona-scoped OTP item.
pub fn otp_item_path(persona: PersonaId, item: OtpItemId) -> String {
    format!("{OTP_RELEASE_SERVICE}/{}/{item}", persona.as_uuid())
}

/// Owner policy for session-bound OTP release.
///
/// Each admission must name a persona and item below [`OTP_RELEASE_SERVICE`]. The rule
/// requires both a valid Personae delegation and transport-authenticated peer
/// identity; Notochord also requires that peer identity to equal the claimed
/// subject.
pub fn otp_release_policy(
    network: NetworkId,
    trusted_roots: Vec<TrustedRoot>,
    accepted_profiles: Vec<ProfileRef>,
    max_sessions: Option<u32>,
) -> LocalNetworkPolicy {
    let mut policy = LocalNetworkPolicy::closed(network);
    policy.trusted_roots = trusted_roots;
    policy.accepted_profiles = accepted_profiles;
    policy.services.insert(
        OTP_RELEASE_SERVICE.into(),
        ServiceRule::new(
            ServiceAccess::MemberOnly,
            OTP_RELEASE_DOMAIN,
            [OTP_RELEASE_ACTION],
            true,
            max_sessions,
        ),
    );
    policy
}

/// Failure while binding, approving, or delivering an admitted OTP release.
#[derive(Debug)]
pub enum OtpAdmittedReleaseError {
    /// The session was admitted for another service, action, or item path.
    ActionNotServed,
    /// The admitted chain did not cover its claimed OTP item at this check.
    ActionNotCovered,
    /// Authority expired or was revoked before the operation could finish.
    AuthorityLapsed(AuthorityLapse),
    /// The request was not petitioned by this admitted session.
    ForeignRequest(OtpReleaseId),
    /// The approved value belongs to another transcript-bound session.
    ForeignApproval,
    /// The TOTP step ended before delivery began.
    CodeExpired,
    /// The local release gate could not complete its operation.
    Release(OtpReleaseError),
}

impl fmt::Display for OtpAdmittedReleaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ActionNotServed => {
                f.write_str("the admitted session does not name one Castellan OTP release item")
            }
            Self::ActionNotCovered => {
                f.write_str("the admitted authority does not cover its OTP release item")
            }
            Self::AuthorityLapsed(AuthorityLapse::Expired { at_ms }) => {
                write!(f, "the admitted authority expired at {at_ms}")
            }
            Self::AuthorityLapsed(AuthorityLapse::Revoked) => {
                f.write_str("the admitted authority was revoked")
            }
            Self::ForeignRequest(id) => {
                write!(f, "OTP release {id} belongs to another admitted session")
            }
            Self::ForeignApproval => {
                f.write_str("the approved OTP release belongs to another admitted session")
            }
            Self::CodeExpired => f.write_str("the approved TOTP code expired before delivery"),
            Self::Release(error) => write!(f, "OTP release gate: {error}"),
        }
    }
}

impl std::error::Error for OtpAdmittedReleaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Release(error) => Some(error),
            _ => None,
        }
    }
}

impl From<OtpReleaseError> for OtpAdmittedReleaseError {
    fn from(error: OtpReleaseError) -> Self {
        Self::Release(error)
    }
}

/// One Notochord-admitted session allowed to petition for one exact
/// persona-scoped OTP item.
///
/// The carrier stays private until [`Self::begin_delivery`] pairs it with a
/// same-session approval. Dropping this object cancels its unresolved local
/// petitions and drops the carrier.
pub struct OtpAdmittedSession<S> {
    carrier: S,
    authority: RetainedAuthority,
    participant: OtpReleaseParticipantClaim,
    persona: PersonaId,
    item: OtpItemId,
    session_id: [u8; 32],
    gate: OtpReleaseGate,
    pending: BTreeSet<OtpReleaseId>,
}

impl<S> OtpAdmittedSession<S> {
    /// Consume one admitted carrier session and bind it to its exact OTP item.
    pub fn from_admitted(
        session: AdmittedSession<S>,
        gate: OtpReleaseGate,
        ledger: &RevocationLedger,
        now_ms: u64,
    ) -> Result<Self, OtpAdmittedReleaseError> {
        let action = &session.principal.action;
        if action.domain != OTP_RELEASE_DOMAIN || action.action != OTP_RELEASE_ACTION {
            return Err(OtpAdmittedReleaseError::ActionNotServed);
        }
        let (persona, item) =
            parse_item_path(&action.path).ok_or(OtpAdmittedReleaseError::ActionNotServed)?;
        if gate.persona() != persona {
            return Err(OtpAdmittedReleaseError::ActionNotServed);
        }
        let authority = RetainedAuthority::from_admitted(&session);
        check_authority(&authority, ledger, now_ms)?;

        let session_id = session.principal.session_id;
        let participant = OtpReleaseParticipantClaim::admitted(
            format!("personae:ed25519:{}", hex(&session.principal.subject)),
            format!("notochord:{}", hex(&session_id)),
        );
        Ok(Self {
            carrier: session.stream,
            authority,
            participant,
            persona,
            item,
            session_id,
            gate,
            pending: BTreeSet::new(),
        })
    }

    /// The single item named by this session's admitted action.
    pub fn item(&self) -> OtpItemId {
        self.item
    }

    /// The persona namespace named by this session's admitted action.
    pub fn persona(&self) -> PersonaId {
        self.persona
    }

    /// Petition the resident for the one admitted item.
    pub fn petition(
        &mut self,
        ledger: &RevocationLedger,
        now_ms: u64,
    ) -> Result<OtpReleaseRequest, OtpAdmittedReleaseError> {
        check_authority(&self.authority, ledger, now_ms)?;
        let request = self.gate.petition(self.item, self.participant.clone())?;
        self.pending.insert(request.id);
        Ok(request)
    }

    /// Approve a petition owned by this live admitted session.
    ///
    /// The returned value exposes no tile. Only the same session can turn it
    /// into an [`OtpSessionDelivery`].
    pub fn approve(
        &mut self,
        id: OtpReleaseId,
        ledger: &RevocationLedger,
        now_ms: u64,
    ) -> Result<OtpApprovedRelease, OtpAdmittedReleaseError> {
        if !self.pending.contains(&id) {
            return Err(OtpAdmittedReleaseError::ForeignRequest(id));
        }
        check_authority(&self.authority, ledger, now_ms)?;
        let released = self.gate.approve_admitted(id)?;
        self.pending.remove(&id);
        Ok(OtpApprovedRelease {
            session_id: self.session_id,
            released,
        })
    }

    /// Deny a petition owned by this admitted session without exercising it.
    pub fn deny(&mut self, id: OtpReleaseId) -> Result<OtpReleaseDenied, OtpAdmittedReleaseError> {
        if !self.pending.contains(&id) {
            return Err(OtpAdmittedReleaseError::ForeignRequest(id));
        }
        let denied = self.gate.deny(id)?;
        self.pending.remove(&id);
        Ok(denied)
    }

    /// Pair one approved value with this session's original carrier.
    ///
    /// The returned guard borrows `ledger`. A host that passes a read guard
    /// from its revocation lock therefore holds that guard through the final
    /// carrier write, linearizing owner revocation before or after delivery.
    pub fn begin_delivery<'a>(
        &'a mut self,
        approved: OtpApprovedRelease,
        ledger: &'a RevocationLedger,
        now_ms: u64,
    ) -> Result<OtpSessionDelivery<'a, S>, OtpAdmittedReleaseError> {
        if approved.session_id != self.session_id {
            return Err(OtpAdmittedReleaseError::ForeignApproval);
        }
        check_authority(&self.authority, ledger, now_ms)?;
        if approved
            .released
            .tile()
            .code_at_unix_time(now_ms / 1_000)
            .is_none()
        {
            return Err(OtpAdmittedReleaseError::CodeExpired);
        }
        Ok(OtpSessionDelivery {
            carrier: &mut self.carrier,
            released: approved.released,
            _revocations: ledger,
        })
    }
}

impl<S> Drop for OtpAdmittedSession<S> {
    fn drop(&mut self) {
        for id in std::mem::take(&mut self.pending) {
            let _ = self.gate.deny(id);
        }
    }
}

/// An approved code that remains opaque until its admitted session delivers it.
pub struct OtpApprovedRelease {
    session_id: [u8; 32],
    released: OtpReleasedCode,
}

impl fmt::Debug for OtpApprovedRelease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OtpApprovedRelease")
            .field("session_id", &"<redacted>")
            .field("released", &self.released)
            .finish()
    }
}

/// Final same-session delivery access for a host-owned application protocol.
///
/// This is the only adapter type that exposes both the carrier and code tile.
/// It invents no encoding: the host writes its existing typed message while
/// this guard keeps the approval and revocation snapshot beside the carrier.
pub struct OtpSessionDelivery<'a, S> {
    carrier: &'a mut S,
    released: OtpReleasedCode,
    _revocations: &'a RevocationLedger,
}

impl<'a, S> OtpSessionDelivery<'a, S> {
    /// Borrow the original carrier and approved tile together for one write.
    pub fn parts(&mut self) -> (&mut S, &OtpCodeTile) {
        (self.carrier, self.released.tile())
    }

    /// The resident-approved petition associated with this delivery.
    pub fn request(&self) -> &OtpReleaseRequest {
        self.released.request()
    }
}

fn parse_item_path(path: &str) -> Option<(PersonaId, OtpItemId)> {
    let suffix = path.strip_prefix(OTP_RELEASE_SERVICE)?.strip_prefix('/')?;
    let (persona, item) = suffix.split_once('/')?;
    let persona = PersonaId::from_uuid(uuid::Uuid::parse_str(persona).ok()?);
    let item = OtpItemId::from_uuid(uuid::Uuid::parse_str(item).ok()?);
    (otp_item_path(persona, item) == path).then_some((persona, item))
}

fn check_authority(
    authority: &RetainedAuthority,
    ledger: &RevocationLedger,
    now_ms: u64,
) -> Result<(), OtpAdmittedReleaseError> {
    if let Some(lapse) = authority.lapse(ledger, now_ms) {
        return Err(OtpAdmittedReleaseError::AuthorityLapsed(lapse));
    }
    let action = &authority.principal().action;
    if !authority.covers(&action.path, OTP_RELEASE_ACTION, now_ms) {
        return Err(OtpAdmittedReleaseError::ActionNotCovered);
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;

    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut encoded, byte| {
            write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
            encoded
        },
    )
}

#[cfg(test)]
#[path = "admitted_tests.rs"]
mod tests;
