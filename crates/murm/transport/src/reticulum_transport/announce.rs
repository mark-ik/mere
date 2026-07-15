//! Announce-based discovery with an authenticated `PeerID` binding.
//!
//! A retinue destination hash cannot be synthesized from a Mere [`PeerID`] (the
//! retinue identity is dual-key and only the Ed25519 half is in a `PeerID`), so a
//! peer's destination must be *learned* from an announce. To stop a node from
//! announcing someone else's `PeerID`, each announce carries an `app_data` blob
//! signed by the Mere master key that binds the `PeerID` to the announcing retinue
//! identity and ALPN destination name.

use identity::{Ed25519Keypair, Ed25519Signature};
use retinue::destination::DestinationName;
use retinue::identity::Identity;

use crate::PeerID;

/// `peer_id(32) ++ master_signature(64)`.
const APP_DATA_LEN: usize = 32 + 64;

/// The message the master key signs to bind a `PeerID` to a retinue identity for
/// one destination name: `x25519_public(32) ++ ed25519_public(32) ++ peer_id(32)
/// ++ name_hash(10)`.
///
/// Binding to the name hash (not just the identity) stops a peer from replaying
/// one ALPN's binding to claim a `PeerID` on a different ALPN.
fn binding_message(peer_id: &PeerID, name: &DestinationName, identity: &Identity) -> Vec<u8> {
    let mut msg = Vec::with_capacity(32 + 32 + 32 + 10);
    msg.extend_from_slice(identity.x25519_bytes());
    msg.extend_from_slice(identity.ed25519_bytes());
    msg.extend_from_slice(&peer_id.to_bytes());
    msg.extend_from_slice(name.name_hash().as_slice());
    msg
}

/// Build the announce `app_data`: `peer_id(32) ++ master_signature(64)`.
pub(super) fn build_app_data(
    peer_id: &PeerID,
    name: &DestinationName,
    identity: &Identity,
    master: &Ed25519Keypair,
) -> Vec<u8> {
    let signature = master.sign(&binding_message(peer_id, name, identity));
    let mut app_data = Vec::with_capacity(APP_DATA_LEN);
    app_data.extend_from_slice(&peer_id.to_bytes());
    app_data.extend_from_slice(&signature.to_bytes());
    app_data
}

/// Recover the `PeerID` an announce binds to this retinue identity + name, or
/// `None` if the `app_data` is malformed or the master signature does not verify.
pub(super) fn recover_peer_id(
    app_data: &[u8],
    name: &DestinationName,
    identity: &Identity,
) -> Option<PeerID> {
    if app_data.len() < APP_DATA_LEN {
        return None;
    }
    let peer_bytes: [u8; 32] = app_data[..32].try_into().ok()?;
    let peer_id = PeerID::from_bytes(&peer_bytes).ok()?;
    let sig_bytes: [u8; 64] = app_data[32..96].try_into().ok()?;
    let signature = Ed25519Signature::from_bytes(&sig_bytes);

    if peer_id
        .public_key()
        .verify(&binding_message(&peer_id, name, identity), &signature)
    {
        Some(peer_id)
    } else {
        None
    }
}
