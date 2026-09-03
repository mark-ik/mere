// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use personae::{PersonaId, SealedRecordStorage};
use tempfile::tempdir;

use super::*;

const RFC4226_SECRET_BASE32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

fn participant() -> OtpReleaseParticipantClaim {
    OtpReleaseParticipantClaim::unverified("device:q-pc", "session:carrier-proof").unwrap()
}

#[test]
fn independent_gates_over_one_open_store_serialize_hotp() {
    let dir = tempdir().unwrap();
    let storage = SealedRecordStorage::open_with_key(dir.path(), [0x62; 32]);
    let items = OtpItemStore::new(storage, PersonaId::new());
    let item = items
        .import_otpauth_uri(&format!(
            "otpauth://hotp/Merely:mark?secret={RFC4226_SECRET_BASE32}&issuer=Merely&counter=0"
        ))
        .unwrap();
    let now = Arc::new(AtomicU64::new(1));
    let clock = || {
        let now = Arc::clone(&now);
        Arc::new(move || Ok(now.load(Ordering::SeqCst))) as Arc<ReleaseClock>
    };
    let left = OtpReleaseGate::with_clock(items.clone(), OtpReleasePolicy::default(), clock());
    let right = OtpReleaseGate::with_clock(items, OtpReleasePolicy::default(), clock());
    let left_request = left.petition(item.id, participant()).unwrap();
    let right_request = right.petition(item.id, participant()).unwrap();

    let left_thread = std::thread::spawn(move || {
        left.approve(left_request.id)
            .unwrap()
            .tile()
            .code_at_unix_time(1)
            .unwrap()
            .to_string()
    });
    let right_thread = std::thread::spawn(move || {
        right
            .approve(right_request.id)
            .unwrap()
            .tile()
            .code_at_unix_time(1)
            .unwrap()
            .to_string()
    });
    let mut codes = [left_thread.join().unwrap(), right_thread.join().unwrap()];
    codes.sort();

    assert_eq!(codes, ["287082", "755224"]);
}
