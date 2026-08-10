// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What this device will lend, and what it is doing right now.
//!
//! [`DevicePolicy`] is configuration the user owns; [`DeviceConditions`] is a
//! reading the *host* takes and passes down. `mere-mesh` never queries the OS,
//! never asks about battery or foreground windows, and never decides rendering
//! versus compute priority — it only compares a policy against a snapshot the
//! host handed it.
//!
//! One enum answers both questions the scheduler asks, because they are the same
//! question at two moments: [`ReclaimReason`] says why this device will not take
//! new work *and* why it is handing running work back. That is deliberate — a
//! device that would refuse to start a job for a reason should not keep running
//! one for the same reason.

use std::collections::BTreeSet;

use crate::ident::ResourceId;
use crate::lease::ReclaimReason;
use crate::spec::{CheckpointClass, JobSpec};

/// Link quality, coarsest first. Ordered so a policy can name a floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetworkClass {
    Offline,
    Metered,
    Wifi,
    Wired,
}

/// A local-wall-clock window during which this device does not work. Hours are
/// host-supplied readings: the mesh has no timezone and does not want one.
/// `start == end` means the whole day; a window may wrap midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuietHours {
    pub start_hour: u8,
    pub end_hour: u8,
}

impl QuietHours {
    pub fn contains(&self, hour: u8) -> bool {
        if self.start_hour == self.end_hour {
            return true;
        }
        if self.start_hour < self.end_hour {
            hour >= self.start_hour && hour < self.end_hour
        } else {
            hour >= self.start_hour || hour < self.end_hour
        }
    }
}

/// What the user has agreed to lend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevicePolicy {
    /// Work only after this much continuous idleness. `0` lends immediately.
    pub min_idle_ms: u64,
    /// Refuse below this battery percentage unless on mains.
    pub min_battery_pct: u8,
    /// Refuse at or above this package temperature. `0` disables the check.
    pub max_thermal_c: u16,
    /// Slowest link this device will work over.
    pub min_network: NetworkClass,
    /// Refuse when this much of the link is already spoken for. `0` disables.
    pub max_bandwidth_in_use_kbps: u32,
    pub quiet_hours: Option<QuietHours>,
    pub max_concurrent_jobs: u32,
    /// Empty means "every resource this device has registered".
    pub allowed_resources: BTreeSet<ResourceId>,
    /// Which interruption promises this device is willing to take on.
    pub accepted_checkpoints: BTreeSet<CheckpointClass>,
    /// How long a [`CheckpointClass::NonInterruptible`] job may keep running
    /// after the owner has asked for the device back. The owner still wins; this
    /// only says how abrupt the handoff is.
    pub reclaim_grace_ms: u64,
}

impl DevicePolicy {
    /// Lend everything, always. The starting point for tests and for a device
    /// whose owner has not configured anything yet.
    pub fn permissive() -> Self {
        Self {
            min_idle_ms: 0,
            min_battery_pct: 0,
            max_thermal_c: 0,
            min_network: NetworkClass::Offline,
            max_bandwidth_in_use_kbps: 0,
            quiet_hours: None,
            max_concurrent_jobs: u32::MAX,
            allowed_resources: BTreeSet::new(),
            accepted_checkpoints: BTreeSet::new(),
            reclaim_grace_ms: 0,
        }
    }

    /// A laptop that lends only while genuinely spare.
    pub fn conservative() -> Self {
        Self {
            min_idle_ms: 120_000,
            min_battery_pct: 40,
            max_thermal_c: 85,
            min_network: NetworkClass::Wifi,
            max_bandwidth_in_use_kbps: 0,
            quiet_hours: Some(QuietHours {
                start_hour: 22,
                end_hour: 8,
            }),
            max_concurrent_jobs: 1,
            allowed_resources: BTreeSet::new(),
            accepted_checkpoints: BTreeSet::from([
                CheckpointClass::Restart,
                CheckpointClass::Resumable,
            ]),
            reclaim_grace_ms: 5_000,
        }
    }

    /// Whether this device is already running as much as its owner allows.
    ///
    /// Separate from [`withholding`](Self::withholding) on purpose: being full
    /// is a reason not to *start* another job, never a reason to abandon one
    /// already running — a device that conflated them would reclaim its own
    /// work the moment it accepted it. [`ReclaimReason::AtCapacity`] stays in
    /// the wire vocabulary for a host that deliberately sheds load; the worker
    /// loop never produces it.
    pub fn at_capacity(&self, conditions: &DeviceConditions) -> bool {
        conditions.running_jobs >= self.max_concurrent_jobs
    }

    /// Why the device's owner wants it back, or `None` when the device is free
    /// to work.
    ///
    /// The same answer drives both refusing new work and handing running work
    /// back, so a device cannot quietly keep a job it would not have accepted.
    pub fn withholding(&self, conditions: &DeviceConditions) -> Option<ReclaimReason> {
        if conditions.idle_ms < self.min_idle_ms {
            return Some(ReclaimReason::ForegroundActivity);
        }
        if !conditions.on_mains && conditions.battery_pct < self.min_battery_pct {
            return Some(ReclaimReason::Battery);
        }
        if self.max_thermal_c > 0 && conditions.thermal_c >= self.max_thermal_c {
            return Some(ReclaimReason::Thermal);
        }
        if conditions.network < self.min_network {
            return Some(ReclaimReason::Network);
        }
        if self.max_bandwidth_in_use_kbps > 0
            && conditions.bandwidth_in_use_kbps >= self.max_bandwidth_in_use_kbps
        {
            return Some(ReclaimReason::Network);
        }
        if self
            .quiet_hours
            .is_some_and(|hours| hours.contains(conditions.local_hour))
        {
            return Some(ReclaimReason::QuietHours);
        }
        None
    }

    /// Whether this device will take on `spec` at all, independent of current
    /// conditions: the resource is allowed and the interruption promise is one
    /// the owner accepted.
    pub fn accepts(&self, spec: &JobSpec) -> bool {
        (self.allowed_resources.is_empty() || self.allowed_resources.contains(&spec.resource))
            && (self.accepted_checkpoints.is_empty()
                || self.accepted_checkpoints.contains(&spec.checkpoint))
    }
}

/// What the host observed about the device this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceConditions {
    pub idle_ms: u64,
    pub battery_pct: u8,
    pub on_mains: bool,
    pub thermal_c: u16,
    pub network: NetworkClass,
    pub bandwidth_in_use_kbps: u32,
    /// Local wall-clock hour, `0..24`.
    pub local_hour: u8,
    /// Mesh jobs this device already has in flight.
    pub running_jobs: u32,
}

impl DeviceConditions {
    /// An idle, plugged-in, cool, wired device in the middle of the afternoon.
    pub fn spare() -> Self {
        Self {
            idle_ms: u64::MAX,
            battery_pct: 100,
            on_mains: true,
            thermal_c: 40,
            network: NetworkClass::Wired,
            bandwidth_in_use_kbps: 0,
            local_hour: 14,
            running_jobs: 0,
        }
    }

    /// The same device with its human back at the keyboard.
    pub fn in_use(mut self) -> Self {
        self.idle_ms = 0;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{DeterminismClass, JobSpec};
    use proofs::BlobRef;

    fn spec(resource: &str, checkpoint: CheckpointClass) -> JobSpec {
        let mut spec = JobSpec::simple(
            ResourceId::parse(resource).unwrap(),
            "payload",
            BlobRef::blake3(b"x"),
            "result",
            64,
            DeterminismClass::Exact,
        );
        spec.checkpoint = checkpoint;
        spec
    }

    #[test]
    fn a_permissive_device_lends_under_any_conditions() {
        let policy = DevicePolicy::permissive();
        assert_eq!(policy.withholding(&DeviceConditions::spare()), None);
        assert_eq!(
            policy.withholding(&DeviceConditions::spare().in_use()),
            None
        );
        assert!(policy.accepts(&spec("mesh.echo/v1", CheckpointClass::NonInterruptible)));
    }

    #[test]
    fn each_configured_limit_names_the_reason_it_withholds() {
        let policy = DevicePolicy::conservative();
        assert_eq!(policy.withholding(&DeviceConditions::spare()), None);

        let cases = [
            (
                DeviceConditions::spare().in_use(),
                ReclaimReason::ForegroundActivity,
            ),
            (
                DeviceConditions {
                    on_mains: false,
                    battery_pct: 10,
                    ..DeviceConditions::spare()
                },
                ReclaimReason::Battery,
            ),
            (
                DeviceConditions {
                    thermal_c: 90,
                    ..DeviceConditions::spare()
                },
                ReclaimReason::Thermal,
            ),
            (
                DeviceConditions {
                    network: NetworkClass::Metered,
                    ..DeviceConditions::spare()
                },
                ReclaimReason::Network,
            ),
            (
                DeviceConditions {
                    local_hour: 23,
                    ..DeviceConditions::spare()
                },
                ReclaimReason::QuietHours,
            ),
        ];
        for (conditions, expected) in cases {
            assert_eq!(
                policy.withholding(&conditions),
                Some(expected),
                "{conditions:?}"
            );
        }
    }

    #[test]
    fn being_full_stops_new_work_without_abandoning_running_work() {
        let policy = DevicePolicy::conservative(); // max_concurrent_jobs = 1
        let busy = DeviceConditions {
            running_jobs: 1,
            ..DeviceConditions::spare()
        };
        assert!(policy.at_capacity(&busy));
        assert_eq!(
            policy.withholding(&busy),
            None,
            "a full device keeps the job it is already running"
        );
    }

    #[test]
    fn a_low_battery_on_mains_is_not_a_reason_to_stop() {
        let policy = DevicePolicy::conservative();
        assert_eq!(
            policy.withholding(&DeviceConditions {
                battery_pct: 5,
                on_mains: true,
                ..DeviceConditions::spare()
            }),
            None
        );
    }

    #[test]
    fn quiet_hours_wrap_midnight() {
        let overnight = QuietHours {
            start_hour: 22,
            end_hour: 8,
        };
        assert!(overnight.contains(23));
        assert!(overnight.contains(2));
        assert!(overnight.contains(22));
        assert!(!overnight.contains(8));
        assert!(!overnight.contains(14));

        let daytime = QuietHours {
            start_hour: 9,
            end_hour: 17,
        };
        assert!(daytime.contains(12));
        assert!(!daytime.contains(20));

        let always = QuietHours {
            start_hour: 0,
            end_hour: 0,
        };
        assert!(always.contains(3) && always.contains(15));
    }

    #[test]
    fn allowed_resources_and_checkpoint_classes_gate_acceptance() {
        let mut policy = DevicePolicy::permissive();
        policy.allowed_resources = BTreeSet::from([ResourceId::parse("mesh.echo/v1").unwrap()]);
        assert!(policy.accepts(&spec("mesh.echo/v1", CheckpointClass::Restart)));
        assert!(!policy.accepts(&spec("mesh.blake3/v1", CheckpointClass::Restart)));

        let mut fussy = DevicePolicy::permissive();
        fussy.accepted_checkpoints = BTreeSet::from([CheckpointClass::Resumable]);
        assert!(fussy.accepts(&spec("mesh.echo/v1", CheckpointClass::Resumable)));
        assert!(
            !fussy.accepts(&spec("mesh.echo/v1", CheckpointClass::NonInterruptible)),
            "a device that wants to interrupt does not take work it cannot"
        );
    }
}
