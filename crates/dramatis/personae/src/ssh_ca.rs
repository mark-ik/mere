//! The fleet's SSH certificate authority, projected from delegation.
//!
//! [`crate::ssh_slot`] serves keys; this serves *authority*. A key in an
//! `authorized_keys` file is a bilateral fact — one key, one account, one
//! machine, enrolled by hand. An OpenSSH certificate is a statement the
//! holder carries: any host that trusts the CA accepts it, so N machines
//! cost N enrollments rather than N².
//!
//! Nothing here is a second delegation model. A certificate is *minted
//! from* a [`SignedDelegationCertificate`] and carries its
//! [`DelegationId`] as the OpenSSH key id, so an SSH login traces back to
//! the grant that authorized it and a revocation of that grant names the
//! certificate without a second ledger. Per the 2026-08-11 reconciliation
//! ruling, the grammar in [`crate::delegation`] stays the only one.
//!
//! The CA key is derived from the master through the ordinary
//! protocol-salt ceremony and attested by it, so the master itself signs
//! no certificate and appears in no `sshd_config`.
//!
//! Feature `ssh`.

use ssh_key::certificate::{Builder, CertType, Certificate};
use ssh_key::private::{Ed25519Keypair as SshEd25519Keypair, PrivateKey};
use ssh_key::public::PublicKey;
use ssh_key::{Fingerprint, HashAlg};

use crate::carry::{
    ACTION_SSH_AGENT_FORWARD, ACTION_SSH_LOGIN, ACTION_SSH_PORT_FORWARD, ACTION_SSH_PTY,
    DEVICE_AUTHORITY_DOMAIN,
};
use crate::delegation::{DelegationId, SignedDelegationCertificate};
use crate::{DerivedKeyAttestation, IdentityError, IdentityProvider};

/// The `mod_id` under which the CA's own key would be stored.
pub const SSH_CA_MOD_ID: &str = "ssh-ca";

/// Domain separator for the CA key's derivation salt.
const SSH_CA_SALT_DOMAIN: &[u8] = b"personae/ssh-certificate-authority/v1";

/// Longest life any minted certificate may have, in milliseconds.
///
/// Twelve hours: long enough that one unlock covers a working day, short
/// enough that a revocation nobody could deliver still expires on its own.
/// A grant that ends sooner still ends sooner — this is a ceiling, not a
/// default.
pub const MAX_CERT_TTL_MS: u64 = 12 * 60 * 60 * 1000;

/// Deterministic derivation salt for the certificate authority key.
pub fn ssh_ca_salt() -> Vec<u8> {
    SSH_CA_SALT_DOMAIN.to_vec()
}

/// Failure while minting a certificate.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CertMintError {
    /// The grant's signature, attestation, or issuer binding did not verify.
    #[error("the delegation grant does not verify")]
    UnverifiedGrant,
    /// The grant belongs to some other application's action vocabulary.
    #[error("expected a {DEVICE_AUTHORITY_DOMAIN} grant, found domain {0:?}")]
    WrongDomain(String),
    /// The grant does not carry [`ACTION_SSH_LOGIN`].
    #[error("the grant does not carry the {ACTION_SSH_LOGIN} action")]
    NoLoginAction,
    /// The grant is expired, or not yet valid, at the stated time.
    #[error("the grant is not valid at {at_ms}ms")]
    OutsideGrantWindow {
        /// The evaluation time that fell outside the grant's window.
        at_ms: u64,
    },
    /// A certificate with no principals is an OpenSSH "golden ticket".
    #[error("refusing to mint a certificate with no principals")]
    NoPrincipals,
    /// The device holding this grant has been revoked.
    #[error("device serial {serial} is revoked; this vault will not certify it")]
    RevokedDevice {
        /// The revoked device's certificate serial.
        serial: u64,
    },
    /// The CA key could not be derived from the identity provider.
    #[error("could not derive the certificate authority key")]
    Identity,
    /// `ssh-key` refused to build or sign the certificate.
    #[error("openssh certificate encoding: {0}")]
    Encoding(String),
}

/// What a user certificate should say, beyond what its grant already says.
///
/// The grant fixes the window, the extensions, and the audit trail; this
/// fixes who the holder may log in *as*. Principals come from the persona
/// face rather than the grant because a capability is not a username.
pub struct UserCertRequest<'a> {
    /// The authority being projected.
    pub grant: &'a SignedDelegationCertificate,
    /// The SSH public key the holder will present.
    pub subject: &'a PublicKey,
    /// Unix account names this certificate may authenticate as.
    pub principals: Vec<String>,
    /// A command to force in place of whatever the client asks for.
    pub force_command: Option<String>,
    /// Addresses the certificate is usable from, in OpenSSH's CIDR list form.
    pub source_address: Option<String>,
    /// What this vault has stopped certifying.
    ///
    /// Required rather than optional so that no caller can mint without
    /// having considered revocation: pass an empty ledger to mean "nothing
    /// is revoked", and say so deliberately.
    pub ledger: &'a crate::ssh_krl::RevocationLedger,
}

/// What a host certificate should say.
///
/// Host certificates are what retire `known_hosts` TOFU: one
/// `@cert-authority` line replaces every per-host key a client would
/// otherwise learn by being asked to trust it on first sight.
pub struct HostCertRequest<'a> {
    /// The host's own public key.
    pub subject: &'a PublicKey,
    /// Hostnames this certificate is valid for.
    pub principals: Vec<String>,
    /// How long the host certificate lives, in milliseconds.
    pub valid_for_ms: u64,
}

/// The certificate authority derived from a persona master identity.
pub struct SshCertAuthority {
    private: PrivateKey,
    attestation: DerivedKeyAttestation,
}

impl SshCertAuthority {
    /// Derive the CA from an identity provider.
    ///
    /// The seed crosses from [`crate::Ed25519Keypair`] into `ssh-key`'s own
    /// Ed25519 type, which is what [`crate::Ed25519Keypair::to_seed`] exists
    /// for; the master's own seed never leaves the provider.
    pub fn derive<P: IdentityProvider>(provider: &P) -> Result<Self, IdentityError> {
        let salt = ssh_ca_salt();
        let attestation = provider.attest_derived_key(&salt)?;
        let derived = provider.derive_keypair(&salt)?;
        let mut seed = derived.to_seed();
        let keypair = SshEd25519Keypair::from_seed(&seed);
        seed.fill(0);
        let mut private = PrivateKey::from(keypair);
        private.set_comment("personae ssh certificate authority");
        Ok(Self {
            private,
            attestation,
        })
    }

    /// The CA public key, as `sshd` and clients need it.
    pub fn public_key(&self) -> PublicKey {
        PublicKey::from(&self.private)
    }

    /// SHA256 fingerprint, in the `SHA256:...` form ssh tools print.
    pub fn fingerprint(&self) -> Fingerprint {
        self.public_key().fingerprint(HashAlg::Sha256)
    }

    /// The master-signed proof that this CA key is the master's.
    ///
    /// Verifiable by anyone holding the master public key, which is what
    /// lets a host accept a CA line as authentically the persona's without
    /// meeting the master key itself.
    pub fn attestation(&self) -> &DerivedKeyAttestation {
        &self.attestation
    }

    /// One line for the file `sshd_config`'s `TrustedUserCAKeys` names.
    pub fn trusted_user_ca_line(&self) -> Result<String, CertMintError> {
        self.public_key()
            .to_openssh()
            .map_err(|err| CertMintError::Encoding(err.to_string()))
    }

    /// One `known_hosts` line replacing per-host TOFU for `patterns`.
    ///
    /// `patterns` is OpenSSH's host pattern list, e.g. `*.local,192.168.4.*`.
    pub fn known_hosts_ca_line(&self, patterns: &str) -> Result<String, CertMintError> {
        Ok(format!(
            "@cert-authority {patterns} {}",
            self.trusted_user_ca_line()?
        ))
    }

    /// Mint a user certificate projecting one grant.
    ///
    /// The certificate's window is the grant's window intersected with
    /// `[now, now + MAX_CERT_TTL_MS]`, so a certificate can never outlive
    /// the authority it projects.
    pub fn mint_user_cert(
        &self,
        request: &UserCertRequest<'_>,
        now_ms: u64,
    ) -> Result<Certificate, CertMintError> {
        let grant = check_grant(request.grant, now_ms)?;
        if request.principals.is_empty() {
            return Err(CertMintError::NoPrincipals);
        }
        // The first line of revocation: a revoked device is simply never
        // certified again, so its reach ends when its last certificate
        // expires -- without contacting a single host.
        let serial = device_serial(&request.grant.certificate.scope.resource);
        if request.ledger.is_revoked(serial) {
            return Err(CertMintError::RevokedDevice { serial });
        }

        let (valid_after, valid_before) = user_cert_window(request.grant, now_ms);
        let mut builder = Builder::new(
            nonce(&grant, now_ms),
            request.subject.key_data().clone(),
            valid_after,
            valid_before,
        )
        .map_err(encoding)?;
        builder.cert_type(CertType::User).map_err(encoding)?;
        // The key id is the audit trail: OpenSSH logs it on every accepted
        // login, so it carries the grant id that authorized this one.
        builder.key_id(key_id_for(&grant)).map_err(encoding)?;
        // The serial is the *revocation* handle, and it has to be the
        // device rather than the grant. A self-grant is re-issued on every
        // mint, so its id is new each time and revoking one id would close
        // exactly one certificate while the next mint walked around it. A
        // serial derived from the device is stable, so a single KRL
        // `serial:` line retires every certificate that machine ever
        // carried — the outstanding ones and any minted later.
        builder.serial(serial).map_err(encoding)?;
        builder.comment("personae").map_err(encoding)?;
        for principal in &request.principals {
            builder.valid_principal(principal).map_err(encoding)?;
        }

        // Extensions are positive permissions in OpenSSH, so an action the
        // grant omits is simply never written here.
        let actions = &request.grant.certificate.scope.actions;
        if actions.contains(ACTION_SSH_PTY) {
            builder.extension("permit-pty", "").map_err(encoding)?;
        }
        if actions.contains(ACTION_SSH_AGENT_FORWARD) {
            builder
                .extension("permit-agent-forwarding", "")
                .map_err(encoding)?;
        }
        if actions.contains(ACTION_SSH_PORT_FORWARD) {
            builder
                .extension("permit-port-forwarding", "")
                .map_err(encoding)?;
        }

        if let Some(command) = &request.force_command {
            builder
                .critical_option("force-command", command.as_str())
                .map_err(encoding)?;
        }
        if let Some(addresses) = &request.source_address {
            builder
                .critical_option("source-address", addresses.as_str())
                .map_err(encoding)?;
        }

        builder.sign(&self.private).map_err(encoding)
    }

    /// Mint a host certificate, so clients need no `known_hosts` entry.
    pub fn mint_host_cert(
        &self,
        request: &HostCertRequest<'_>,
        now_ms: u64,
    ) -> Result<Certificate, CertMintError> {
        if request.principals.is_empty() {
            return Err(CertMintError::NoPrincipals);
        }
        let valid_after = now_ms / 1000;
        let valid_before = (now_ms.saturating_add(request.valid_for_ms)) / 1000;
        let mut builder = Builder::new(
            nonce_bytes(now_ms, request.principals.len() as u64),
            request.subject.key_data().clone(),
            valid_after,
            valid_before,
        )
        .map_err(encoding)?;
        builder.cert_type(CertType::Host).map_err(encoding)?;
        builder
            .key_id(format!("personae-host-{}", request.principals[0]))
            .map_err(encoding)?;
        for principal in &request.principals {
            builder.valid_principal(principal).map_err(encoding)?;
        }
        builder.sign(&self.private).map_err(encoding)
    }
}

/// Issue the self-grant a persona holds over its own device.
///
/// The device is the machine *holding* the credential, not the one being
/// logged into, and that is the load-bearing choice. An OpenSSH certificate
/// has no destination field: any host trusting the CA and listing the
/// principal accepts it, so scoping the grant to a target would promise a
/// restriction the format cannot keep. Scoping it to the holder keeps the
/// promise revocation actually needs — a stolen laptop's authority dies
/// everywhere at once, by revoking one grant.
///
/// Restricting *which* hosts a face reaches is a principals question, not a
/// grant question: enroll each host for the principals it should admit
/// (see [`crate::enroll::user_trust_line`]).
pub fn self_grant<P: IdentityProvider>(
    provider: &P,
    device: crate::carry::DeviceId,
    actions: &[&str],
    valid_for_ms: u64,
    now_ms: u64,
) -> Result<SignedDelegationCertificate, crate::delegation::DelegationError> {
    // The construction lives in `carry::grant`, which the RemoteAuth device
    // grant shares: the two modes differ only in the certificate's subject.
    // Keeping one constructor is what keeps that difference legible.
    crate::carry::issue_self_grant(provider, device, actions, valid_for_ms, now_ms)
}

/// The OpenSSH certificate serial standing for one device.
///
/// Derived from the device's own bytes so every certificate that device
/// carries shares it, and non-zero because OpenSSH treats serial zero as
/// "no serial" and refuses to revoke it.
pub fn device_serial(device_resource: &[u8]) -> u64 {
    let digest = blake3::hash(device_resource);
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(bytes).max(1)
}

/// The serial standing for a [`crate::carry::DeviceId`].
pub fn serial_for_device(device: crate::carry::DeviceId) -> u64 {
    device_serial(device.as_uuid().as_bytes())
}

/// The OpenSSH key id carried by a certificate minted from `grant`.
///
/// Hex of the delegation id. [`crate::ssh_krl`] renders revocations against
/// exactly this string.
pub fn key_id_for(grant: &DelegationId) -> String {
    let mut out = String::with_capacity(2 * grant.0.len());
    for byte in grant.0 {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Verify a grant is a usable SSH authority at `now_ms`, returning its id.
fn check_grant(
    grant: &SignedDelegationCertificate,
    now_ms: u64,
) -> Result<DelegationId, CertMintError> {
    if !grant.verify() {
        return Err(CertMintError::UnverifiedGrant);
    }
    let certificate = &grant.certificate;
    if certificate.scope.domain != DEVICE_AUTHORITY_DOMAIN {
        return Err(CertMintError::WrongDomain(certificate.scope.domain.clone()));
    }
    if !certificate.scope.actions.contains(ACTION_SSH_LOGIN) {
        return Err(CertMintError::NoLoginAction);
    }
    // `covers` is the grammar's own time and path check; the path is the
    // device scope's fixed leaf, which is the only path a device grant has.
    if !certificate.covers(&certificate.scope.path_prefix, ACTION_SSH_LOGIN, now_ms) {
        return Err(CertMintError::OutsideGrantWindow { at_ms: now_ms });
    }
    Ok(certificate.id())
}

/// The certificate's validity window in Unix seconds.
fn user_cert_window(grant: &SignedDelegationCertificate, now_ms: u64) -> (u64, u64) {
    let ceiling = now_ms.saturating_add(MAX_CERT_TTL_MS);
    let end_ms = match grant.certificate.expires_at_ms {
        Some(expires) => expires.min(ceiling),
        None => ceiling,
    };
    let start_ms = now_ms.max(grant.certificate.not_before_ms);
    (start_ms / 1000, end_ms / 1000)
}

fn nonce(grant: &DelegationId, now_ms: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(24);
    bytes.extend_from_slice(&grant.0[..16]);
    bytes.extend_from_slice(&now_ms.to_le_bytes());
    bytes
}

fn nonce_bytes(now_ms: u64, salt: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16);
    bytes.extend_from_slice(&now_ms.to_le_bytes());
    bytes.extend_from_slice(&salt.to_le_bytes());
    bytes
}

fn encoding(err: ssh_key::Error) -> CertMintError {
    CertMintError::Encoding(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryProvider;
    use crate::carry::{DeviceId, device_capability_scope};
    use crate::delegation::{DelegationCertificate, DelegationParent};
    use ssh_key::Algorithm;

    const NOW_MS: u64 = 1_760_000_000_000;

    fn device() -> DeviceId {
        DeviceId::from_uuid(uuid::Uuid::from_u128(0x2026_0812))
    }

    fn subject_key() -> PrivateKey {
        PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap()
    }

    fn grant_with(
        provider: &InMemoryProvider,
        actions: &[&str],
        expires_at_ms: Option<u64>,
    ) -> SignedDelegationCertificate {
        let master = provider.master_public_key().to_bytes();
        SignedDelegationCertificate::issue(
            provider,
            DelegationCertificate::new(
                DelegationParent::Root([7; 32]),
                master,
                master,
                device_capability_scope(device(), actions.iter().copied()),
                NOW_MS - 2000,
                NOW_MS - 1000,
                expires_at_ms,
                0,
                [4; 32],
            ),
        )
        .unwrap()
    }

    fn request<'a>(
        grant: &'a SignedDelegationCertificate,
        subject: &'a PublicKey,
        ledger: &'a crate::ssh_krl::RevocationLedger,
    ) -> UserCertRequest<'a> {
        UserCertRequest {
            grant,
            subject,
            principals: vec!["markik".into()],
            force_command: None,
            source_address: None,
            ledger,
        }
    }

    fn clean() -> crate::ssh_krl::RevocationLedger {
        crate::ssh_krl::RevocationLedger::new()
    }

    /// The plan's first validation: `ssh-key` 0.6 can mint and verify an
    /// Ed25519 certificate, so the projection needs no new dependency.
    #[test]
    fn a_minted_certificate_validates_against_the_ca() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let ca = SshCertAuthority::derive(&provider).unwrap();
        let grant = grant_with(&provider, &[ACTION_SSH_LOGIN, ACTION_SSH_PTY], None);
        let subject = subject_key();
        let cert = ca
            .mint_user_cert(
                &request(&grant, &PublicKey::from(&subject), &clean()),
                NOW_MS,
            )
            .unwrap();

        assert!(cert.validate_at(NOW_MS / 1000, [&ca.fingerprint()]).is_ok());
        assert_eq!(cert.cert_type(), CertType::User);
        // A different CA must not validate it.
        let other = SshCertAuthority::derive(&InMemoryProvider::from_seed([9; 32])).unwrap();
        assert!(
            cert.validate_at(NOW_MS / 1000, [&other.fingerprint()])
                .is_err()
        );
    }

    /// The CA is a derived key like any other: same master, same CA.
    #[test]
    fn the_ca_key_is_deterministic_and_master_attested() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let first = SshCertAuthority::derive(&provider).unwrap();
        let second = SshCertAuthority::derive(&provider).unwrap();
        assert_eq!(first.fingerprint(), second.fingerprint());
        assert!(first.attestation().verify(&ssh_ca_salt()));
        assert_eq!(
            first.attestation().master_public_key().unwrap().to_bytes(),
            provider.master_public_key().to_bytes()
        );

        let other = SshCertAuthority::derive(&InMemoryProvider::from_seed([2; 32])).unwrap();
        assert_ne!(first.fingerprint(), other.fingerprint());
    }

    /// Actions the grant omits become extensions the certificate lacks.
    #[test]
    fn extensions_are_exactly_the_actions_the_grant_carries() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let ca = SshCertAuthority::derive(&provider).unwrap();
        let subject = PublicKey::from(&subject_key());

        let full = grant_with(
            &provider,
            &[
                ACTION_SSH_LOGIN,
                ACTION_SSH_PTY,
                ACTION_SSH_AGENT_FORWARD,
                ACTION_SSH_PORT_FORWARD,
            ],
            None,
        );
        let cert = ca
            .mint_user_cert(&request(&full, &subject, &clean()), NOW_MS)
            .unwrap();
        assert_eq!(cert.extensions().len(), 3);
        assert!(cert.extensions().contains_key("permit-pty"));

        let burner = grant_with(&provider, &[ACTION_SSH_LOGIN], None);
        let cert = ca
            .mint_user_cert(&request(&burner, &subject, &clean()), NOW_MS)
            .unwrap();
        assert!(
            cert.extensions().is_empty(),
            "a login-only grant permits nothing beyond the login"
        );
    }

    /// A certificate must never outlive the authority it projects.
    #[test]
    fn the_window_is_the_grant_intersected_with_the_ttl_ceiling() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let ca = SshCertAuthority::derive(&provider).unwrap();
        let subject = PublicKey::from(&subject_key());

        let short = grant_with(&provider, &[ACTION_SSH_LOGIN], Some(NOW_MS + 60_000));
        let cert = ca
            .mint_user_cert(&request(&short, &subject, &clean()), NOW_MS)
            .unwrap();
        assert_eq!(cert.valid_before(), (NOW_MS + 60_000) / 1000);

        let endless = grant_with(&provider, &[ACTION_SSH_LOGIN], None);
        let cert = ca
            .mint_user_cert(&request(&endless, &subject, &clean()), NOW_MS)
            .unwrap();
        assert_eq!(cert.valid_before(), (NOW_MS + MAX_CERT_TTL_MS) / 1000);
    }

    /// Every refusal the mint owes its caller, and the reason it owes it.
    #[test]
    fn minting_refuses_what_it_cannot_honestly_project() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let ca = SshCertAuthority::derive(&provider).unwrap();
        let subject = PublicKey::from(&subject_key());

        // No ssh.login: the grant is about some other device capability.
        let no_login = grant_with(&provider, &[crate::carry::ACTION_PRIVATE_READ], None);
        assert_eq!(
            ca.mint_user_cert(&request(&no_login, &subject, &clean()), NOW_MS),
            Err(CertMintError::NoLoginAction)
        );

        // Expired.
        let expired = grant_with(&provider, &[ACTION_SSH_LOGIN], Some(NOW_MS - 1));
        assert_eq!(
            ca.mint_user_cert(&request(&expired, &subject, &clean()), NOW_MS),
            Err(CertMintError::OutsideGrantWindow { at_ms: NOW_MS })
        );

        // Tampered: verify() fails, so the projection never begins.
        let mut tampered = grant_with(&provider, &[ACTION_SSH_LOGIN], None);
        tampered
            .certificate
            .scope
            .actions
            .insert(ACTION_SSH_PTY.into());
        assert_eq!(
            ca.mint_user_cert(&request(&tampered, &subject, &clean()), NOW_MS),
            Err(CertMintError::UnverifiedGrant)
        );

        // No principals is OpenSSH's "golden ticket": valid everywhere.
        let grant = grant_with(&provider, &[ACTION_SSH_LOGIN], None);
        let empty = clean();
        let mut bare = request(&grant, &subject, &empty);
        bare.principals.clear();
        assert_eq!(
            ca.mint_user_cert(&bare, NOW_MS),
            Err(CertMintError::NoPrincipals)
        );

        // A grant from another application's vocabulary.
        let mut foreign_cert = grant.certificate.clone();
        foreign_cert.scope.domain = "moot".into();
        let foreign = SignedDelegationCertificate::issue(&provider, foreign_cert).unwrap();
        assert_eq!(
            ca.mint_user_cert(&request(&foreign, &subject, &clean()), NOW_MS),
            Err(CertMintError::WrongDomain("moot".into()))
        );
    }

    /// The key id is the grant id, which is what makes revocation possible
    /// without a second ledger.
    #[test]
    fn the_key_id_is_the_grant_id() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let ca = SshCertAuthority::derive(&provider).unwrap();
        let grant = grant_with(&provider, &[ACTION_SSH_LOGIN], None);
        let cert = ca
            .mint_user_cert(
                &request(&grant, &PublicKey::from(&subject_key()), &clean()),
                NOW_MS,
            )
            .unwrap();
        assert_eq!(cert.key_id(), key_id_for(&grant.certificate.id()));
        assert_eq!(cert.key_id().len(), 64);
    }

    #[test]
    fn a_host_certificate_names_its_hostnames() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let ca = SshCertAuthority::derive(&provider).unwrap();
        let host = PublicKey::from(&subject_key());
        let cert = ca
            .mint_host_cert(
                &HostCertRequest {
                    subject: &host,
                    principals: vec!["q-pc.local".into(), "192.168.4.105".into()],
                    valid_for_ms: 365 * 24 * 60 * 60 * 1000,
                },
                NOW_MS,
            )
            .unwrap();
        assert_eq!(cert.cert_type(), CertType::Host);
        assert!(cert.validate_at(NOW_MS / 1000, [&ca.fingerprint()]).is_ok());
        assert!(
            ca.known_hosts_ca_line("*.local")
                .unwrap()
                .starts_with("@cert-authority *.local ssh-ed25519 ")
        );
    }
}
