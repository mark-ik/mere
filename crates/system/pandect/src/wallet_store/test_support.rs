// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Shared fixtures for the store modules' test suites.

use std::fs;
use std::path::PathBuf;

use identity::PersonaId;
use uuid::Uuid;

use super::{DeviceId, KeyEpochId, PersonaChainRoot};

pub(super) fn temp_data_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mere-wallet-store-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

pub(super) fn fixture_persona() -> PersonaId {
    PersonaId::from_uuid(Uuid::from_u128(0x1111))
}

pub(super) fn fixture_device() -> DeviceId {
    DeviceId::from_uuid(Uuid::from_u128(0x2222))
}

pub(super) fn fixture_chain_root() -> PersonaChainRoot {
    PersonaChainRoot([7u8; 32])
}

pub(super) fn fixture_epoch() -> KeyEpochId {
    KeyEpochId(Uuid::from_u128(0x3333))
}
