//! Deterministic retinue identity derivation from a Mere master seed.

use hkdf::Hkdf;
use sha2::Sha256;

use identity::Ed25519Keypair;
use retinue::identity::PrivateIdentity;

/// HKDF-SHA256 context string for the dual-key derivation. Changing this changes
/// every derived retinue identity, so treat it as a wire-format version.
const IDENTITY_HKDF_INFO: &[u8] = b"mere-reticulum-identity-v1";

/// Derive a deterministic retinue identity from a Mere master keypair.
///
/// Reticulum needs a dual-key identity: 64 secret bytes, `x25519_secret(32) ++
/// ed25519_seed(32)`. Mere's master key is a single 32-byte Ed25519 seed, so we
/// stretch it with HKDF-SHA256 into 64 bytes and hand them to
/// [`PrivateIdentity::from_secret_bytes`], which reads them in exactly that split.
///
/// The derivation is pure, so the same master seed always yields the same retinue
/// destination across restarts.
pub(super) fn derive_identity(master: &Ed25519Keypair) -> PrivateIdentity {
    let seed = master.to_seed();
    let hk = Hkdf::<Sha256>::new(None, &seed);
    let mut okm = [0u8; 64];
    hk.expand(IDENTITY_HKDF_INFO, &mut okm)
        .expect("64 bytes is a valid HKDF-SHA256 output length");

    PrivateIdentity::from_secret_bytes(&okm)
}
