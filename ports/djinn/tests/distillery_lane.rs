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
//! Four properties are load-bearing, and each has its own refusal:
//!
//! 1. **No permissive default.** The installed policy is asserted to be the
//!    converted device settings and asserted *not* to be
//!    [`DevicePolicy::permissive`].
//! 2. **No fabricated condition.** A thermal limit on a machine with no
//!    thermometer and no stated temperature refuses composition by name.
//! 3. **No assumed lending posture.** An enabled lane on a device that has
//!    never stated one refuses rather than lending everything.
//! 4. **No feature the build does not carry.** A lane whose settings name a
//!    trainer refuses on a build compiled without the `trainer` feature. That
//!    test lives here, under `cfg(not(feature = "trainer"))`, so the gate has a
//!    live assertion in *both* feature sets: the composed half is proved by
//!    `distillery_trainer.rs`, the refusing half by this file.
//!
//! The clock is [`mesh_host::SystemClock`] throughout — there is no
//! `ManualClock` here, because a lease is a wall-clock promise and a receipt
//! taken on a hand-set clock would not be a receipt about this device.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use common::{
    lane, lending, now_ms, open_vault, owner, profile, refusal_from, unlock, write_device_settings,
};
use distillery::ResidentReceipt;
use djinn::resident::DjinnResident;
use mesh::spec::{DeterminismClass, JobSpec};
use mesh::{DevicePolicy, NetworkClass, ResourceId};
use mesh_host::Step;
use tokio::sync::Notify;

/// The whole receipt: stated policy in, completed job out, clean close.
///
/// It runs wherever the lane can compose, which is wherever this crate can
/// measure physical memory: Windows, Linux and macOS. That is the one fact
/// `HostFacts` refuses to invent, so it is the one thing that decides where
/// this receipt is a receipt.
///
/// Off Windows nothing about the device is *sensed*, and the run leans on the
/// fixture's stated fallbacks instead. That is not a weakening of the claim:
/// `common::lending` enables exactly one rule, `min_idle_ms`, and states an
/// idle time for it; battery, thermal, network and bandwidth are each switched
/// off by the owner, for reasons the fixture's own docs give. A rule left
/// enabled with nothing sensed and nothing stated behind it would refuse
/// composition on such a machine rather than pass quietly — which is exactly
/// what the thermal test below proves, on purpose.
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
            allowed_resources: BTreeSet::from([ResourceId::parse("mesh.blake3/v1").unwrap()]),
            accepted_checkpoints: BTreeSet::from([mesh::CheckpointClass::Restart]),
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
        installed.allowed_resources,
        BTreeSet::from([ResourceId::parse("mesh.blake3/v1").unwrap()]),
        "the stated allowed_resources must reach the installed policy, not just parse"
    );
    assert_eq!(
        works.authority().host().me(),
        works.author(),
        "the works speak as the profile's derived mesh author"
    );
    assert!(
        works.model_library().is_none(),
        "a lane that stated no trainer must not have opened a model library"
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

/// The gate's other half: this build carries no trainer, so a lane whose
/// settings say it trains is refused rather than composed without one.
///
/// The failure this exists against is the quiet one. A composition that
/// dropped the trainer and started anyway would leave the owner reading their
/// own settings file as proof the device trains, and the ring reading a device
/// advertising capacity for jobs it would then fail. The refusal is checked to
/// name both ways out — recompile, or say `null` — because a refusal that does
/// not say what to do next is a wall.
///
/// It runs everywhere, including off Windows: this refusal is raised before
/// any host fact is read, so it does not need a machine that can measure its
/// own memory.
#[cfg(not(feature = "trainer"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_with_no_trainer_refuses_a_lane_that_states_one() {
    use djinn::settings::TrainerLaneSettings;

    let directory = tempfile::tempdir().expect("temporary resident root");
    let data_root = directory.path().join("data");
    let (vault_dir, identity) = open_vault(directory.path());
    // A perfectly good lending posture: the refusal under test must be about
    // the missing trainer, not about anything else being wrong.
    write_device_settings(&data_root, Some(lending()));

    let mut asks_for_a_trainer = lane();
    asks_for_a_trainer.trainer = Some(TrainerLaneSettings {
        device: "cpu".into(),
        library_root: None,
    });
    // The settings themselves are valid; only this *build* cannot honour them.
    assert_eq!(asks_for_a_trainer.validate(), Ok(()));

    let refusal = refusal_from(
        DjinnResident::open(
            &identity,
            &data_root,
            &profile(),
            owner(Some(asks_for_a_trainer)),
            &vault_dir,
            unlock(),
        )
        .await,
        "a build with no trainer must refuse a lane that states one rather than \
         composing a works that cannot do what its settings claim",
    )
    .await;

    assert!(refusal.contains("distillery.trainer"), "{refusal}");
    assert!(refusal.contains("carries no trainer"), "{refusal}");
    assert!(
        refusal.contains("`trainer` feature"),
        "the refusal must name the way out: {refusal}"
    );
    assert!(
        refusal.contains("null"),
        "the refusal must name the other way out: {refusal}"
    );
}
