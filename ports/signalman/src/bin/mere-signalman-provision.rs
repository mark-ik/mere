#![forbid(unsafe_code)]

// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0


use std::collections::BTreeMap;
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mere_signalman::{
    SITED_STATION_CONTROL_TITLE, SitedStationControlResult, SitedStationCredential,
    SitedStationHead, SitedStationHeadError,
};
use pandect::wallet_store::identity_auto_unlock_root_path;
use pandect::{DeviceId, ensure_wallet_state};
use personae::{InMemoryProvider, PersonaId, SealedRecordStorage, load_or_create_auto_unlock_root};

struct ProvisionArgs {
    authority_root: PathBuf,
    station_root: PathBuf,
    record: PathBuf,
    label: String,
    expires_hours: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let issued_at_ms = unix_time_ms()?;
    let expires_at_ms = issued_at_ms
        .checked_add(
            args.expires_hours
                .checked_mul(60 * 60 * 1_000)
                .ok_or("grant duration is too large")?,
        )
        .ok_or("grant expiry is too large")?;

    let root_key =
        load_or_create_auto_unlock_root(identity_auto_unlock_root_path(&args.station_root))?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "this host has no automatic sealed-station unlock backend",
                )
            })?;
    let storage = SealedRecordStorage::open_with_key(&args.station_root, root_key);
    refuse_existing_head(storage.clone(), &args.record)?;

    let seed = ensure_wallet_state(
        &args.authority_root,
        PersonaId::default_persona(),
        "Signalman bench authority",
    )?;
    let provider = InMemoryProvider::from_seed(seed);
    let device_id = DeviceId::new();
    let credential = SitedStationCredential::derive_for_device(&provider, device_id)?;
    let grant = credential.issue_remote_auth_grant(
        &args.authority_root,
        device_id,
        &args.label,
        issued_at_ms,
        expires_at_ms,
    )?;

    let head = credential.provision_head(storage.clone(), &args.record)?;
    let control = credential.control_signer().grant(&grant)?;
    let acknowledgement = head
        .receive_delivery(
            SITED_STATION_CONTROL_TITLE,
            &control.to_bytes()?,
            issued_at_ms,
        )?
        .ok_or("grant control did not produce an acknowledgement")?;
    let result = acknowledgement.verify(credential.public_identity(), &control)?;
    if result != &(SitedStationControlResult::GrantInstalled { expires_at_ms }) {
        return Err(format!("unexpected station acknowledgement: {result:?}").into());
    }

    let restored = SitedStationHead::restore(storage, &args.record)?;
    restored.authorize_at(issued_at_ms)?;

    println!("device_id={}", device_id.as_uuid());
    println!(
        "station_address={}",
        outrider::delivery_destination(restored.public_identity())
    );
    println!("station_identity={}", restored.public_identity().hash());
    println!("issued_at_ms={issued_at_ms}");
    println!("expires_at_ms={expires_at_ms}");
    println!("station_root={}", args.station_root.display());
    println!("station_record={}", args.record.display());
    Ok(())
}

fn refuse_existing_head(storage: SealedRecordStorage, record: &Path) -> Result<(), Box<dyn Error>> {
    match SitedStationHead::restore(storage, record) {
        Ok(head) => Err(format!(
            "station record already exists for device {} at {}; refusing to replace it",
            head.device_id().as_uuid(),
            record.display()
        )
        .into()),
        Err(SitedStationHeadError::MissingState { .. }) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn parse_args() -> Result<ProvisionArgs, Box<dyn Error>> {
    let mut values = BTreeMap::new();
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        if flag == "--help" || flag == "-h" {
            print_usage();
            std::process::exit(0);
        }
        if !flag.starts_with("--") {
            return Err(format!("unexpected argument {flag:?}").into());
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        if values.insert(flag.clone(), value).is_some() {
            return Err(format!("duplicate argument {flag}").into());
        }
    }

    let authority_root = required_path(&mut values, "--authority-root")?;
    let station_root = required_path(&mut values, "--station-root")?;
    let record = required_path(&mut values, "--record")?;
    if record.is_absolute() {
        return Err("--record must be relative to --station-root".into());
    }
    let label = required(&mut values, "--label")?;
    if label.trim().is_empty() {
        return Err("--label must not be empty".into());
    }
    let expires_hours = required(&mut values, "--expires-hours")?.parse::<u64>()?;
    if expires_hours == 0 {
        return Err("--expires-hours must be greater than zero".into());
    }
    if let Some(flag) = values.keys().next() {
        return Err(format!("unknown argument {flag}").into());
    }

    Ok(ProvisionArgs {
        authority_root,
        station_root,
        record,
        label,
        expires_hours,
    })
}

fn required(values: &mut BTreeMap<String, String>, name: &str) -> Result<String, Box<dyn Error>> {
    values
        .remove(name)
        .ok_or_else(|| format!("missing required argument {name}").into())
}

fn required_path(
    values: &mut BTreeMap<String, String>,
    name: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    Ok(PathBuf::from(required(values, name)?))
}

fn unix_time_ms() -> Result<u64, std::time::SystemTimeError> {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH)?;
    Ok(u64::try_from(duration.as_millis()).expect("unix millisecond count fits u64"))
}

fn print_usage() {
    println!(
        "usage: mere-signalman-provision \\\n  --authority-root PATH \\\n  --station-root PATH \\\n  --record RELATIVE_PATH \\\n  --label NAME \\\n  --expires-hours HOURS"
    );
}
