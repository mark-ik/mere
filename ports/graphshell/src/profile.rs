//! The Graphshell application's identity: a user-selected Personae profile
//! loaded from the shared vault.
//!
//! Section 3 puts this here and nowhere else. `graphshell-client` stays
//! transport-independent and takes **no** Personae dependency — a client that
//! carried an identity would be a client that could only run where that
//! identity lives, which is the opposite of the portable boundary. The
//! *application* composes Personae; the protocol crates never see it.
//!
//! ## Fail closed, unlike Turnstone
//!
//! Turnstone's equivalent falls back to an unsealed profile seed when no vault
//! backend exists, because a browser refusing to start over a key store is
//! worse than one that says plainly what protects its key. **Graphshell must
//! not copy that.** It serves sessions to remote peers who pin the key they
//! reached; inventing one because the vault would not open means a peer's
//! pinned identity silently changes, which is indistinguishable from an
//! impostor. So a vault that will not open is an error here, not a fallback.
//!
//! ## Selection
//!
//! The profile is configurable and defaults to `default` — the same profile
//! the Personae SSH agent serves, so Graphshell speaks as the user rather than
//! as a second identity the user did not know they had.

use std::path::{Path, PathBuf};

use personae::bootstrap::{self, Unlock};
use personae::vault::{IdentityStorage, IdentityVault, ProfileId};
use personae::{
    DerivedKeyAttestation, Ed25519Keypair, Ed25519PublicKey, IdentityError, IdentityProvider,
};

/// Environment override for the profile Graphshell speaks as.
pub const PROFILE_ENV: &str = "GRAPHSHELL_PROFILE";

/// The default profile: the one the Personae bins and SSH agent use.
pub const DEFAULT_PROFILE: &str = "default";

/// The profile this process should load — `GRAPHSHELL_PROFILE`, else
/// [`DEFAULT_PROFILE`].
pub fn selected_profile() -> ProfileId {
    ProfileId(
        std::env::var(PROFILE_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_PROFILE.to_string()),
    )
}

/// The shared Personae vault directory.
pub fn default_vault_dir() -> PathBuf {
    bootstrap::default_vault_dir()
}

/// A loaded Graphshell identity.
pub struct GraphshellIdentity {
    vault: IdentityVault<Box<dyn IdentityStorage>>,
    profile: ProfileId,
    description: String,
}

impl GraphshellIdentity {
    /// Load `profile` from the vault at `vault_dir`.
    ///
    /// Errors rather than inventing an identity; see the module note.
    pub fn load(vault_dir: &Path, profile: &ProfileId) -> Result<Self, IdentityError> {
        let opened = bootstrap::open_storage(vault_dir, Unlock::from_env())?;
        let (loaded, created) = bootstrap::load_or_create_profile(&*opened.storage, profile)?;
        if created {
            // Worth saying out loud: a peer that pinned a previous key will
            // not recognise this one.
            eprintln!(
                "graphshell: minted a new Personae profile `{}` in {}",
                profile.0,
                vault_dir.display()
            );
        }
        Ok(Self {
            vault: IdentityVault::with_profile(opened.storage, loaded),
            profile: profile.clone(),
            description: opened.description,
        })
    }

    /// Load [`selected_profile`] from the shared vault.
    pub fn load_selected() -> Result<Self, IdentityError> {
        Self::load(&default_vault_dir(), &selected_profile())
    }

    /// The profile this identity speaks as.
    pub fn profile(&self) -> &ProfileId {
        &self.profile
    }

    /// Personae's own account of what protects the key at rest — shown, never
    /// guessed, so a user is never left inferring whether their key is sealed.
    pub fn protection(&self) -> &str {
        &self.description
    }

    /// The durable identity a remote peer pins.
    pub fn master_public_key(&self) -> Ed25519PublicKey {
        IdentityProvider::master_public_key(&self.vault)
    }

    /// The derivation salt for one session's endpoint key.
    pub fn session_salt(session: &str) -> Vec<u8> {
        format!("graphshell/endpoint-session/{session}").into_bytes()
    }

    /// A per-session endpoint keypair, derived from the profile identity.
    ///
    /// The same shape Turnstone's projection endpoint uses: a session acts under
    /// its own key rather than the master, so a compromised session key is not
    /// the profile.
    pub fn session_key(&self, session: &str) -> Result<Ed25519Keypair, IdentityError> {
        self.vault.derive_keypair(&Self::session_salt(session))
    }

    /// **The endpoint-key proof.** A master-signed attestation that this
    /// session's derived key belongs to this profile.
    ///
    /// This is what replaces the `endpoint_subject` field deleted in G5a.1. A
    /// key asserted in one's own frame proves nothing; an attestation is
    /// checkable against the master identity the carrier authenticated, which
    /// is what lets a client believe that the session key it is talking to and
    /// the peer its transport proved are the same party.
    pub fn attest_session_key(
        &self,
        session: &str,
    ) -> Result<DerivedKeyAttestation, IdentityError> {
        self.vault.attest_derived_key(&Self::session_salt(session))
    }
}

impl IdentityProvider for GraphshellIdentity {
    fn master_public_key(&self) -> Ed25519PublicKey {
        IdentityProvider::master_public_key(&self.vault)
    }

    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError> {
        self.vault.derive_keypair(salt)
    }

    fn attest_derived_key(&self, salt: &[u8]) -> Result<DerivedKeyAttestation, IdentityError> {
        self.vault.attest_derived_key(salt)
    }
}

/// Verify an endpoint-key proof: that `attestation` binds a session key to
/// `expected_master` for `session`.
///
/// `expected_master` must come from what the **carrier authenticated**, never
/// from the same frame that carried the attestation — otherwise an impostor
/// supplies both halves and they agree with each other.
pub fn verify_session_key(
    attestation: &DerivedKeyAttestation,
    expected_master: Ed25519PublicKey,
    session: &str,
) -> Option<Ed25519PublicKey> {
    if !attestation.verify(&GraphshellIdentity::session_salt(session)) {
        return None;
    }
    let master = attestation.master_public_key().ok()?;
    if master.to_bytes() != expected_master.to_bytes() {
        return None;
    }
    attestation.derived_public_key().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("graphshell-profile-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_selected_profile_defaults_to_the_shared_one() {
        // Graphshell speaks as the user's own profile — the one the SSH agent
        // serves — not as a second identity minted behind their back.
        assert_eq!(selected_profile().0, DEFAULT_PROFILE);
    }

    #[test]
    fn a_session_key_is_derived_stably_and_its_proof_verifies() {
        let dir = scratch("attest");
        let loaded = GraphshellIdentity::load(&dir, &ProfileId("test".into()));
        // Where a sealed backend exists (DPAPI on Windows), this test must
        // actually run: a silent skip would let "the proof verifies" be
        // reported with no evidence behind it.
        #[cfg(windows)]
        let identity = loaded.expect("the Windows vault opens, so this proves the attestation");
        #[cfg(not(windows))]
        let Ok(identity) = loaded else {
            // No sealed backend and no passphrase: the fail-closed path, which
            // has its own test below.
            eprintln!("skipped: no sealed vault backend on this platform");
            return;
        };

        let first = identity.session_key("s1").unwrap();
        let again = identity.session_key("s1").unwrap();
        let other = identity.session_key("s2").unwrap();
        assert_eq!(
            first.public_key().to_bytes(),
            again.public_key().to_bytes(),
            "the same session derives the same key"
        );
        assert_ne!(
            first.public_key().to_bytes(),
            other.public_key().to_bytes(),
            "a different session is a different key"
        );

        // The proof binds that key to this profile's master.
        let attestation = identity.attest_session_key("s1").unwrap();
        let master = identity.master_public_key();
        let proved = verify_session_key(&attestation, master, "s1").expect("the proof verifies");
        assert_eq!(proved.to_bytes(), first.public_key().to_bytes());

        // It does not verify for another session...
        assert!(
            verify_session_key(&attestation, master, "s2").is_none(),
            "a proof is bound to its session"
        );
        // ...nor against a master the carrier did not authenticate.
        let impostor = Ed25519Keypair::from_seed([9; 32]).public_key();
        assert!(
            verify_session_key(&attestation, impostor, "s1").is_none(),
            "a proof is worthless against the wrong identity"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unopenable_vault_is_an_error_not_an_invented_identity() {
        // A vault path under a FILE cannot be created. Turnstone would fall back
        // to an unsealed seed here; Graphshell must not, because a peer pins
        // the key it reached and a silently different one is an impostor.
        let dir = scratch("closed");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("not-a-dir");
        std::fs::write(&file, b"x").unwrap();
        assert!(
            GraphshellIdentity::load(&file.join("vault"), &ProfileId("test".into())).is_err(),
            "no identity is invented when the vault will not open"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
