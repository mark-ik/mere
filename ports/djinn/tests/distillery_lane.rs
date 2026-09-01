// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The receipt for the Distillery lane: a resident started from nothing but
//! owner-stated policy runs a real mesh job to completion on the wall clock and
//! shuts down clean.
//!
//! Everything here goes through [`DjinnResident::open`] rather than straight to
//! `ResidentDistillery::open`, because the claim being proved is a claim about
//! *composition*: the lane is opened in construction order behind credentials,
//! blob custody and Knot, and it is closed by the resident's own ordered
//! shutdown. Nothing in this file dials a broker or builds a graph session, so
//! the extra path costs one Castellan claim and one blob-store open.
//!
//! Three properties are load-bearing, and each has its own refusal:
//!
//! 1. **No permissive default.** The installed policy is asserted to be the
//!    converted device settings and asserted *not* to be
//!    [`DevicePolicy::permissive`].
//! 2. **No fabricated condition.** A thermal limit on a machine with no
//!    thermometer and no stated temperature refuses composition by name.
//! 3. **No assumed lending posture.** An enabled lane on a device that has
//!    never stated one refuses rather than lending everything.
//!
//! The clock is [`mesh_host::SystemClock`] throughout — there is no
//! `ManualClock` here, because a lease is a wall-clock promise and a receipt
//! taken on a hand-set clock would not be a receipt about this device.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use distillery::ResidentReceipt;
use djinn::resident::DjinnResident;
use djinn::settings::{DistilleryLaneSettings, OwnerSettings};
use mesh::spec::{DeterminismClass, JobSpec};
use mesh::{DevicePolicy, NetworkClass, ResourceId};
use mesh_host::Step;
use pandect::{DeviceSettings, MeshLendingSettings, StatedConditionSettings};
use personae::bootstrap::{self, Unlock};
use personae::{IdentityVault, ProfileId};
use tokio::sync::Notify;

const PASSPHRASE: &[u8] = b"djinn-distillery-lane-receipt-passphrase";

fn unlock() -> Unlock {
    Unlock::passphrase(PASSPHRASE)
}

fn profile() -> ProfileId {
    ProfileId("works".into())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Open the shared Personae vault the way `bin/djinn.rs` does, and keep it
/// open.
///
/// Holding it across [`DjinnResident::open`] is the point: the lane derives its
/// mesh author from the same vault directory and therefore opens it a *second*
/// time in this process. If Personae's storage opens were exclusive, every test
/// below would fail at that second open rather than at its assertion.
fn open_vault(root: &Path) -> (PathBuf, IdentityVault<Box<dyn personae::IdentityStorage>>) {
    let vault_dir = root.join("vault");
    let opened = bootstrap::open_storage(&vault_dir, unlock()).expect("open vault storage");
    let (record, _created) =
        bootstrap::load_or_create_profile(&*opened.storage, &profile()).expect("create profile");
    (
        vault_dir,
        IdentityVault::with_profile(opened.storage, record),
    )
}

/// A lending posture whose one enabled rule is idle time.
///
/// Every other check is switched off *by the owner*, and each `0` here is a
/// statement rather than a shrug:
///
/// - `max_thermal_c: 0` because this build has no thermometer, which is
///   precisely what the second test proves is refused when it is non-zero.
/// - `min_network: "offline"` because a network floor cannot be rescued by a
///   stated value: a sensed `Offline` is a real observation and beats the
///   owner's word, so a machine with no default gateway (a CI box, a laptop on
///   a plane) would withhold forever and this receipt would hang instead of
///   failing. The floor is exercised by unit conversion, not by this run.
/// - `quiet_hours: None` because a wall-clock window would make this receipt
///   pass or fail depending on the hour it was run.
///
/// `min_idle_ms: 1` is the enabled rule the sensor genuinely covers: sensed
/// from `GetLastInputInfo` on Windows, and stated below so the same settings
/// are coverable anywhere.
fn lending() -> MeshLendingSettings {
    MeshLendingSettings {
        min_idle_ms: 1,
        min_battery_pct: 0,
        max_thermal_c: 0,
        min_network: "offline".into(),
        max_bandwidth_in_use_kbps: 0,
        quiet_hours: None,
        max_concurrent_jobs: 1,
        reclaim_grace_ms: 0,
        supervises_leases: true,
        stated: StatedConditionSettings {
            idle_ms: Some(600_000),
            battery_pct: None,
            on_mains: None,
            thermal_c: None,
            network: Some("wired".into()),
            bandwidth_in_use_kbps: None,
        },
    }
}

fn lane() -> DistilleryLaneSettings {
    DistilleryLaneSettings {
        tick_every_ms: 25,
        // Explicit-only: this receipt is about running work, and a maintenance
        // cadence firing mid-run would put a checkpoint between the job and the
        // assertion about it.
        maintenance_every_ms: None,
        blob_gc_every_ms: 1_000,
        collect_after_checkpoint: true,
        retention_revision: "3f".repeat(32),
        promised_floor: "forever".into(),
        privacy_ceiling: "until-checkpoint".into(),
        erase_terminal_at_checkpoint: true,
        max_skew_ms: 0,
    }
}

fn write_device_settings(data_root: &Path, mesh_lending: Option<MeshLendingSettings>) {
    pandect::save_device_settings(
        data_root,
        &DeviceSettings {
            mesh_lending,
            ..DeviceSettings::default()
        },
    )
    .expect("write device settings");
}

/// The refusal text, or a failure that closes the resident it should not have
/// been handed. [`DjinnResident`] is not `Debug`, so `expect_err` cannot be
/// used, and leaking an opened blob store on the failing path would bury the
/// real message under a store-lock error in the next test.
async fn refusal_from(opened: Result<DjinnResident, String>, why: &str) -> String {
    match opened {
        Err(refusal) => refusal,
        Ok(resident) => {
            let _ = resident.shutdown().await;
            panic!("{why}");
        }
    }
}

fn owner(lane: Option<DistilleryLaneSettings>) -> OwnerSettings {
    OwnerSettings {
        distillery: lane,
        ..OwnerSettings::default()
    }
}

/// The whole receipt: stated policy in, completed job out, clean close.
///
/// Windows-only, and the reason is itself part of the discipline: the lane
/// reads `HostFacts::memory_mib` from the operating system and refuses to
/// advertise a capacity it never measured. Only the Windows half of this crate
/// can measure one today, so only Windows can produce this receipt. The two
/// refusal tests below run everywhere, because they refuse before any fact is
/// read.
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_works_run_a_real_mesh_job_to_completion_and_close_clean() {
    let directory = tempfile::tempdir().expect("temporary resident root");
    let data_root = directory.path().join("data");
    let (vault_dir, identity) = open_vault(directory.path());
    write_device_settings(&data_root, Some(lending()));

    let mut resident = DjinnResident::open(
        &identity,
        &data_root,
        &profile(),
        owner(Some(lane())),
        &vault_dir,
        unlock(),
    )
    .await
    .expect("compose the Djinn resident with its Distillery lane");
    assert!(resident.distillery_enabled());

    let mut works = resident
        .take_distillery()
        .expect("the lane was composed and can be driven");

    // No permissive default: the supervisor is running the owner's posture.
    let installed = works.authority().host().policy().clone();
    assert_eq!(
        installed,
        DevicePolicy {
            min_idle_ms: 1,
            min_battery_pct: 0,
            max_thermal_c: 0,
            min_network: NetworkClass::Offline,
            max_bandwidth_in_use_kbps: 0,
            quiet_hours: None,
            max_concurrent_jobs: 1,
            allowed_resources: Default::default(),
            accepted_checkpoints: Default::default(),
            reclaim_grace_ms: 0,
            supervises_leases: true,
        },
        "the installed policy must be exactly the device's stated posture"
    );
    assert_ne!(
        installed,
        DevicePolicy::permissive(),
        "a lane that fell back to permissive() would lend on nobody's authority"
    );
    assert_eq!(
        works.authority().host().me(),
        works.author(),
        "the works speak as the profile's derived mesh author"
    );

    let input = works
        .space()
        .put(b"djinn distillery mash")
        .await
        .expect("stage the job input");
    works
        .post_job(
            JobSpec::simple(
                ResourceId::parse("mesh.blake3/v1").unwrap(),
                "payload",
                input.clone(),
                "result",
                32,
                DeterminismClass::Exact,
            ),
            1,
            now_ms(),
        )
        .await
        .expect("post the job as this device");

    let stop = Arc::new(Notify::new());
    let signal = Arc::clone(&stop);
    let mut observed: Vec<Step> = Vec::new();
    let mut receipts: Vec<ResidentReceipt> = Vec::new();
    let mut completed = false;
    works
        .run_until(
            async move {
                // The timeout is a failure bound, not the expected path: the
                // assertions below distinguish the two.
                tokio::select! {
                    _ = signal.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                }
            },
            |receipt| {
                if let ResidentReceipt::Tick { steps } = &receipt {
                    for step in steps {
                        if !matches!(step, Step::Idle) {
                            observed.push(step.clone());
                        }
                    }
                    if steps
                        .iter()
                        .any(|step| matches!(step, Step::Completed { .. }))
                    {
                        completed = true;
                        stop.notify_one();
                    }
                }
                receipts.push(receipt);
            },
        )
        .await
        .expect("the works ran to the stop request");

    println!("observed step sequence: {observed:?}");
    assert!(
        completed,
        "the works never completed the job; steps were {observed:?}"
    );
    assert!(
        observed
            .iter()
            .any(|step| matches!(step, Step::Claimed { .. })),
        "a completion with no claim would mean the board was not really driven: {observed:?}"
    );
    assert!(matches!(
        receipts.last(),
        Some(ResidentReceipt::StopRequested)
    ));

    // The lane goes back so the resident's own ordered close owns it: works
    // first, then credentials, then the shared blob custody.
    resident.restore_distillery(Some(works));
    resident
        .shutdown()
        .await
        .expect("clean ordered resident shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_thermal_limit_with_no_stated_temperature_refuses_composition() {
    let directory = tempfile::tempdir().expect("temporary resident root");
    let data_root = directory.path().join("data");
    let (vault_dir, identity) = open_vault(directory.path());

    let mut hot = lending();
    hot.max_thermal_c = 85;
    assert_eq!(hot.stated.thermal_c, None);
    write_device_settings(&data_root, Some(hot));

    let refusal = refusal_from(
        DjinnResident::open(
            &identity,
            &data_root,
            &profile(),
            owner(Some(lane())),
            &vault_dir,
            unlock(),
        )
        .await,
        "a thermal limit on a device with no thermometer is not a policy",
    )
    .await;

    assert!(refusal.contains("thermal"), "{refusal}");
    assert!(refusal.contains("max_thermal_c"), "{refusal}");
    assert!(
        !refusal.contains("close blob custody"),
        "the refusal must unwind cleanly, not report a second failure: {refusal}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_enabled_lane_with_no_stated_lending_posture_refuses_composition() {
    let directory = tempfile::tempdir().expect("temporary resident root");
    let data_root = directory.path().join("data");
    let (vault_dir, identity) = open_vault(directory.path());
    write_device_settings(&data_root, None);

    let refusal = refusal_from(
        DjinnResident::open(
            &identity,
            &data_root,
            &profile(),
            owner(Some(lane())),
            &vault_dir,
            unlock(),
        )
        .await,
        "a works lane with no lending posture must refuse rather than assume one",
    )
    .await;

    assert!(refusal.contains("mesh_lending"), "{refusal}");
    assert!(
        refusal.contains("must refuse rather than assume"),
        "the refusal should say why, not just that: {refusal}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profile_with_no_lane_composes_no_works_at_all() {
    let directory = tempfile::tempdir().expect("temporary resident root");
    let data_root = directory.path().join("data");
    let (vault_dir, identity) = open_vault(directory.path());
    // Deliberately no device settings file at all: an absent lane must not
    // read, need, or complain about a lending posture.

    let resident = DjinnResident::open(
        &identity,
        &data_root,
        &profile(),
        owner(None),
        &vault_dir,
        unlock(),
    )
    .await
    .expect("a resident with no works lane still opens");
    assert!(!resident.distillery_enabled());
    assert!(resident.distillery().is_none());
    resident.shutdown().await.expect("clean shutdown");
}
