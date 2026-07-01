//! Announce-based discovery with an authenticated `PeerID` binding.
//!
//! A Reticulum destination hash cannot be synthesized from a Mere [`PeerID`]
//! (the identity is dual-key and only the Ed25519 half is in a `PeerID`), so a
//! peer's destination must be *learned* from an announce. To stop a node from
//! announcing someone else's `PeerID`, each announce carries an `app_data` blob
//! signed by the Mere master key that binds the `PeerID` to the announcing
//! Reticulum identity and ALPN.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use identity::{Ed25519Keypair, Ed25519Signature};
use reticulum::destination::{DestinationDesc, DestinationName, SingleInputDestination, NAME_HASH_LENGTH};
use reticulum::identity::Identity as ReticulumIdentity;
use reticulum::transport::{AnnounceEvent, Transport as ReticulumStack};
use tokio::sync::{broadcast, Mutex as TokioMutex};

use crate::{Alpn, PeerID};

/// The key an announce is matched on. Announces carry only the 10-byte
/// destination name hash, so ALPN lookup keys on that slice, never the full
/// 32-byte name hash (which is zero-padded on the wire and would not match).
pub(super) type NameHashKey = [u8; NAME_HASH_LENGTH];

const APP_DATA_MIN_LEN: usize = 32 + 64; // PeerID(32) ++ signature(64)

/// Compute the [`NameHashKey`] for a destination name.
pub(super) fn name_hash_key(name: &DestinationName) -> NameHashKey {
    let mut key = [0u8; NAME_HASH_LENGTH];
    key.copy_from_slice(name.as_name_hash_slice());
    key
}

/// The message the master key signs to bind a `PeerID` to a Reticulum identity
/// for one ALPN: `ret_public_key(32) ++ ret_verifying_key(32) ++ peer_id(32) ++
/// name_hash(10)`.
fn binding_message(
    peer_id: &PeerID,
    name: &DestinationName,
    ret_identity: &ReticulumIdentity,
) -> Vec<u8> {
    let mut msg = Vec::with_capacity(32 + 32 + 32 + NAME_HASH_LENGTH);
    msg.extend_from_slice(ret_identity.public_key_bytes());
    msg.extend_from_slice(ret_identity.verifying_key_bytes());
    msg.extend_from_slice(&peer_id.to_bytes());
    msg.extend_from_slice(name.as_name_hash_slice());
    msg
}

/// Build the announce `app_data`: `peer_id(32) ++ master_signature(64)`.
pub(super) fn build_app_data(
    peer_id: &PeerID,
    name: &DestinationName,
    ret_identity: &ReticulumIdentity,
    master: &Ed25519Keypair,
) -> Vec<u8> {
    let signature = master.sign(&binding_message(peer_id, name, ret_identity));
    let mut app_data = Vec::with_capacity(APP_DATA_MIN_LEN);
    app_data.extend_from_slice(&peer_id.to_bytes());
    app_data.extend_from_slice(&signature.to_bytes());
    app_data
}

/// Verify an announce `app_data` against the announced Reticulum identity, and
/// recover the bound `PeerID` if (and only if) the master signature checks out.
fn verify_app_data(
    app_data: &[u8],
    name: &DestinationName,
    ret_identity: &ReticulumIdentity,
) -> Option<PeerID> {
    if app_data.len() < APP_DATA_MIN_LEN {
        return None;
    }
    let peer_bytes: [u8; 32] = app_data[..32].try_into().ok()?;
    let peer_id = PeerID::from_bytes(&peer_bytes).ok()?;
    let sig_bytes: [u8; 64] = app_data[32..96].try_into().ok()?;
    let signature = Ed25519Signature::from_bytes(&sig_bytes);

    if peer_id
        .public_key()
        .verify(&binding_message(&peer_id, name, ret_identity), &signature)
    {
        Some(peer_id)
    } else {
        None
    }
}

/// Background task: drain announces, verify the binding, and record
/// `peer_id -> (alpn, destination)` so [`connect`](super::ReticulumTransport)
/// can resolve a peer.
pub(super) async fn announce_listener(
    mut rx: broadcast::Receiver<AnnounceEvent>,
    name_to_alpn: Arc<HashMap<NameHashKey, Alpn>>,
    peers: Arc<StdMutex<HashMap<PeerID, HashMap<Alpn, DestinationDesc>>>>,
) {
    loop {
        match rx.recv().await {
            Ok(announce) => {
                let (desc, name) = {
                    let dest = announce.destination.lock().await;
                    (dest.desc, dest.desc.name)
                };
                let Some(alpn) = name_to_alpn.get(&name_hash_key(&name)).cloned() else {
                    continue;
                };
                if let Some(peer_id) =
                    verify_app_data(announce.app_data.as_slice(), &name, &desc.identity)
                {
                    peers
                        .lock()
                        .expect("peers mutex poisoned")
                        .entry(peer_id)
                        .or_default()
                        .insert(alpn, desc);
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Background task: periodically re-announce every registered destination with
/// its precomputed, signed `app_data`. The `app_data` is fixed per ALPN, so this
/// never needs the master secret after construction.
pub(super) async fn announce_sender(
    inner: Arc<ReticulumStack>,
    destinations: Arc<StdMutex<HashMap<Alpn, Arc<TokioMutex<SingleInputDestination>>>>>,
    app_data_by_alpn: Arc<HashMap<Alpn, Vec<u8>>>,
    interval: Duration,
) {
    let mut timer = tokio::time::interval(interval);
    loop {
        timer.tick().await;
        let entries: Vec<(Alpn, Arc<TokioMutex<SingleInputDestination>>)> = {
            destinations
                .lock()
                .expect("destinations mutex poisoned")
                .iter()
                .map(|(alpn, dest)| (alpn.clone(), Arc::clone(dest)))
                .collect()
        };
        for (alpn, dest) in entries {
            if let Some(app_data) = app_data_by_alpn.get(&alpn) {
                inner.send_announce(&dest, Some(app_data.as_slice())).await;
            }
        }
    }
}
