#![forbid(unsafe_code)]
//! Signalman's Persona-backed Retinue station adapter.
//!
//! This port is the only layer here that knows both Castellan/Personae and
//! Retinue. It derives a station-scoped Reticulum identity, binds its signing
//! key to Castellan's narrow expiring RemoteAuth grant, immediately converts
//! its transient secret bytes into Retinue's typed identity, and supplies that
//! to [`postilion::StationConfig`]. Retinue does not know where the credential
//! came from and never creates a local account file.

use std::path::Path;

use castellan::reticulum::ReticulumStationMaterial;
use castellan::reticulum::grant::{
    SitedStationGrant, SitedStationGrantError, SitedStationGrantRequest,
};
use personae::{IdentityError, IdentityProvider};
use postilion::StationConfig;
use retinue::identity::{Identity, PrivateIdentity};
use session_runtime::DeviceId;
use zeroize::Zeroize;

/// A Reticulum station credential derived for one sited device.
///
/// The underlying identity remains private. Callers can inspect its public
/// form and construct a station configuration, but cannot ask this type to
/// serialize or persist the secret.
#[derive(Clone, Debug)]
pub struct SitedStationCredential {
    identity: PrivateIdentity,
    issuer_public_key: [u8; 32],
}

impl SitedStationCredential {
    fn derive(
        provider: &dyn IdentityProvider,
        station_scope: &[u8],
    ) -> Result<Self, IdentityError> {
        let material = ReticulumStationMaterial::derive(provider, station_scope)?;
        let mut secret = material.secret_bytes();
        let identity = PrivateIdentity::from_secret_bytes(&secret);
        secret.zeroize();
        Ok(Self {
            identity,
            issuer_public_key: provider.master_public_key().to_bytes(),
        })
    }

    /// Derive a station credential from its durable host-side device id.
    ///
    /// The id is persisted only in the host's wallet grant and roster. It is
    /// not a radio location or a device-local account file.
    pub fn derive_for_device(
        provider: &dyn IdentityProvider,
        device_id: DeviceId,
    ) -> Result<Self, IdentityError> {
        Self::derive(provider, device_id.as_uuid().as_bytes())
    }

    /// The public Reticulum identity the station will announce.
    pub fn public_identity(&self) -> &Identity {
        self.identity.public()
    }

    fn station_config(&self, port: impl Into<String>, name: impl Into<String>) -> StationConfig {
        StationConfig::new(port, name, self.identity.clone())
    }

    /// Issue the only grant shape an unattended station can receive.
    ///
    /// The host wallet root must belong to the same Persona that derived this
    /// credential. The resulting grant carries exactly `transport.egress`, no
    /// personas, no private epochs, and a mandatory hard expiry.
    pub fn issue_remote_auth_grant(
        &self,
        data_root: &Path,
        device_id: DeviceId,
        label: impl Into<String>,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<SitedStationGrant, SitedStationGrantError> {
        let request = SitedStationGrantRequest::new(
            device_id,
            *self.public_identity().ed25519_bytes(),
            label,
            issued_at_ms,
            expires_at_ms,
        )?;
        SitedStationGrant::issue(data_root, self.issuer_public_key, request)
    }

    /// Build a station configuration only while the host grant is live.
    ///
    /// This host-side check rejects expired grants at their exact expiry
    /// instant, revoked device ids, stale roster references, and any signing
    /// key that does not match this derived station credential.
    pub fn station_config_from_active_grant(
        &self,
        data_root: &Path,
        device_id: DeviceId,
        now_ms: u64,
        port: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<StationConfig, SitedStationGrantError> {
        SitedStationGrant::load_active(
            data_root,
            device_id,
            *self.public_identity().ed25519_bytes(),
            now_ms,
        )?;
        Ok(self.station_config(port, name))
    }
}

#[cfg(test)]
mod tests {
    use personae::{InMemoryProvider, PersonaId};
    use session_runtime::{DeviceId, ensure_wallet_state, revoke_remote_auth_device};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn one_persona_and_station_scope_keep_the_same_reticulum_address() {
        let provider = InMemoryProvider::from_seed([0x44; 32]);
        let first = SitedStationCredential::derive(&provider, b"device:ridge-north").unwrap();
        let second = SitedStationCredential::derive(&provider, b"device:ridge-north").unwrap();

        assert_eq!(
            first.public_identity().hash(),
            second.public_identity().hash()
        );
    }

    #[test]
    fn station_scopes_cannot_share_a_reticulum_address_accidentally() {
        let provider = InMemoryProvider::from_seed([0x44; 32]);
        let first = SitedStationCredential::derive(&provider, b"device:ridge-north").unwrap();
        let second = SitedStationCredential::derive(&provider, b"device:ridge-south").unwrap();

        assert_ne!(
            first.public_identity().hash(),
            second.public_identity().hash()
        );
    }

    #[test]
    fn station_config_is_a_typed_injection_not_an_identity_path() {
        let provider = InMemoryProvider::from_seed([0x44; 32]);
        let credential = SitedStationCredential::derive(&provider, b"device:ridge-north").unwrap();
        let config = credential.station_config("COM7", "Ridge north");

        assert_eq!(config.port, "COM7");
        assert_eq!(config.name, "Ridge north");
        assert_eq!(
            config.identity.public().hash(),
            credential.public_identity().hash()
        );
    }

    #[test]
    fn commissioned_station_is_egress_only_and_fails_closed_at_expiry_or_revocation() {
        let root = tempdir().unwrap();
        let seed = ensure_wallet_state(root.path(), PersonaId::new(), "Station host").unwrap();
        let provider = InMemoryProvider::from_seed(seed);
        let device_id = DeviceId::new();
        let credential = SitedStationCredential::derive_for_device(&provider, device_id).unwrap();

        let grant = credential
            .issue_remote_auth_grant(root.path(), device_id, "Ridge north", 100, 200)
            .unwrap();
        assert_eq!(grant.signed().payload.personas, Vec::new());
        assert_eq!(grant.signed().payload.scopes, ["transport.egress"]);
        assert_eq!(grant.signed().payload.attenuations, ["no-subdelegation"]);
        assert!(grant.signed().payload.wrapped_private_epochs.is_empty());

        let config = credential
            .station_config_from_active_grant(root.path(), device_id, 199, "COM7", "Ridge north")
            .unwrap();
        assert_eq!(
            config.identity.public().hash(),
            credential.public_identity().hash()
        );

        let expiry = credential
            .station_config_from_active_grant(root.path(), device_id, 200, "COM7", "Ridge north")
            .unwrap_err();
        assert!(matches!(
            expiry,
            SitedStationGrantError::Expired {
                expires_at_ms: 200,
                ..
            }
        ));

        revoke_remote_auth_device(root.path(), device_id).unwrap();
        let revoked = credential
            .station_config_from_active_grant(root.path(), device_id, 199, "COM7", "Ridge north")
            .unwrap_err();
        assert!(matches!(revoked, SitedStationGrantError::Revoked { .. }));
    }

    #[test]
    fn commissioning_refuses_a_credential_from_another_persona() {
        let root = tempdir().unwrap();
        ensure_wallet_state(root.path(), PersonaId::new(), "Station host").unwrap();
        let foreign_provider = InMemoryProvider::from_seed([0x77; 32]);
        let credential =
            SitedStationCredential::derive_for_device(&foreign_provider, DeviceId::new()).unwrap();

        let error = credential
            .issue_remote_auth_grant(root.path(), DeviceId::new(), "Ridge north", 100, 200)
            .unwrap_err();
        assert!(matches!(error, SitedStationGrantError::IssuerMismatch));
    }
}
