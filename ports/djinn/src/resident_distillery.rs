// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Resident Distillery composition inside Djinn.
//!
//! Distillery owns the works: the supervisor loop, retention maintenance, and
//! blob custody for one mesh. Mesh owns job grammar and leases. Pandect owns
//! the device's lending posture. Personae owns the identity. This module is
//! only the place those authorities are put together, and its whole discipline
//! is that **it invents nothing**.
//!
//! That is a stronger claim than it sounds, so here is what it costs:
//!
//! - **No `DevicePolicy::permissive()`.** The lending policy is converted from
//!   pandect's device-scoped `mesh_lending` block. An enabled lane on a device
//!   with no such block is refused, because a composition that needs a lending
//!   posture and cannot find one must say so rather than lend everything.
//! - **No fabricated condition.** Conditions come from
//!   [`DeviceConditionSensor`], which reports each signal's provenance, and
//!   [`validate_policy_coverage`] refuses before the resident binds if any
//!   enabled rule rests on a signal nobody can supply. Thermal is not sensed on
//!   this platform, so a `max_thermal_c` above zero with no stated temperature
//!   refuses composition — loudly, at start, rather than by never lending.
//! - **No invented capacity.** [`mesh::HostFacts::memory_mib`] is read from the
//!   operating system, and a build that cannot read it refuses rather than
//!   advertising a number.
//! - **No borrowed identity.** The mesh id is derived under Distillery's own
//!   salt from the same Personae profile Djinn resolved, and the retention
//!   checkpoint authority is that profile's derived mesh author.
//!
//! ## What this lane deliberately does not adopt
//!
//! It builds its own [`transport::P2pandaTransport`] and its own job-byte
//! store, exactly as [`InstalledAuthority::bind_resident`] already does. It
//! does **not** borrow Djinn's shared [`crate::resident_blobs`] custody. Mesh
//! job payloads are governed by a mesh retention policy with its own checkpoint
//! authority, and personal-graph transfers and Knot evidence are governed by
//! their own leases; folding them into one physical store would make one
//! policy's collection decision reach into another policy's bytes.

use std::collections::BTreeSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use distillery::{
    Distillery, InstalledAuthority, InstalledSettings, ResidentAuthority, ResidentReceipt,
    ResidentSettings, RetentionSettings,
};
use mesh::spec::JobSpec;
use mesh::{AvailabilityPolicy, DevicePolicy, ErasurePolicy, MeshRetentionPolicy};
use muniment::RedbBackend;
use personae::bootstrap::Unlock;
use personae::{Ed25519Keypair, ProfileId};
use tokio::sync::Mutex;

use crate::conditions::{DeviceConditionSensor, StatedConditions, validate_policy_coverage};
use crate::settings::{DistilleryLaneSettings, parse_keep_bound, sanitize_profile};

/// One resident Distillery works, bound to this profile's personal mesh.
pub struct ResidentDistillery {
    resident: ResidentAuthority<RedbBackend>,
    /// Retained because on a single device the poster and the runner are the
    /// same process: this resident authors its own job posts as itself. The
    /// key is already resident inside the supervisor for the lane's whole
    /// life, so keeping the handle exposes nothing the composition did not
    /// already hold.
    author: Ed25519Keypair,
    mesh_id: [u8; 32],
    profile: String,
    mesh_root: PathBuf,
    /// The persona-scoped model library the trainer writes artifacts into,
    /// held for the lane's life so the resource and any reader see one handle
    /// on one redb file. `None` when the owner stated no trainer — and, in a
    /// build without the `trainer` feature, always `None`, because such a
    /// build refuses a lane that asks for one rather than opening a store
    /// nothing would ever write to.
    model_library: Option<Arc<Mutex<RedbBackend>>>,
}

impl ResidentDistillery {
    /// Open the works for one already-resolved Personae profile.
    pub async fn open(
        data_root: &Path,
        profile: &ProfileId,
        vault_dir: &Path,
        unlock: Unlock,
        lane: DistilleryLaneSettings,
    ) -> Result<Self, String> {
        lane.validate()?;

        // The gate's honesty, checked before anything is opened: a build with
        // no trainer compiled in cannot answer a lane whose settings say it
        // trains. Composing anyway would leave a device advertising work it
        // would then fail, which is the same sin as a permissive policy
        // default, one layer up.
        #[cfg(not(feature = "trainer"))]
        if lane.trainer.is_some() {
            return Err(
                "distillery.trainer states this lane composes a trainer, but this build \
                 carries no trainer; rebuild with the `trainer` feature or set \
                 distillery.trainer to null. A works must not compose settings it cannot \
                 honour."
                    .into(),
            );
        }

        // Djinn already resolved which face this device speaks as, and that
        // resolution IS the owner's explicit choice; asking them to configure
        // the same profile a second time in Distillery's own settings would be
        // ceremony. A settings file naming a DIFFERENT profile is another
        // matter: two faces in one resident is a surprise, not a shorthand.
        match InstalledSettings::load(data_root)
            .map_err(|error| format!("read Distillery installed settings: {error}"))?
        {
            Some(installed) if installed.profile_id() != *profile => {
                return Err(format!(
                    "the Distillery works are configured for Personae profile {:?} but this \
                     resident speaks as {:?}; reconfigure one of them rather than running two \
                     faces in one process",
                    installed.profile, profile.0
                ));
            }
            Some(_) => {}
            None => {
                InstalledAuthority::configure(data_root, profile.clone())
                    .map_err(|error| format!("configure the Distillery works: {error}"))?;
            }
        }

        let authority = InstalledAuthority::open_with(data_root, vault_dir, unlock)
            .map_err(|error| format!("open the Distillery works: {error}"))?;
        let mesh_id = authority
            .personal_mesh_id()
            .map_err(|error| format!("derive the personal mesh id: {error}"))?;
        let paths = authority.paths(mesh_id);
        paths
            .prepare()
            .map_err(|error| format!("prepare {}: {error}", paths.root().display()))?;

        let lending = load_lending(data_root)?;
        let policy = device_policy(&lending)?;
        let sensor = Arc::new(DeviceConditionSensor::new(stated_conditions(&lending)?));
        validate_policy_coverage(&policy, &sensor.coverage()).map_err(|refusal| {
            format!(
                "this device's mesh lending policy cannot be honestly evaluated here: {refusal}"
            )
        })?;

        let author = authority
            .mesh_author()
            .map_err(|error| format!("derive the mesh author: {error}"))?;
        let store = mesh::MeshStore::at_path_with_retention(
            paths.mesh_store_path(),
            retention_policy(&lane, author.public_key().to_bytes())?,
        )
        .map_err(|error| {
            format!(
                "open the mesh store at {}: {error}",
                paths.mesh_store_path().display()
            )
        })?;

        // Facts and paths are read before `bind_resident` consumes the
        // authority, so nothing below has to reopen a vault to say where it is.
        let facts = host_facts()?;
        let profile_name = authority.profile().0;
        let mesh_root = paths.root().to_path_buf();

        // The trainer's artifact store, opened before the host config closure
        // so a failure to open it refuses composition rather than being
        // swallowed inside a closure that can only return a config.
        #[cfg(feature = "trainer")]
        let model_library = match lane.trainer.as_ref() {
            Some(trainer) => Some(open_model_library(
                data_root,
                &profile_name,
                trainer.library_root.as_deref(),
            )?),
            None => None,
        };
        #[cfg(not(feature = "trainer"))]
        let model_library: Option<Arc<Mutex<RedbBackend>>> = None;

        // Registering is fallible, and the closure below cannot be, so the
        // registry is assembled here and only *installed* there.
        #[cfg(feature = "trainer")]
        let registry = match model_library.as_ref() {
            Some(store) => {
                let mut registry = mesh::ResourceRegistry::builtin();
                registry
                    .register(Arc::new(distillery::TrainerResource::new(
                        Arc::clone(store),
                        distillery::TrainerDevice::ndarray(),
                    )))
                    .map_err(|error| format!("register the trainer resource: {error}"))?;
                Some(registry)
            }
            None => None,
        };

        let resident = authority
            .bind_resident(mesh_id, store, resident_settings(&lane), move |space| {
                let mut config = mesh_host::HostConfig::supervised(space);
                config.policy = policy;
                config.conditions = sensor;
                config.facts = facts;
                // `SystemClock` stays: leases are wall-clock promises other
                // devices read, so a resident that ran on a hand-set clock
                // would be lying to the ring about when it let go.
                //
                // `NoCourier` stays, and on a single device that is truthful
                // rather than a gap: the poster and the runner are this same
                // process, so a job's inputs are already in this blob space
                // and there is nobody to pull them from. A second device makes
                // this a `TransportCourier`, not before.
                //
                // `ResourceRegistry::builtin()` is the floor, and it is all a
                // default build installs. Distillery's trainer resource is
                // added on top only when the owner stated one *and* this build
                // compiled the `trainer` feature, because the resource pulls
                // the burn stack into the desktop resident: a weight every
                // Djinn pays for otherwise, for a capability most of them never
                // use. The two halves are kept honest together — settings that
                // name a trainer a build cannot carry refuse `open` above.
                #[cfg(feature = "trainer")]
                if let Some(registry) = registry {
                    config.registry = registry;
                }
                config
            })
            .await
            .map_err(|error| format!("bind the Distillery resident: {error}"))?;

        Ok(Self {
            resident,
            author,
            mesh_id,
            profile: profile_name,
            mesh_root,
            model_library,
        })
    }

    /// The mesh this works joined, derived rather than configured.
    pub fn mesh_id(&self) -> [u8; 32] {
        self.mesh_id
    }

    /// The Personae profile this works speaks as.
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// The product-private root holding this mesh's durable state.
    pub fn mesh_root(&self) -> &Path {
        &self.mesh_root
    }

    /// This device's mesh author key, as the ring sees it.
    pub fn author(&self) -> [u8; 32] {
        self.author.public_key().to_bytes()
    }

    /// The persona-scoped model library the composed trainer publishes into,
    /// or `None` when this lane composes no trainer.
    ///
    /// Handing the same handle out rather than a path is deliberate: redb is
    /// single-writer, so a reader that opened the file itself would either
    /// fail or read a stale snapshot. Everything that wants the adapter
    /// manifests and evaluation reports this device produced goes through the
    /// one open store the trainer is writing.
    pub fn model_library(&self) -> Option<Arc<Mutex<RedbBackend>>> {
        self.model_library.as_ref().map(Arc::clone)
    }

    /// The product authority, for read-only board and progress projections.
    pub fn authority(&self) -> &Distillery<RedbBackend> {
        self.resident.authority()
    }

    /// The mesh-scoped blob space this works reads and writes.
    ///
    /// Staging a job's input goes through here, which is also why the lane
    /// needs no courier while it is the only device: what it posts, it already
    /// holds.
    pub fn space(&self) -> Arc<mesh_host::TransportBlobSpace> {
        self.resident.storage().space()
    }

    /// Post a job onto this mesh as this device.
    ///
    /// Authoring needs the mesh author key, and on a single device the poster
    /// and the runner are the same process, so this resident signs its own
    /// posts. `at_ms` is the caller's reading of the wall clock, not one this
    /// module invents.
    pub async fn post_job(&self, spec: JobSpec, nonce: u64, at_ms: u64) -> Result<(), String> {
        self.resident
            .authority()
            .host()
            .synced()
            .author(
                &self.author,
                &mesh::MeshEvent::JobPostedV2 {
                    spec: Box::new(spec),
                    nonce,
                    at_ms,
                },
            )
            .await
            .map(|_| ())
            .map_err(|error| format!("post a Distillery job: {error}"))
    }

    /// Drive the works until `shutdown` resolves, reporting every receipt.
    pub async fn run_until<S, O>(&mut self, shutdown: S, observe: O) -> Result<(), String>
    where
        S: Future<Output = ()>,
        O: FnMut(ResidentReceipt),
    {
        self.resident
            .run_until(shutdown, observe)
            .await
            .map_err(|error| format!("resident Distillery works: {error}"))
    }

    /// Stop mesh networking, release the joined mesh, then flush blob storage.
    pub async fn close(self) -> Result<(), String> {
        self.resident
            .shutdown()
            .await
            .map_err(|error| format!("could not close the resident Distillery works: {error}"))
    }
}

/// Where this persona's trained artifacts live.
///
/// `override_root` is the owner's word and is taken as the file path itself;
/// otherwise the path is derived as
/// `<data_root>/models/<profile>/library.redb`.
///
/// The scoping is the ruling, and two places it deliberately is **not** are
/// worth naming:
///
/// - **Not the personal-graph store.** Adapters are not the owner's notes.
///   Folding them into the graph would put multi-megabyte tensors under a
///   store whose retention, sync and privacy classes were written for a
///   personal graph, and would sync them to every paired device by default.
/// - **Not under the mesh root.** A mesh's root is governed by that mesh's
///   retention policy, and its blob custody may collect at a checkpoint. A
///   trained adapter is a durable possession of the persona; it must outlive
///   any one mesh, including a mesh the owner leaves and rejoins under a new
///   id.
///
/// The profile segment goes through the same [`sanitize_profile`] the settings
/// filename uses, because a profile id reaches this process from an argument
/// or an environment variable and is about to become a directory name.
/// It is `pub` rather than crate-private because the answer is the owner's
/// business: a host that wants to tell them where their trained adapters live,
/// or to back that directory up, should not have to guess the derivation. It
/// is also answerable in a build with no `trainer` feature — the path is where
/// the library *would* be, and saying so costs nothing.
pub fn model_library_path(
    data_root: &Path,
    profile: &str,
    override_root: Option<&Path>,
) -> PathBuf {
    match override_root {
        Some(stated) => stated.to_path_buf(),
        None => data_root
            .join("models")
            .join(sanitize_profile(profile))
            .join("library.redb"),
    }
}

/// Open (creating if absent) the persona's model library.
#[cfg(feature = "trainer")]
fn open_model_library(
    data_root: &Path,
    profile: &str,
    override_root: Option<&Path>,
) -> Result<Arc<Mutex<RedbBackend>>, String> {
    let path = model_library_path(data_root, profile, override_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("prepare {}: {error}", parent.display()))?;
    }
    let store = RedbBackend::open(&path)
        .map_err(|error| format!("open the model library at {}: {error}", path.display()))?;
    Ok(Arc::new(Mutex::new(store)))
}

/// The device's lending posture, or a refusal naming what is missing.
fn load_lending(data_root: &Path) -> Result<pandect::MeshLendingSettings, String> {
    let settings = pandect::load_device_settings(data_root)
        .map_err(|error| format!("read device settings: {error}"))?;
    settings
        .and_then(|settings| settings.mesh_lending)
        .ok_or_else(|| {
            format!(
                "the Distillery works are enabled but this device has never stated a mesh \
                 lending posture: write a `mesh_lending` block in {} before running work for \
                 anyone. A composition that needs a lending posture must refuse rather than \
                 assume one.",
                pandect::device_settings_path(data_root).display()
            )
        })
}

/// Convert the device's stated lending posture into mesh's vocabulary.
///
/// This conversion lives here rather than in pandect on purpose: pandect must
/// stay mesh-free, so it stores the closed-set strings and the consumer types
/// them.
fn device_policy(lending: &pandect::MeshLendingSettings) -> Result<DevicePolicy, String> {
    Ok(DevicePolicy {
        min_idle_ms: lending.min_idle_ms,
        min_battery_pct: lending.min_battery_pct,
        max_thermal_c: lending.max_thermal_c,
        min_network: network_class(&lending.min_network, "mesh_lending.min_network")?,
        max_bandwidth_in_use_kbps: lending.max_bandwidth_in_use_kbps,
        quiet_hours: lending.quiet_hours.map(|hours| mesh::QuietHours {
            start_hour: hours.start_hour,
            end_hour: hours.end_hour,
        }),
        max_concurrent_jobs: lending.max_concurrent_jobs,
        // Empty is still mesh's own word for "no restriction" here, but it is
        // now a value the owner chose rather than one this lane invented: an
        // empty `allowed_resources` or `accepted_checkpoints` in the device
        // settings block means the owner wrote `[]`, and pandect refuses a
        // file that omits the field entirely. Each stated entry is converted
        // into mesh's own type below, and a value that does not convert
        // refuses composition rather than being dropped.
        allowed_resources: resource_set(&lending.allowed_resources)?,
        accepted_checkpoints: checkpoint_set(&lending.accepted_checkpoints)?,
        reclaim_grace_ms: lending.reclaim_grace_ms,
        supervises_leases: lending.supervises_leases,
    })
}

/// Convert the owner's stated fallbacks into the sensor's typed form.
fn stated_conditions(lending: &pandect::MeshLendingSettings) -> Result<StatedConditions, String> {
    let stated = &lending.stated;
    Ok(StatedConditions {
        idle_ms: stated.idle_ms,
        battery_pct: stated.battery_pct,
        on_mains: stated.on_mains,
        thermal_c: stated.thermal_c,
        network: stated
            .network
            .as_deref()
            .map(|value| network_class(value, "mesh_lending.stated.network"))
            .transpose()?,
        bandwidth_in_use_kbps: stated.bandwidth_in_use_kbps,
    })
}

/// Read one link class out of pandect's closed set.
fn network_class(value: &str, field: &str) -> Result<mesh::NetworkClass, String> {
    match value.trim() {
        "offline" => Ok(mesh::NetworkClass::Offline),
        "metered" => Ok(mesh::NetworkClass::Metered),
        "wifi" => Ok(mesh::NetworkClass::Wifi),
        "wired" => Ok(mesh::NetworkClass::Wired),
        other => Err(format!(
            "{field} must be one of \"offline\", \"metered\", \"wifi\", \"wired\" (got {other:?})"
        )),
    }
}

/// Parse the owner's stated resource ids with mesh's real grammar. Pandect
/// only checked that each entry could plausibly be an identifier; this is
/// where a string that is syntactically plausible but not a valid
/// `ResourceId` is actually caught, and it is refused by the parser's own
/// message so the offending string is named rather than swallowed.
fn resource_set(ids: &[String]) -> Result<BTreeSet<mesh::ResourceId>, String> {
    ids.iter()
        .map(|id| {
            mesh::ResourceId::parse(id).map_err(|error| {
                format!(
                    "mesh_lending.allowed_resources: {id:?} is not a valid resource id: {error}"
                )
            })
        })
        .collect()
}

/// Convert the owner's stated checkpoint-class strings into mesh's enum.
///
/// Pandect's own `validate` already refuses any string outside the closed
/// set before a device settings file can be saved or loaded, which makes an
/// unknown value here unreachable through that path. This conversion is
/// still not allowed to default or silently drop one: a caller that built
/// `MeshLendingSettings` some other way (a test, a future importer) gets a
/// named refusal instead of a policy that quietly accepted less than it was
/// told to.
fn checkpoint_set(values: &[String]) -> Result<BTreeSet<mesh::CheckpointClass>, String> {
    values
        .iter()
        .map(|value| match value.as_str() {
            "restart" => Ok(mesh::CheckpointClass::Restart),
            "resumable" => Ok(mesh::CheckpointClass::Resumable),
            "non-interruptible" => Ok(mesh::CheckpointClass::NonInterruptible),
            other => Err(format!(
                "mesh_lending.accepted_checkpoints: unknown checkpoint class {other:?}"
            )),
        })
        .collect()
}

/// The cadences the owner stated, as Distillery's resident settings.
fn resident_settings(lane: &DistilleryLaneSettings) -> ResidentSettings {
    ResidentSettings {
        tick_every: Duration::from_millis(lane.tick_every_ms),
        maintenance_every: lane.maintenance_every_ms.map(Duration::from_millis),
        blob_gc_every: Duration::from_millis(lane.blob_gc_every_ms),
        retention: RetentionSettings {
            collect_after_checkpoint: lane.collect_after_checkpoint,
        },
    }
}

/// The retention posture the owner stated, under their governance tag.
///
/// `checkpoint_authority` is not a setting: it is this profile's derived mesh
/// author. On a single device the owner's works govern their own retention, and
/// a typed key here could only ever hand that governance to somebody else by
/// accident.
fn retention_policy(
    lane: &DistilleryLaneSettings,
    checkpoint_authority: [u8; 32],
) -> Result<MeshRetentionPolicy, String> {
    Ok(MeshRetentionPolicy {
        revision: mesh::PolicyRevision(lane.retention_revision_bytes()?),
        checkpoint_authority,
        availability: AvailabilityPolicy {
            promised_floor: parse_keep_bound(&lane.promised_floor, "distillery.promised_floor")?,
        },
        erasure: ErasurePolicy {
            privacy_ceiling: parse_keep_bound(&lane.privacy_ceiling, "distillery.privacy_ceiling")?,
            terminal_job_payload: if lane.erase_terminal_at_checkpoint {
                mesh::PayloadRule::EraseTerminalAtCheckpoint
            } else {
                mesh::PayloadRule::Keep
            },
        },
        lease: mesh::LeasePolicy {
            max_skew_ms: lane.max_skew_ms,
        },
    })
}

/// What this machine can honestly say it has.
///
/// `memory_mib` is total physical memory, read once at composition. It is a
/// ceiling rather than a live reading, and it is deliberately not "free right
/// now": [`mesh::HostFacts`] is a static offer other devices match job
/// requirements against, so a number that moved every time a browser opened
/// would make this device's offers meaningless. A job that would fit the
/// machine but not this moment is a scheduling concern, and scheduling by live
/// pressure is a later slice.
///
/// `gpu` is `false`, and that is the honest answer rather than a placeholder:
/// nothing in this composition can run a GPU job. The builtin resources are
/// CPU work, and the one resource this lane may add on top — Distillery's
/// trainer — is composed on `TrainerDevice::ndarray()`, which is why
/// `distillery.trainer.device` refuses anything but `"cpu"`. Claiming a GPU
/// would advertise work this device would then fail.
fn host_facts() -> Result<mesh::HostFacts, String> {
    Ok(mesh::HostFacts {
        memory_mib: total_memory_mib().ok_or(
            "this build cannot read total physical memory, and a device must not advertise a \
             capacity it never measured",
        )?,
        gpu: false,
    })
}

#[cfg(windows)]
fn total_memory_mib() -> Option<u32> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).ok()?,
        ..Default::default()
    };
    // SAFETY: `status` is a live, correctly sized `MEMORYSTATUSEX`; the call
    // only writes through the pointer it is given.
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    u32::try_from(status.ullTotalPhys / (1024 * 1024)).ok()
}

/// Off Windows this lane has no dependency-free reading of physical memory, so
/// it reports nothing and [`host_facts`] refuses. The same posture
/// [`crate::conditions`] takes with its sensed signals: a port that wants this
/// lane supplies a real reading first.
#[cfg(not(windows))]
fn total_memory_mib() -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use pandect::{MeshLendingSettings, QuietHoursSettings, StatedConditionSettings};

    use super::*;

    fn lending() -> MeshLendingSettings {
        MeshLendingSettings {
            min_idle_ms: 120_000,
            min_battery_pct: 40,
            max_thermal_c: 0,
            min_network: "wifi".into(),
            max_bandwidth_in_use_kbps: 0,
            quiet_hours: Some(QuietHoursSettings {
                start_hour: 22,
                end_hour: 8,
            }),
            max_concurrent_jobs: 2,
            allowed_resources: Vec::new(),
            accepted_checkpoints: Vec::new(),
            reclaim_grace_ms: 5_000,
            supervises_leases: true,
            stated: StatedConditionSettings::default(),
        }
    }

    fn lane() -> DistilleryLaneSettings {
        DistilleryLaneSettings {
            tick_every_ms: 25,
            maintenance_every_ms: None,
            blob_gc_every_ms: 1_000,
            collect_after_checkpoint: true,
            retention_revision: "2a".repeat(32),
            promised_floor: "forever".into(),
            privacy_ceiling: "until-checkpoint".into(),
            erase_terminal_at_checkpoint: true,
            max_skew_ms: 0,
            trainer: None,
        }
    }

    #[test]
    fn the_stated_posture_becomes_the_policy_with_nothing_added() {
        let policy = device_policy(&lending()).unwrap();
        assert_eq!(policy.min_idle_ms, 120_000);
        assert_eq!(policy.min_battery_pct, 40);
        assert_eq!(policy.min_network, mesh::NetworkClass::Wifi);
        assert_eq!(policy.max_concurrent_jobs, 2);
        assert_eq!(policy.reclaim_grace_ms, 5_000);
        assert!(policy.supervises_leases);
        assert_eq!(
            policy.quiet_hours,
            Some(mesh::QuietHours {
                start_hour: 22,
                end_hour: 8
            })
        );
        assert_ne!(
            policy,
            DevicePolicy::permissive(),
            "a stated posture that converted into the permissive default would \
             mean the owner's settings never reached the supervisor"
        );
    }

    #[test]
    fn a_link_class_outside_the_closed_set_is_refused_by_name() {
        let mut broken = lending();
        broken.min_network = "ethernet".into();
        let refusal = device_policy(&broken).expect_err("`ethernet` is not a link class");
        assert!(refusal.contains("min_network"), "{refusal}");

        let mut stated = lending();
        stated.stated.network = Some("fibre".into());
        let refusal = stated_conditions(&stated).expect_err("`fibre` is not a link class");
        assert!(refusal.contains("stated.network"), "{refusal}");
    }

    #[test]
    fn allowed_resources_and_accepted_checkpoints_convert_into_mesh_types() {
        let mut stated = lending();
        stated.allowed_resources = vec!["mesh.blake3/v1".into(), "mesh.echo/v1".into()];
        stated.accepted_checkpoints = vec!["restart".into(), "resumable".into()];

        let policy = device_policy(&stated).unwrap();
        assert_eq!(
            policy.allowed_resources,
            BTreeSet::from([
                mesh::ResourceId::parse("mesh.blake3/v1").unwrap(),
                mesh::ResourceId::parse("mesh.echo/v1").unwrap(),
            ])
        );
        assert_eq!(
            policy.accepted_checkpoints,
            BTreeSet::from([
                mesh::CheckpointClass::Restart,
                mesh::CheckpointClass::Resumable,
            ])
        );
    }

    #[test]
    fn an_unparseable_resource_id_is_refused_by_the_parsers_own_message() {
        let mut stated = lending();
        stated.allowed_resources = vec!["Not A Valid Id".into()];
        let refusal =
            device_policy(&stated).expect_err("`Not A Valid Id` is not a valid resource id");
        assert!(refusal.contains("Not A Valid Id"), "{refusal}");
    }

    #[test]
    fn an_unknown_checkpoint_string_is_refused_by_name() {
        let mut stated = lending();
        stated.accepted_checkpoints = vec!["paused".into()];
        let refusal = device_policy(&stated).expect_err("`paused` is not a checkpoint class");
        assert!(refusal.contains("paused"), "{refusal}");
    }

    #[test]
    fn a_thermal_limit_without_a_thermal_reading_cannot_compose() {
        // The composition-time half of the same refusal the receipt proves end
        // to end: thermal is never sensed, so an enabled thermal rule needs a
        // stated temperature or it rests on nothing.
        let mut hot = lending();
        hot.max_thermal_c = 85;
        let policy = device_policy(&hot).unwrap();
        let sensor = DeviceConditionSensor::new(stated_conditions(&hot).unwrap());
        let refusal = validate_policy_coverage(&policy, &sensor.coverage())
            .expect_err("a thermal limit on a device with no thermometer is not a policy");
        assert!(refusal.contains("thermal"), "{refusal}");

        hot.stated.thermal_c = Some(45);
        let sensor = DeviceConditionSensor::new(stated_conditions(&hot).unwrap());
        assert_eq!(
            validate_policy_coverage(&policy, &sensor.coverage()),
            Ok(()),
            "the owner's word is a provenance, so stating one satisfies the rule"
        );
    }

    #[test]
    fn the_retention_policy_carries_the_owners_revision_and_self_authority() {
        let author = [0x7c; 32];
        let policy = retention_policy(&lane(), author).unwrap();
        assert_eq!(policy.revision, mesh::PolicyRevision([0x2a; 32]));
        assert_eq!(
            policy.checkpoint_authority, author,
            "the works govern their own retention on a single device"
        );
        assert_eq!(policy.availability.promised_floor, mesh::KeepBound::Forever);
        assert_eq!(
            policy.erasure.privacy_ceiling,
            mesh::KeepBound::UntilCheckpoint
        );
        assert_eq!(
            policy.erasure.terminal_job_payload,
            mesh::PayloadRule::EraseTerminalAtCheckpoint
        );
        assert_eq!(policy.lease.max_skew_ms, 0);
    }

    #[test]
    fn the_cadences_are_exactly_the_ones_the_owner_stated() {
        let settings = resident_settings(&lane());
        assert_eq!(settings.tick_every, Duration::from_millis(25));
        assert_eq!(settings.maintenance_every, None);
        assert_eq!(settings.blob_gc_every, Duration::from_millis(1_000));
        assert!(settings.retention.collect_after_checkpoint);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn the_model_library_is_persona_scoped_and_lives_outside_every_mesh() {
        let data_root = Path::new("C:").join("djinn").join("data");
        let derived = model_library_path(&data_root, "works", None);
        assert_eq!(
            derived,
            data_root.join("models").join("works").join("library.redb"),
            "the derived path is <data_root>/models/<profile>/library.redb"
        );
        assert!(
            !derived.starts_with(data_root.join("distillery")),
            "a trained adapter must not sit under a mesh root whose retention \
             policy could collect it: {}",
            derived.display()
        );
        assert_ne!(
            model_library_path(&data_root, "works", None),
            model_library_path(&data_root, "burner", None),
            "two faces on one device do not share a model library"
        );
    }

    #[test]
    fn a_profile_that_could_escape_the_data_root_is_sanitised_into_one_segment() {
        let data_root = Path::new("C:").join("djinn").join("data");
        let escaped = model_library_path(&data_root, "../../evil", None);
        assert_eq!(
            escaped,
            data_root
                .join("models")
                .join("______evil")
                .join("library.redb"),
            "a profile id arrives from an argument and is about to be a directory name"
        );
    }

    #[test]
    fn a_stated_library_root_is_taken_as_the_owners_word() {
        let data_root = Path::new("C:").join("djinn").join("data");
        let stated = Path::new("D:").join("archive").join("adapters.redb");
        assert_eq!(
            model_library_path(&data_root, "works", Some(&stated)),
            stated,
            "an override is a path the owner chose, not a hint to be re-derived"
        );
    }

    #[cfg(windows)]
    #[test]
    fn this_machine_reports_a_real_memory_size_and_no_gpu() {
        let facts = host_facts().expect("Windows can read physical memory");
        assert!(
            facts.memory_mib >= 512,
            "a machine running this test has more than half a gigabyte: {}",
            facts.memory_mib
        );
        assert!(
            !facts.gpu,
            "nothing in this composition can run a GPU job, so nothing may claim one"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn a_build_that_cannot_measure_memory_refuses_to_advertise_one() {
        assert!(host_facts().is_err());
    }
}
