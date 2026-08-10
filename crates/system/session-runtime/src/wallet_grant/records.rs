// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Upserts into the roster, the identity wallet's grant index, and the local
//! device record.

use std::path::Path;
use std::io;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider, PersonaId};

use crate::wallet_store::*;

use super::*;

pub(crate) fn upsert_remote_auth_device_record(
    roster: &mut DeviceRoster,
    spec: &RemoteAuthGrantSpec,
    grant_ref: CarryRef,
) {
    if let Some(existing) = roster
        .devices
        .iter_mut()
        .find(|record| record.device_id == spec.device_id)
    {
        existing.device_pubkey = spec.delegatee_pubkey;
        existing.label = spec.label.clone();
        existing.mode = DeviceMode::RemoteAuth;
        existing.exposure = spec.exposure;
        existing.grant_ref = Some(grant_ref);
        return;
    }
    roster.devices.push(DeviceRecord {
        device_id: spec.device_id,
        device_pubkey: spec.delegatee_pubkey,
        label: spec.label.clone(),
        mode: DeviceMode::RemoteAuth,
        exposure: spec.exposure,
        grant_ref: Some(grant_ref),
    });
}

pub(crate) fn upsert_grant_index(wallet: &mut IdentityWalletManifest, device_id: DeviceId, grant_ref: CarryRef) {
    if let Some(existing) = wallet
        .grant_index
        .iter_mut()
        .find(|known| known.device_id == device_id)
    {
        existing.grant_ref = Some(grant_ref);
        return;
    }
    wallet.grant_index.push(DeviceGrantRef {
        device_id,
        grant_ref: Some(grant_ref),
    });
}

pub(crate) fn upsert_local_remote_auth_record(
    roster: &mut DeviceRoster,
    local: &LocalDeviceIdentity,
    grant_ref: CarryRef,
) {
    if let Some(existing) = roster
        .devices
        .iter_mut()
        .find(|record| record.device_id == local.device_id)
    {
        existing.device_pubkey = local.public_key();
        existing.label = local.label.clone();
        existing.mode = DeviceMode::RemoteAuth;
        existing.exposure = DeviceExposure::HiddenClient;
        existing.grant_ref = Some(grant_ref);
        return;
    }
    roster.devices.push(DeviceRecord {
        device_id: local.device_id,
        device_pubkey: local.public_key(),
        label: local.label.clone(),
        mode: DeviceMode::RemoteAuth,
        exposure: DeviceExposure::HiddenClient,
        grant_ref: Some(grant_ref),
    });
}
