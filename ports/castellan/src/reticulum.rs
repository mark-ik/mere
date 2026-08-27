// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Derived Reticulum station credentials.
//!
//! Reticulum identities have two independent 32-byte secret halves: one for
//! X25519 exchange and one for Ed25519 signing. A Persona provider derives
//! them under separate, length-delimited salts so a sited radio is tied to a
//! Persona without becoming a copy of the Persona's master seed.
//!
//! This module has no dependency on Retinue. Its caller converts the returned
//! material into Retinue's `PrivateIdentity`, then scrubs the transient byte
//! array. A resident Castellan authority can later expose the same operation
//! over its gate instead of handing material to an in-process port.

use std::fmt;

use personae::{IdentityError, IdentityProvider};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Narrow RemoteAuth issuance and validation for an unattended station.
///
/// This is optional because it is the host-side wallet adapter, while derived
/// station material itself remains a Personae-only operation.
#[cfg(feature = "station-grants")]
pub mod grant;

const RETICULUM_STATION_DOMAIN: &[u8] = b"mere.castellan.reticulum.station/v1";
const EXCHANGE_PURPOSE: &[u8] = b"x25519";
const SIGNING_PURPOSE: &[u8] = b"ed25519";

/// The two secret halves needed to construct one Reticulum station identity.
///
/// The material is deliberately neither cloneable nor debuggable. It is
/// derived for one station scope and zeroized when dropped.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ReticulumStationMaterial {
    secret: [u8; 64],
}

impl fmt::Debug for ReticulumStationMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ReticulumStationMaterial(<redacted>)")
    }
}

impl ReticulumStationMaterial {
    /// Derive one station credential from a Persona provider and stable station scope.
    ///
    /// `station_scope` should identify the commissioned device, such as a
    /// stable device id. It must not be a mutable display name or a coordinate.
    pub fn derive(
        provider: &dyn IdentityProvider,
        station_scope: &[u8],
    ) -> Result<Self, IdentityError> {
        if station_scope.is_empty() {
            return Err(IdentityError::DerivationFailed(
                "a Reticulum station scope must not be empty".into(),
            ));
        }

        let exchange_salt = derivation_salt(EXCHANGE_PURPOSE, station_scope);
        let signing_salt = derivation_salt(SIGNING_PURPOSE, station_scope);
        let exchange_key = provider.derive_keypair(&exchange_salt)?;
        let signing_key = provider.derive_keypair(&signing_salt)?;
        let mut exchange_seed = exchange_key.to_seed();
        let mut signing_seed = signing_key.to_seed();
        let mut secret = [0_u8; 64];
        secret[..32].copy_from_slice(&exchange_seed);
        secret[32..].copy_from_slice(&signing_seed);
        exchange_seed.zeroize();
        signing_seed.zeroize();

        Ok(Self { secret })
    }

    /// Copy the Reticulum private wire form for immediate consumption.
    ///
    /// Callers must use it only to construct their typed identity and must
    /// zeroize the returned bytes immediately afterwards. It is not a storage
    /// format and must not be logged or persisted.
    pub fn secret_bytes(&self) -> [u8; 64] {
        self.secret
    }
}

fn derivation_salt(purpose: &[u8], station_scope: &[u8]) -> Vec<u8> {
    let mut salt = Vec::with_capacity(
        RETICULUM_STATION_DOMAIN.len() + 4 + purpose.len() + 8 + station_scope.len(),
    );
    salt.extend_from_slice(RETICULUM_STATION_DOMAIN);
    salt.extend_from_slice(&(purpose.len() as u32).to_le_bytes());
    salt.extend_from_slice(purpose);
    salt.extend_from_slice(&(station_scope.len() as u64).to_le_bytes());
    salt.extend_from_slice(station_scope);
    salt
}

#[cfg(test)]
mod tests {
    use personae::InMemoryProvider;

    use super::*;

    #[test]
    fn station_material_is_stable_for_one_persona_and_station() {
        let provider = InMemoryProvider::from_seed([0x31; 32]);
        let first = ReticulumStationMaterial::derive(&provider, b"radio:ridge-north").unwrap();
        let second = ReticulumStationMaterial::derive(&provider, b"radio:ridge-north").unwrap();

        assert_eq!(first.secret_bytes(), second.secret_bytes());
    }

    #[test]
    fn station_material_is_distinct_across_station_scopes() {
        let provider = InMemoryProvider::from_seed([0x31; 32]);
        let first = ReticulumStationMaterial::derive(&provider, b"radio:ridge-north").unwrap();
        let second = ReticulumStationMaterial::derive(&provider, b"radio:ridge-south").unwrap();

        assert_ne!(first.secret_bytes(), second.secret_bytes());
    }

    #[test]
    fn station_scope_is_required() {
        let provider = InMemoryProvider::from_seed([0x31; 32]);
        let error = ReticulumStationMaterial::derive(&provider, b"").unwrap_err();

        assert!(error.to_string().contains("must not be empty"));
    }
}
