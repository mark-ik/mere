//! Narrow, expiring RemoteAuth grants for sited Reticulum stations.
//!
//! The underlying `DeviceGrantPayload` remains the wallet carry layer's
//! current signed envelope. This adapter fixes the station policy over that
//! envelope: one derived station signing key, `transport.egress` only, no
//! persona or private-lane authority, no subdelegation, and a mandatory hard
//! expiry. It deliberately does not decide the later reconciliation with
//! `SignedDelegationCertificate`.

use std::fmt;
use std::path::Path;

use personae::{IdentityProvider, InMemoryProvider};
use session_runtime::{
    DeviceExposure, DeviceGrantError, DeviceId, DeviceMode, DevicePublicKey, RemoteAuthGrantSpec,
    SignedDeviceGrant, device_grant_ref, issue_remote_auth_device_grant, load_device_roster,
    load_identity_seed, load_signed_device_grant, verify_device_grant,
};

/// The sole capability a sited station can receive.
pub const TRANSPORT_EGRESS_SCOPE: &str = "transport.egress";
/// The compulsory attenuation for a sited station grant.
pub const NO_SUBDELEGATION_ATTENUATION: &str = "no-subdelegation";

/// Inputs for issuing one sited-station RemoteAuth grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SitedStationGrantRequest {
    device_id: DeviceId,
    station_ed25519_public_key: [u8; 32],
    label: String,
    issued_at_ms: u64,
    expires_at_ms: u64,
}

impl SitedStationGrantRequest {
    /// Create a request whose expiry is strictly after its issuance time.
    pub fn new(
        device_id: DeviceId,
        station_ed25519_public_key: [u8; 32],
        label: impl Into<String>,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self, SitedStationGrantError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(SitedStationGrantError::EmptyLabel);
        }
        if expires_at_ms <= issued_at_ms {
            return Err(SitedStationGrantError::InvalidExpiryWindow {
                issued_at_ms,
                expires_at_ms,
            });
        }
        Ok(Self {
            device_id,
            station_ed25519_public_key,
            label,
            issued_at_ms,
            expires_at_ms,
        })
    }

    fn remote_auth_spec(&self) -> RemoteAuthGrantSpec {
        RemoteAuthGrantSpec {
            device_id: self.device_id,
            delegatee_pubkey: DevicePublicKey(self.station_ed25519_public_key),
            label: self.label.clone(),
            exposure: DeviceExposure::ExposedEgress,
            issued_at_ms: self.issued_at_ms,
            expires_at_ms: Some(self.expires_at_ms),
            personas: Vec::new(),
            scopes: vec![TRANSPORT_EGRESS_SCOPE.into()],
            attenuations: vec![NO_SUBDELEGATION_ATTENUATION.into()],
            wrapped_private_epochs: Vec::new(),
        }
    }
}

/// A signed RemoteAuth envelope that passed the sited-station policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SitedStationGrant {
    grant: SignedDeviceGrant,
}

impl SitedStationGrant {
    /// Issue and persist a narrow grant through the host wallet store.
    ///
    /// `issuer_public_key` must be the Persona public key that derived the
    /// station credential. It is checked against the unlocked wallet root so
    /// a different Persona cannot accidentally commission the station.
    pub fn issue(
        data_root: &Path,
        issuer_public_key: [u8; 32],
        request: SitedStationGrantRequest,
    ) -> Result<Self, SitedStationGrantError> {
        let wallet_seed =
            load_identity_seed(data_root)?.ok_or(SitedStationGrantError::WalletLocked)?;
        let wallet_provider = InMemoryProvider::from_seed(wallet_seed);
        if wallet_provider.master_public_key().to_bytes() != issuer_public_key {
            return Err(SitedStationGrantError::IssuerMismatch);
        }

        if let Some(existing) = load_signed_device_grant(data_root, request.device_id)? {
            let existing = Self::from_signed(existing)?;
            if request.expires_at_ms <= existing.expires_at_ms() {
                return Err(SitedStationGrantError::NonExtendingRenewal {
                    device_id: request.device_id,
                    existing_expires_at_ms: existing.expires_at_ms(),
                    requested_expires_at_ms: request.expires_at_ms,
                });
            }
        }

        let grant = issue_remote_auth_device_grant(data_root, &request.remote_auth_spec())?;
        Self::from_signed(grant)
    }

    /// Check a signed envelope's static station policy and signature.
    pub fn from_signed(grant: SignedDeviceGrant) -> Result<Self, SitedStationGrantError> {
        let station_grant = Self { grant };
        station_grant.validate_policy()?;
        Ok(station_grant)
    }

    /// Load the active grant for a station, including host-side revocation.
    ///
    /// Expiry is fail-closed: `now_ms == expires_at_ms` is already expired.
    pub fn load_active(
        data_root: &Path,
        device_id: DeviceId,
        station_ed25519_public_key: [u8; 32],
        now_ms: u64,
    ) -> Result<Self, SitedStationGrantError> {
        let grant = load_signed_device_grant(data_root, device_id)?
            .ok_or(SitedStationGrantError::MissingGrant { device_id })?;
        let station_grant = Self::from_signed(grant)?;

        let roster = load_device_roster(data_root)?.ok_or(SitedStationGrantError::MissingRoster)?;
        if roster.revoked.contains(&device_id) {
            return Err(SitedStationGrantError::Revoked { device_id });
        }
        let device = roster
            .devices
            .iter()
            .find(|device| device.device_id == device_id)
            .ok_or(SitedStationGrantError::UnknownDevice { device_id })?;
        if device.mode != DeviceMode::RemoteAuth {
            return Err(SitedStationGrantError::NotRemoteAuth { device_id });
        }
        if device.exposure != DeviceExposure::ExposedEgress {
            return Err(SitedStationGrantError::NotExposedEgress { device_id });
        }
        if device.device_pubkey != DevicePublicKey(station_ed25519_public_key) {
            return Err(SitedStationGrantError::StationKeyMismatch { device_id });
        }
        let expected_ref = device_grant_ref(&station_grant.grant)?;
        if device.grant_ref != Some(expected_ref) {
            return Err(SitedStationGrantError::RosterGrantMismatch { device_id });
        }

        station_grant.authorize_at(device_id, station_ed25519_public_key, now_ms)?;
        Ok(station_grant)
    }

    /// Check that this grant is for the expected station and still live.
    ///
    /// This does not consult host storage. Use [`Self::load_active`] when a
    /// host must also reject revoked or stale roster entries.
    pub fn authorize_at(
        &self,
        device_id: DeviceId,
        station_ed25519_public_key: [u8; 32],
        now_ms: u64,
    ) -> Result<(), SitedStationGrantError> {
        self.validate_policy()?;
        if self.grant.payload.device_id != device_id {
            return Err(SitedStationGrantError::DeviceMismatch {
                expected: device_id,
                actual: self.grant.payload.device_id,
            });
        }
        if self.grant.payload.delegatee_pubkey != DevicePublicKey(station_ed25519_public_key) {
            return Err(SitedStationGrantError::StationKeyMismatch { device_id });
        }
        let expires_at_ms = self
            .grant
            .payload
            .expires_at_ms
            .expect("validate_policy requires a station-grant expiry");
        if now_ms >= expires_at_ms {
            return Err(SitedStationGrantError::Expired {
                device_id,
                expires_at_ms,
                now_ms,
            });
        }
        Ok(())
    }

    /// Borrow the signed wallet envelope for transport or durable storage.
    pub fn signed(&self) -> &SignedDeviceGrant {
        &self.grant
    }

    /// The mandatory fail-closed expiry of this grant.
    pub fn expires_at_ms(&self) -> u64 {
        self.grant
            .payload
            .expires_at_ms
            .expect("validate_policy requires a station-grant expiry")
    }

    /// The RemoteAuth device this grant is bound to.
    pub fn device_id(&self) -> DeviceId {
        self.grant.payload.device_id
    }

    /// The Reticulum Ed25519 station key this grant authorizes.
    pub fn station_ed25519_public_key(&self) -> [u8; 32] {
        self.grant.payload.delegatee_pubkey.0
    }

    /// The Persona root that signed this grant.
    pub fn issuer_public_key(&self) -> [u8; 32] {
        self.grant.payload.delegator_pubkey.0
    }

    fn validate_policy(&self) -> Result<(), SitedStationGrantError> {
        if !verify_device_grant(&self.grant)? {
            return Err(SitedStationGrantError::InvalidSignature);
        }
        let payload = &self.grant.payload;
        if payload.personas.len() != 0 {
            return Err(SitedStationGrantError::PersonaAuthority);
        }
        if payload.wrapped_private_epochs.len() != 0 {
            return Err(SitedStationGrantError::PrivateEpochMaterial);
        }
        if payload.scopes.as_slice() != [TRANSPORT_EGRESS_SCOPE] {
            return Err(SitedStationGrantError::ScopeViolation);
        }
        if payload.attenuations.as_slice() != [NO_SUBDELEGATION_ATTENUATION] {
            return Err(SitedStationGrantError::AttenuationViolation);
        }
        let expires_at_ms = payload
            .expires_at_ms
            .ok_or(SitedStationGrantError::MissingExpiry)?;
        if expires_at_ms <= payload.issued_at_ms {
            return Err(SitedStationGrantError::InvalidExpiryWindow {
                issued_at_ms: payload.issued_at_ms,
                expires_at_ms,
            });
        }
        Ok(())
    }
}

/// Why a sited-station grant could not be issued or used.
#[derive(Debug)]
pub enum SitedStationGrantError {
    /// The host wallet store could not be read or written.
    Storage(std::io::Error),
    /// The signed envelope could not be decoded, signed, or content-addressed.
    Envelope(DeviceGrantError),
    /// The commissioning label was blank.
    EmptyLabel,
    /// The expiry did not fall strictly after the issuance time.
    InvalidExpiryWindow {
        /// The grant's issuance time.
        issued_at_ms: u64,
        /// The grant's requested expiry time.
        expires_at_ms: u64,
    },
    /// The host wallet has no unlocked root to issue a grant with.
    WalletLocked,
    /// The credential's Persona did not match the host wallet root.
    IssuerMismatch,
    /// A replacement grant did not extend the existing live window.
    NonExtendingRenewal {
        /// The station whose grant was being replaced.
        device_id: DeviceId,
        /// The existing grant's expiry time.
        existing_expires_at_ms: u64,
        /// The requested replacement expiry time.
        requested_expires_at_ms: u64,
    },
    /// The host store has no signed grant for this device.
    MissingGrant {
        /// The requested device.
        device_id: DeviceId,
    },
    /// The host store has no device roster.
    MissingRoster,
    /// The requested device is absent from the host roster.
    UnknownDevice {
        /// The requested device.
        device_id: DeviceId,
    },
    /// The host has revoked this device.
    Revoked {
        /// The revoked device.
        device_id: DeviceId,
    },
    /// The roster records a different device mode.
    NotRemoteAuth {
        /// The incorrectly configured device.
        device_id: DeviceId,
    },
    /// The roster does not allow the device to act as egress.
    NotExposedEgress {
        /// The incorrectly configured device.
        device_id: DeviceId,
    },
    /// The signed envelope names a different device than expected.
    DeviceMismatch {
        /// The device the caller expected.
        expected: DeviceId,
        /// The device named by the envelope.
        actual: DeviceId,
    },
    /// The envelope or roster names a different station signing key.
    StationKeyMismatch {
        /// The affected device.
        device_id: DeviceId,
    },
    /// The roster does not point at the currently stored signed envelope.
    RosterGrantMismatch {
        /// The affected device.
        device_id: DeviceId,
    },
    /// Signature verification of the signed envelope failed.
    InvalidSignature,
    /// The envelope grants any Persona authority.
    PersonaAuthority,
    /// The envelope carries private-lane epoch material.
    PrivateEpochMaterial,
    /// The envelope grants anything other than `transport.egress`.
    ScopeViolation,
    /// The envelope permits any attenuation other than `no-subdelegation`.
    AttenuationViolation,
    /// The envelope has no expiry.
    MissingExpiry,
    /// The envelope's expiry has passed or arrived.
    Expired {
        /// The expired device.
        device_id: DeviceId,
        /// The strict expiry instant.
        expires_at_ms: u64,
        /// The instant used for evaluation.
        now_ms: u64,
    },
}

impl From<std::io::Error> for SitedStationGrantError {
    fn from(error: std::io::Error) -> Self {
        Self::Storage(error)
    }
}

impl From<DeviceGrantError> for SitedStationGrantError {
    fn from(error: DeviceGrantError) -> Self {
        Self::Envelope(error)
    }
}

impl fmt::Display for SitedStationGrantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "sited station wallet storage failed: {error}"),
            Self::Envelope(error) => write!(f, "sited station grant envelope failed: {error}"),
            Self::EmptyLabel => f.write_str("a sited station label must not be blank"),
            Self::InvalidExpiryWindow {
                issued_at_ms,
                expires_at_ms,
            } => write!(
                f,
                "sited station expiry {expires_at_ms} must be after issuance {issued_at_ms}"
            ),
            Self::WalletLocked => f.write_str("the host wallet root is locked or absent"),
            Self::IssuerMismatch => f.write_str(
                "the station credential was derived by a different Persona than the host wallet",
            ),
            Self::NonExtendingRenewal {
                device_id,
                existing_expires_at_ms,
                requested_expires_at_ms,
            } => write!(
                f,
                "replacement grant for sited station {} expires at {requested_expires_at_ms}, which does not extend existing expiry {existing_expires_at_ms}",
                device_id.as_uuid()
            ),
            Self::MissingGrant { device_id } => {
                write!(f, "sited station grant missing for {}", device_id.as_uuid())
            }
            Self::MissingRoster => f.write_str("the host device roster is missing"),
            Self::UnknownDevice { device_id } => {
                write!(
                    f,
                    "sited station device {} is absent from the host roster",
                    device_id.as_uuid()
                )
            }
            Self::Revoked { device_id } => {
                write!(f, "sited station device {} is revoked", device_id.as_uuid())
            }
            Self::NotRemoteAuth { device_id } => write!(
                f,
                "sited station device {} is not a RemoteAuth device",
                device_id.as_uuid()
            ),
            Self::NotExposedEgress { device_id } => write!(
                f,
                "sited station device {} is not an exposed egress",
                device_id.as_uuid()
            ),
            Self::DeviceMismatch { expected, actual } => write!(
                f,
                "sited station grant targets {}, not expected device {}",
                actual.as_uuid(),
                expected.as_uuid()
            ),
            Self::StationKeyMismatch { device_id } => write!(
                f,
                "sited station signing key does not match device {}",
                device_id.as_uuid()
            ),
            Self::RosterGrantMismatch { device_id } => write!(
                f,
                "sited station roster reference does not match device {} grant",
                device_id.as_uuid()
            ),
            Self::InvalidSignature => f.write_str("sited station grant signature is invalid"),
            Self::PersonaAuthority => {
                f.write_str("sited station grants must not authorize any persona")
            }
            Self::PrivateEpochMaterial => {
                f.write_str("sited station grants must not carry private epoch material")
            }
            Self::ScopeViolation => {
                f.write_str("sited station grants must contain only the transport.egress scope")
            }
            Self::AttenuationViolation => f.write_str(
                "sited station grants must contain only the no-subdelegation attenuation",
            ),
            Self::MissingExpiry => f.write_str("sited station grants require an expiry"),
            Self::Expired {
                device_id,
                expires_at_ms,
                now_ms,
            } => write!(
                f,
                "sited station device {} expired at {expires_at_ms}; evaluated at {now_ms}",
                device_id.as_uuid()
            ),
        }
    }
}

impl std::error::Error for SitedStationGrantError {}

#[cfg(test)]
mod tests {
    use personae::{IdentityProvider, InMemoryProvider};
    use session_runtime::{DeviceGrantPayload, issue_device_grant};

    use super::*;

    #[test]
    fn a_signed_grant_with_extra_authority_is_not_a_sited_station_grant() {
        let issuer = InMemoryProvider::from_seed([0x51; 32]);
        let station = issuer.derive_keypair(b"sited-station-grant-test").unwrap();
        let device_id = DeviceId::new();
        let mut payload = DeviceGrantPayload::new_remote_auth(
            device_id,
            DevicePublicKey::from(issuer.master_public_key()),
            DevicePublicKey::from(station.public_key()),
            100,
        );
        payload.expires_at_ms = Some(200);
        payload.scopes = vec![TRANSPORT_EGRESS_SCOPE.into(), "identity.act".into()];
        payload.attenuations = vec![NO_SUBDELEGATION_ATTENUATION.into()];

        let signed = issue_device_grant(issuer.master_keypair(), payload).unwrap();
        let error = SitedStationGrant::from_signed(signed).unwrap_err();

        assert!(matches!(error, SitedStationGrantError::ScopeViolation));
    }

    #[test]
    fn a_station_grant_can_only_be_renewed_with_a_later_expiry() {
        let root = tempfile::tempdir().unwrap();
        let persona = personae::PersonaId::new();
        let seed =
            session_runtime::ensure_wallet_state(root.path(), persona, "Station host").unwrap();
        let issuer = InMemoryProvider::from_seed(seed);
        let device_id = DeviceId::new();
        let key = issuer
            .derive_keypair(b"sited-station-renewal-test")
            .unwrap();
        let first = SitedStationGrantRequest::new(
            device_id,
            key.public_key().to_bytes(),
            "Ridge north",
            100,
            200,
        )
        .unwrap();
        SitedStationGrant::issue(root.path(), issuer.master_public_key().to_bytes(), first)
            .unwrap();

        let replacement = SitedStationGrantRequest::new(
            device_id,
            key.public_key().to_bytes(),
            "Ridge north",
            150,
            200,
        )
        .unwrap();
        let error = SitedStationGrant::issue(
            root.path(),
            issuer.master_public_key().to_bytes(),
            replacement,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            SitedStationGrantError::NonExtendingRenewal {
                existing_expires_at_ms: 200,
                requested_expires_at_ms: 200,
                ..
            }
        ));
    }
}
