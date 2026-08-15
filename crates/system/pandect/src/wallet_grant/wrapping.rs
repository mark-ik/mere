// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The retained per-device wrapping keys that let a later epoch rotation
//! refresh a grant without rerunning the pairing ceremony.

use std::io;
use std::path::Path;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider, PersonaId};

use crate::wallet_store::*;

use super::*;

pub(crate) fn upsert_remote_auth_wrapping_key(
    data_root: &Path,
    device_id: DeviceId,
    ticket_id: Option<Uuid>,
    wrapping_key: [u8; 32],
) -> io::Result<()> {
    let mut bridge = load_remote_auth_wrapping_key_bridge(data_root)?
        .unwrap_or_else(RemoteAuthWrappingKeyBridge::new);
    if let Some(existing) = bridge
        .keys
        .iter_mut()
        .find(|known| known.device_id == device_id)
    {
        if ticket_id.is_some() {
            existing.ticket_id = ticket_id;
        }
        existing.wrapping_key = wrapping_key;
    } else {
        bridge.keys.push(RemoteAuthWrappingKeyRecord {
            device_id,
            ticket_id,
            wrapping_key,
        });
    }
    save_remote_auth_wrapping_key_bridge(data_root, &bridge)
}

pub(crate) fn remove_remote_auth_wrapping_key(
    data_root: &Path,
    device_id: DeviceId,
) -> io::Result<()> {
    let Some(mut bridge) = load_remote_auth_wrapping_key_bridge(data_root)? else {
        return Ok(());
    };
    let original_len = bridge.keys.len();
    bridge.keys.retain(|known| known.device_id != device_id);
    if bridge.keys.len() != original_len {
        save_remote_auth_wrapping_key_bridge(data_root, &bridge)?;
    }
    Ok(())
}

pub(crate) fn load_remote_auth_wrapping_key(
    data_root: &Path,
    device_id: DeviceId,
) -> io::Result<Option<[u8; 32]>> {
    Ok(load_remote_auth_wrapping_key_bridge(data_root)?
        .and_then(|bridge| {
            bridge
                .keys
                .into_iter()
                .find(|known| known.device_id == device_id)
        })
        .map(|known| known.wrapping_key))
}
