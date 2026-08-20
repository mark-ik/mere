// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Presentable participant facts and their local authentication provenance.

use std::fmt;

use super::OtpReleaseError;

/// How Castellan learned the participant facts attached to a petition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtpReleaseParticipantProof {
    /// A local caller supplied presentable text without authentication evidence.
    Unverified,
    /// Notochord admitted the participant on the carrier session named here.
    AdmittedSession,
}

/// Participant facts attached to a release petition.
///
/// The public constructor creates an explicitly unverified claim. Only
/// Castellan's admitted-session adapter can mark facts as admission-derived.
#[derive(Clone, PartialEq, Eq)]
pub struct OtpReleaseParticipantClaim {
    principal: String,
    session_binding: String,
    proof: OtpReleaseParticipantProof,
}

impl fmt::Debug for OtpReleaseParticipantClaim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OtpReleaseParticipantClaim")
            .field("principal", &self.principal)
            .field("session_binding", &"<redacted>")
            .field("proof", &self.proof)
            .finish()
    }
}

impl OtpReleaseParticipantClaim {
    /// Validate an unverified carrier-supplied participant and session claim.
    ///
    /// Callers must not treat successful construction as proof of either value.
    pub fn unverified(
        principal: impl Into<String>,
        session_binding: impl Into<String>,
    ) -> Result<Self, OtpReleaseError> {
        let principal = validate_participant_fact(principal.into())?;
        let session_binding = validate_participant_fact(session_binding.into())?;
        Ok(Self {
            principal,
            session_binding,
            proof: OtpReleaseParticipantProof::Unverified,
        })
    }

    pub(super) fn admitted(principal: String, session_binding: String) -> Self {
        Self {
            principal,
            session_binding,
            proof: OtpReleaseParticipantProof::AdmittedSession,
        }
    }

    /// Stable identity claimed by the carrier for the participant.
    pub fn principal(&self) -> &str {
        &self.principal
    }

    /// Opaque binding associating the petition with one carrier session.
    pub fn session_binding(&self) -> &str {
        &self.session_binding
    }

    /// Whether these facts are merely claimed or came from Notochord admission.
    pub fn proof(&self) -> OtpReleaseParticipantProof {
        self.proof
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
