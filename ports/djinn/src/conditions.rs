// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What this device can honestly say about itself.
//!
//! `mere-mesh` compares a [`DevicePolicy`] against a [`DeviceConditions`]
//! snapshot and never asks where the snapshot came from; `mesh-host` names the
//! seam ([`ConditionSource`]) and ships a hand-set stub. This module is the
//! real reading for the desktop resident.
//!
//! Three rules shape it:
//!
//! 1. **Sense what Windows will tell us without COM.** Idle time, power, and
//!    link class come from plain Win32 calls — `GetLastInputInfo`,
//!    `GetSystemPowerStatus`, `GetAdaptersAddresses`. No WMI, no WinRT, no COM
//!    apartment, because a desktop resident should not pay for one to read a
//!    battery.
//! 2. **Take the owner's word for the rest.** Package temperature and
//!    in-use bandwidth need instrumentation this lane does not have, so they
//!    arrive as [`StatedConditions`] — values the device's owner wrote down.
//! 3. **Never invent the difference.** Every signal carries a
//!    [`SignalProvenance`], and a signal that is neither sensed nor stated is
//!    [`Absent`](SignalProvenance::Absent) — reported as the value that makes
//!    its policy rule *withhold*, never as a plausible-looking number.
//!
//! Rule 3 is only as good as its enforcement, which is why
//! [`validate_policy_coverage`] exists: it refuses, at composition time, a
//! policy whose enabled rules rest on absent signals. Without it, "never
//! fabricate" would be an aspiration; with it, a device physically cannot be
//! configured to decide on a number nobody measured.
//!
//! ## Known approximation: `local_hour` off Windows
//!
//! `local_hour` has no provenance field because on Windows it is always
//! sensed, from `GetLocalTime`. The `cfg(not(windows))` path has no
//! dependency-free route to a *local* hour in `std`, so it reports the **UTC**
//! hour. That path exists to keep this module compiling and testable
//! cross-platform; a real non-Windows port must supply a genuine local clock
//! before [`DevicePolicy::quiet_hours`] can be trusted there.

use std::sync::Mutex;

use mesh::{DeviceConditions, DevicePolicy, NetworkClass};
use mesh_host::ConditionSource;

/// Where one condition signal came from.
///
/// The ordering of the variants is the ordering of trust: a sensed reading
/// beats a stated one, and both beat nothing at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalProvenance {
    /// Read from the operating system on this pass.
    Sensed,
    /// Supplied by the device's owner, because this build cannot sense it.
    Stated,
    /// Neither. The corresponding [`DeviceConditions`] field holds its
    /// fail-closed value and must not be believed.
    Absent,
}

/// Where each policy-relevant signal in a [`DeviceConditions`] came from.
///
/// One field per *decision*, not per struct field: `battery` covers
/// `battery_pct` and `on_mains` together, because [`DevicePolicy::withholding`]
/// reads them as one rule and neither is meaningful alone.
///
/// `local_hour` is not here — it is always sensed (see the module docs).
/// `running_jobs` is not here either: the supervisor owns that count and
/// overwrites whatever a source reports, so it is not a sensing concern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionCoverage {
    pub idle: SignalProvenance,
    pub battery: SignalProvenance,
    pub thermal: SignalProvenance,
    pub network: SignalProvenance,
    pub bandwidth: SignalProvenance,
}

impl ConditionCoverage {
    /// Nothing is known about anything. The starting point of a sensing pass,
    /// and what a sensor on a platform with no sensing at all and no stated
    /// fallbacks reports.
    pub fn all_absent() -> Self {
        Self {
            idle: SignalProvenance::Absent,
            battery: SignalProvenance::Absent,
            thermal: SignalProvenance::Absent,
            network: SignalProvenance::Absent,
            bandwidth: SignalProvenance::Absent,
        }
    }
}

impl Default for ConditionCoverage {
    fn default() -> Self {
        Self::all_absent()
    }
}

/// What the device's owner has said about the signals this build cannot sense.
///
/// Every field is optional and every field is a *fallback*: a sensed reading
/// wins, with one deliberate exception documented on
/// [`DeviceConditionSensor::sense`] — a stated [`NetworkClass::Metered`]
/// downgrades a sensed link, because metering is invisible to Win32.
///
/// The device settings file is not read here. Loading it and converting it to
/// this typed form belongs to the settings lane; this module takes the result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatedConditions {
    pub idle_ms: Option<u64>,
    pub battery_pct: Option<u8>,
    pub on_mains: Option<bool>,
    pub thermal_c: Option<u16>,
    pub network: Option<NetworkClass>,
    pub bandwidth_in_use_kbps: Option<u32>,
}

// ─── Fail-closed values for absent signals ──────────────────────────────────
//
// Defense in depth behind `validate_policy_coverage`. Each constant is the
// value that makes its rule in `DevicePolicy::withholding` fire, so that if an
// absent signal ever reaches a live policy check anyway, the device declines
// to lend rather than lending on a number nobody measured. The direction of
// each rule is quoted next to the choice, because "the safe extreme" is the
// opposite end of the range for a floor than for a ceiling.

/// `idle_ms < min_idle_ms` withholds, so the *smallest* value is fail-closed:
/// an unknown-idle device reads as one whose human just touched it.
pub const ABSENT_IDLE_MS: u64 = 0;
/// `!on_mains && battery_pct < min_battery_pct` withholds, so fail-closed is
/// the *smallest* percentage **and** off mains — both, since either alone
/// leaves the rule silent.
pub const ABSENT_BATTERY_PCT: u8 = 0;
/// See [`ABSENT_BATTERY_PCT`]. `on_mains: true` would disarm the battery rule
/// entirely, which is the fail-*open* direction.
pub const ABSENT_ON_MAINS: bool = false;
/// `thermal_c >= max_thermal_c` withholds, so fail-closed is the *largest*
/// temperature — the reverse of idle and battery. A device with no thermal
/// sensor and an enabled thermal rule reads as permanently too hot.
pub const ABSENT_THERMAL_C: u16 = u16::MAX;
/// `network < min_network` withholds, so fail-closed is the *coarsest* class.
/// `Offline` is below every floor a policy can name.
pub const ABSENT_NETWORK: NetworkClass = NetworkClass::Offline;
/// `bandwidth_in_use_kbps >= max_bandwidth_in_use_kbps` withholds, so
/// fail-closed is the *largest* value: the link reads as entirely spoken for.
pub const ABSENT_BANDWIDTH_IN_USE_KBPS: u32 = u32::MAX;

/// The desktop resident's real [`ConditionSource`].
///
/// Construction runs one sensing pass, so [`coverage`](Self::coverage) is
/// meaningful before the host ever ticks — which is what lets a composer call
/// [`validate_policy_coverage`] against the policy it is about to install.
#[derive(Debug)]
pub struct DeviceConditionSensor {
    stated: StatedConditions,
    last: Mutex<ConditionCoverage>,
}

impl DeviceConditionSensor {
    /// Build a sensor over the owner's stated fallbacks and take a first
    /// reading.
    pub fn new(stated: StatedConditions) -> Self {
        let sensor = Self {
            stated,
            last: Mutex::new(ConditionCoverage::all_absent()),
        };
        let _ = sensor.sense();
        sensor
    }

    /// The provenance of the most recent sensing pass.
    pub fn coverage(&self) -> ConditionCoverage {
        *self.last.lock().expect("condition coverage lock")
    }

    /// The owner-stated fallbacks this sensor was built with.
    pub fn stated(&self) -> StatedConditions {
        self.stated
    }

    /// Take a reading, and say where every part of it came from.
    ///
    /// ## Battery: the one permitted partial reading
    ///
    /// `GetSystemPowerStatus` reports mains and percentage independently, and
    /// either can come back as "unknown" (`255`). The battery rule is
    /// `!on_mains && battery_pct < min_battery_pct`, so `on_mains == true`
    /// short-circuits it: when mains is sensed **true** the rule's outcome does
    /// not depend on the percentage at all, and a machine with no battery at
    /// all (`BatteryLifePercent == 255` on a desktop) is fully decided.
    ///
    /// That case is reported as `Sensed` with `battery_pct: 0`. Reporting
    /// `100` there — the number that "looks right" for a plugged-in desktop —
    /// would be fabrication, and would read as a healthy battery to any future
    /// caller that looks at the percentage without the mains flag. `0` cannot
    /// flip a decision today (the mains flag short-circuits ahead of it) and is
    /// the conservative extreme if a later rule ever reads it directly.
    ///
    /// Every other combination is *not* decided: mains unknown, or mains sensed
    /// false with the percentage unknown, both leave the rule's answer resting
    /// on a number we do not have. Those fall through to the stated values, and
    /// then to `Absent`.
    ///
    /// ## Network: stated values only move downward
    ///
    /// Win32 cannot see metering — that lives behind WinRT's
    /// `NetworkInformation`. So an owner who knows their Wi-Fi is a phone
    /// hotspot states [`NetworkClass::Metered`], and that **wins over** a
    /// sensed `Wifi`, because the classes are ordered and the stated one is
    /// coarser. The rule is one-directional: the resolved class is the *lower*
    /// of sensed and stated, so a stated value can only ever downgrade, and a
    /// sensed link never upgrades what the owner said. A sensed `Offline` still
    /// beats a stated `Wifi`, because an unplugged cable is a real observation.
    pub fn sense(&self) -> (DeviceConditions, ConditionCoverage) {
        let stated = self.stated;

        let (idle_ms, idle) = match sensed::idle_ms() {
            Some(ms) => (ms, SignalProvenance::Sensed),
            None => match stated.idle_ms {
                Some(ms) => (ms, SignalProvenance::Stated),
                None => (ABSENT_IDLE_MS, SignalProvenance::Absent),
            },
        };

        let (sensed_mains, sensed_pct) = sensed::power();
        let (battery_pct, on_mains, battery) = match decided_battery(sensed_mains, sensed_pct) {
            Some((pct, mains)) => (pct, mains, SignalProvenance::Sensed),
            // A sensed half plus a stated half is still only as trustworthy as
            // the stated half, so the weaker provenance is the one recorded.
            None => match decided_battery(
                sensed_mains.or(stated.on_mains),
                sensed_pct.or(stated.battery_pct),
            ) {
                Some((pct, mains)) => (pct, mains, SignalProvenance::Stated),
                None => (
                    ABSENT_BATTERY_PCT,
                    ABSENT_ON_MAINS,
                    SignalProvenance::Absent,
                ),
            },
        };

        let (thermal_c, thermal) = match stated.thermal_c {
            // Not sensed in v1: package temperature needs WMI/COM, which this
            // lane deliberately does not open. Deferred, not forgotten.
            Some(c) => (c, SignalProvenance::Stated),
            None => (ABSENT_THERMAL_C, SignalProvenance::Absent),
        };

        let (network, network_provenance) = merge_network(sensed::network(), stated.network);

        let (bandwidth_in_use_kbps, bandwidth) = match stated.bandwidth_in_use_kbps {
            // Not sensed in v1: a real number means sampling interface counters
            // over an interval, which is a job for a background sampler rather
            // than a per-tick reading.
            Some(kbps) => (kbps, SignalProvenance::Stated),
            None => (ABSENT_BANDWIDTH_IN_USE_KBPS, SignalProvenance::Absent),
        };

        let coverage = ConditionCoverage {
            idle,
            battery,
            thermal,
            network: network_provenance,
            bandwidth,
        };
        let conditions = DeviceConditions {
            idle_ms,
            battery_pct,
            on_mains,
            thermal_c,
            network,
            bandwidth_in_use_kbps,
            local_hour: sensed::local_hour(),
            // The supervisor owns this and overwrites it; a source that
            // guessed would let a device talk past its own concurrency limit.
            running_jobs: 0,
        };

        *self.last.lock().expect("condition coverage lock") = coverage;
        (conditions, coverage)
    }
}

impl ConditionSource for DeviceConditionSensor {
    fn conditions(&self) -> DeviceConditions {
        self.sense().0
    }
}

/// The `(battery_pct, on_mains)` pair, when the battery rule's answer is fully
/// determined by what we have — and `None` when it is not.
///
/// See [`DeviceConditionSensor::sense`] for why `Some(true)` needs no
/// percentage and why the placeholder is `0` rather than `100`.
fn decided_battery(on_mains: Option<bool>, battery_pct: Option<u8>) -> Option<(u8, bool)> {
    match on_mains {
        Some(true) => Some((battery_pct.unwrap_or(ABSENT_BATTERY_PCT), true)),
        Some(false) => battery_pct.map(|pct| (pct, false)),
        None => None,
    }
}

/// Resolve a sensed link class against a stated one. Stated values move the
/// answer downward only; see [`DeviceConditionSensor::sense`].
fn merge_network(
    sensed: Option<NetworkClass>,
    stated: Option<NetworkClass>,
) -> (NetworkClass, SignalProvenance) {
    match (sensed, stated) {
        (Some(sensed), Some(stated)) if stated < sensed => (stated, SignalProvenance::Stated),
        (Some(sensed), _) => (sensed, SignalProvenance::Sensed),
        (None, Some(stated)) => (stated, SignalProvenance::Stated),
        (None, None) => (ABSENT_NETWORK, SignalProvenance::Absent),
    }
}

/// Refuse a policy whose enabled rules rest on signals nobody can supply.
///
/// This is what turns "never fabricate" from an aspiration into a checked
/// property. The sensor's fail-closed defaults keep an absent signal from
/// *lending* wrongly, but a device that silently never lends because its owner
/// enabled a thermal limit on a machine with no thermal sensor is a bug that
/// looks like bad luck. Called at composition time, this names it instead.
///
/// A rule is *enabled* exactly as [`DevicePolicy::withholding`] treats it:
/// a positive threshold, or a network floor above [`NetworkClass::Offline`].
/// `quiet_hours` needs nothing beyond `local_hour`, which is always sensed, so
/// it is never a reason to refuse.
pub fn validate_policy_coverage(
    policy: &DevicePolicy,
    coverage: &ConditionCoverage,
) -> Result<(), String> {
    let rules = [
        (policy.min_idle_ms > 0, "min_idle_ms", "idle", coverage.idle),
        (
            policy.min_battery_pct > 0,
            "min_battery_pct",
            "battery",
            coverage.battery,
        ),
        (
            policy.max_thermal_c > 0,
            "max_thermal_c",
            "thermal",
            coverage.thermal,
        ),
        (
            policy.min_network > NetworkClass::Offline,
            "min_network",
            "network",
            coverage.network,
        ),
        (
            policy.max_bandwidth_in_use_kbps > 0,
            "max_bandwidth_in_use_kbps",
            "bandwidth",
            coverage.bandwidth,
        ),
    ];

    let missing: Vec<String> = rules
        .into_iter()
        .filter(|(enabled, _, _, provenance)| *enabled && *provenance == SignalProvenance::Absent)
        .map(|(_, rule, signal, _)| {
            format!(
                "policy rule `{rule}` is enabled but the `{signal}` signal is absent \
                 (this build cannot sense it and the owner has not stated a value)"
            )
        })
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing.join("; "))
    }
}

// ─── The OS-facing half ─────────────────────────────────────────────────────

#[cfg(windows)]
mod sensed {
    use mesh::NetworkClass;
    use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
    use windows::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_FRIENDLY_NAME, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
        IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211, IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL,
        IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    /// Milliseconds since the last keyboard or mouse input, or `None` if the
    /// call failed (it does, in session-0 services and some lockscreen states).
    pub(super) fn idle_ms() -> Option<u64> {
        let mut info = LASTINPUTINFO {
            cbSize: u32::try_from(size_of::<LASTINPUTINFO>()).ok()?,
            dwTime: 0,
        };
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            return None;
        }
        let now = unsafe { GetTickCount64() };
        // `dwTime` is a 32-bit tick count that wraps every ~49.7 days; the
        // 64-bit clock does not. Subtracting one from the other directly would
        // report a ~49-day idle the first time the low word wrapped past the
        // last input, which is exactly the reading that would make a device
        // lend while its owner was typing. Do the subtraction in 32 bits so it
        // wraps the same way the counter does.
        let now_low = now as u32;
        Some(u64::from(now_low.wrapping_sub(info.dwTime)))
    }

    /// `(on_mains, battery_pct)`, each `None` when Windows says it does not
    /// know. `255` is the documented "unknown" for both fields.
    pub(super) fn power() -> (Option<bool>, Option<u8>) {
        let mut status = SYSTEM_POWER_STATUS::default();
        if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
            return (None, None);
        }
        let on_mains = match status.ACLineStatus {
            0 => Some(false),
            1 => Some(true),
            // 255 is "unknown"; anything else is undocumented and treated the
            // same way, because a value we cannot name is not a reading.
            _ => None,
        };
        let battery_pct = match status.BatteryLifePercent {
            255 => None,
            pct => Some(pct.min(100)),
        };
        (on_mains, battery_pct)
    }

    /// The best link class this machine actually has, `Some(Offline)` when it
    /// genuinely has none, or `None` when the class cannot be named — the call
    /// failed, or the only working links are of a kind Win32's `IfType` does
    /// not tell us how to rank (cellular, PPP). `None` falls back to the
    /// owner's stated value rather than claiming `Offline`.
    pub(super) fn network() -> Option<NetworkClass> {
        // AF_UNSPEC: both address families, so an IPv6-only link still counts.
        const AF_UNSPEC: u32 = 0;
        // 15 KB is the starting size Microsoft's own sample uses.
        const START_WORDS: usize = 15 * 1024 / size_of::<u64>();

        let flags = GAA_FLAG_INCLUDE_GATEWAYS
            | GAA_FLAG_SKIP_ANYCAST
            | GAA_FLAG_SKIP_MULTICAST
            | GAA_FLAG_SKIP_DNS_SERVER
            | GAA_FLAG_SKIP_FRIENDLY_NAME;

        // A `Vec<u64>` rather than `Vec<u8>`: the adapter list is a chain of
        // `#[repr(C)]` structs with 8-byte fields, so the buffer must be
        // 8-byte aligned and a byte vector is not.
        let mut buffer: Vec<u64> = vec![0; START_WORDS];
        // Adapters can appear between the sizing call and the filling call;
        // three retries is plenty and bounds the loop.
        for _ in 0..3 {
            let mut size = u32::try_from(size_of_val(buffer.as_slice())).ok()?;
            let rc = unsafe {
                GetAdaptersAddresses(
                    AF_UNSPEC,
                    flags,
                    None,
                    Some(buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>()),
                    &mut size,
                )
            };
            if rc == ERROR_BUFFER_OVERFLOW.0 {
                let words = usize::try_from(size).ok()? / size_of::<u64>() + 1;
                buffer.resize(words, 0);
                continue;
            }
            if rc != NO_ERROR.0 {
                return None;
            }
            // SAFETY: the call succeeded, so `buffer` holds a well-formed
            // adapter chain, and `buffer` outlives the walk.
            return unsafe { classify(buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>()) };
        }
        None
    }

    /// Walk the adapter chain and pick the best usable link.
    ///
    /// "Usable" is: operationally up, not loopback, not a tunnel, and holding
    /// at least one gateway — a link with no gateway routes nowhere, which is
    /// how a bridged virtual adapter would otherwise pass for a wired uplink.
    ///
    /// # Safety
    ///
    /// `head` must be null or the start of a valid `GetAdaptersAddresses`
    /// chain whose backing buffer outlives this call.
    unsafe fn classify(head: *const IP_ADAPTER_ADDRESSES_LH) -> Option<NetworkClass> {
        let mut best: Option<NetworkClass> = None;
        let mut unnameable = false;
        let mut adapter = head;
        while !adapter.is_null() {
            // SAFETY: the caller promises a valid chain; `Next` is either null
            // or the next element of it.
            let entry = unsafe { &*adapter };
            adapter = entry.Next.cast_const();

            if entry.OperStatus != IfOperStatusUp
                || entry.IfType == IF_TYPE_SOFTWARE_LOOPBACK
                || entry.IfType == IF_TYPE_TUNNEL
                || entry.FirstGatewayAddress.is_null()
            {
                continue;
            }
            let class = match entry.IfType {
                IF_TYPE_ETHERNET_CSMACD => NetworkClass::Wired,
                IF_TYPE_IEEE80211 => NetworkClass::Wifi,
                _ => {
                    // A real uplink we cannot rank. Saying `Offline` here would
                    // be a false statement about the world, so the whole
                    // reading defers to what the owner stated.
                    unnameable = true;
                    continue;
                }
            };
            best = Some(best.map_or(class, |seen| seen.max(class)));
        }
        match best {
            Some(class) => Some(class),
            None if unnameable => None,
            None => Some(NetworkClass::Offline),
        }
    }

    /// The local wall-clock hour, `0..24`.
    pub(super) fn local_hour() -> u8 {
        let now = unsafe { GetLocalTime() };
        (now.wHour % 24) as u8
    }
}

#[cfg(not(windows))]
mod sensed {
    use std::time::{SystemTime, UNIX_EPOCH};

    use mesh::NetworkClass;

    /// Nothing is sensed off Windows; the owner's stated value is the whole
    /// story. See the module docs.
    pub(super) fn idle_ms() -> Option<u64> {
        None
    }

    pub(super) fn power() -> (Option<bool>, Option<u8>) {
        (None, None)
    }

    pub(super) fn network() -> Option<NetworkClass> {
        None
    }

    /// The **UTC** hour, not the local one — `std` has no timezone and this
    /// crate has no date-time dependency. Documented as a known approximation
    /// in the module docs; a real non-Windows port must replace it.
    pub(super) fn local_hour() -> u8 {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        ((secs / 3_600) % 24) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage_with(thermal: SignalProvenance) -> ConditionCoverage {
        ConditionCoverage {
            thermal,
            ..ConditionCoverage::all_absent()
        }
    }

    #[test]
    fn an_enabled_rule_on_an_absent_signal_is_refused_by_name() {
        let mut policy = DevicePolicy::permissive();
        policy.max_thermal_c = 85;

        let refusal = validate_policy_coverage(&policy, &coverage_with(SignalProvenance::Absent))
            .expect_err("a thermal limit on a device with no thermal reading is not a policy");
        assert!(refusal.contains("thermal"), "{refusal}");
        assert!(refusal.contains("max_thermal_c"), "{refusal}");
    }

    #[test]
    fn the_same_absent_signal_passes_once_its_rule_is_disabled() {
        let mut policy = DevicePolicy::permissive();
        policy.max_thermal_c = 0;
        assert_eq!(
            validate_policy_coverage(&policy, &coverage_with(SignalProvenance::Absent)),
            Ok(())
        );
    }

    #[test]
    fn a_permissive_policy_needs_no_signals_at_all() {
        assert_eq!(
            validate_policy_coverage(
                &DevicePolicy::permissive(),
                &ConditionCoverage::all_absent()
            ),
            Ok(()),
            "a device that lends unconditionally decides on nothing"
        );
    }

    #[test]
    fn quiet_hours_alone_never_refuses_a_policy() {
        use mesh::QuietHours;

        let mut policy = DevicePolicy::permissive();
        policy.quiet_hours = Some(QuietHours {
            start_hour: 22,
            end_hour: 8,
        });
        assert_eq!(
            validate_policy_coverage(&policy, &ConditionCoverage::all_absent()),
            Ok(()),
            "local_hour is always sensed, so quiet hours never rest on a gap"
        );
    }

    #[test]
    fn an_owner_stated_signal_satisfies_an_enabled_rule() {
        let mut policy = DevicePolicy::permissive();
        policy.max_thermal_c = 85;
        assert_eq!(
            validate_policy_coverage(&policy, &coverage_with(SignalProvenance::Stated)),
            Ok(())
        );
    }

    #[test]
    fn every_enabled_rule_is_named_when_several_are_missing() {
        let policy = DevicePolicy::conservative();
        let refusal = validate_policy_coverage(&policy, &ConditionCoverage::all_absent())
            .expect_err("conservative() enables idle, battery, thermal and network");
        for signal in ["idle", "battery", "thermal", "network"] {
            assert!(refusal.contains(signal), "{signal} missing from: {refusal}");
        }
    }

    #[test]
    fn a_stated_metered_link_downgrades_a_sensed_one() {
        for sensed in [NetworkClass::Wifi, NetworkClass::Wired] {
            assert_eq!(
                merge_network(Some(sensed), Some(NetworkClass::Metered)),
                (NetworkClass::Metered, SignalProvenance::Stated),
                "the owner knows about metering and Win32 does not"
            );
        }
    }

    #[test]
    fn a_sensed_link_never_upgrades_a_stated_one() {
        assert_eq!(
            merge_network(Some(NetworkClass::Wired), Some(NetworkClass::Wifi)),
            (NetworkClass::Wifi, SignalProvenance::Stated)
        );
        assert_eq!(
            merge_network(Some(NetworkClass::Offline), Some(NetworkClass::Wifi)),
            (NetworkClass::Offline, SignalProvenance::Sensed),
            "an unplugged cable is a real observation and may downgrade"
        );
        assert_eq!(
            merge_network(None, Some(NetworkClass::Metered)),
            (NetworkClass::Metered, SignalProvenance::Stated)
        );
        assert_eq!(
            merge_network(Some(NetworkClass::Wifi), None),
            (NetworkClass::Wifi, SignalProvenance::Sensed)
        );
        assert_eq!(
            merge_network(None, None),
            (ABSENT_NETWORK, SignalProvenance::Absent)
        );
    }

    #[test]
    fn a_stated_metered_link_survives_a_real_sensing_pass() {
        let sensor = DeviceConditionSensor::new(StatedConditions {
            network: Some(NetworkClass::Metered),
            ..StatedConditions::default()
        });
        let conditions = sensor.conditions();
        assert!(
            conditions.network <= NetworkClass::Metered,
            "sensing must never raise the owner's ceiling, got {:?}",
            conditions.network
        );
        assert_ne!(sensor.coverage().network, SignalProvenance::Absent);
    }

    #[test]
    fn on_mains_alone_decides_the_battery_rule_without_a_percentage() {
        assert_eq!(decided_battery(Some(true), None), Some((0, true)));
        assert_eq!(decided_battery(Some(true), Some(64)), Some((64, true)));
        assert_eq!(decided_battery(Some(false), Some(64)), Some((64, false)));
        assert_eq!(
            decided_battery(Some(false), None),
            None,
            "off mains, the rule's answer is the percentage we do not have"
        );
        assert_eq!(decided_battery(None, Some(64)), None);

        // The placeholder must be the conservative extreme, not the plausible
        // one: 100 would read as a healthy battery to anything that looked at
        // the percentage without the mains flag.
        let mut policy = DevicePolicy::permissive();
        policy.min_battery_pct = 40;
        let (pct, mains) = decided_battery(Some(true), None).expect("decided");
        assert_eq!(
            policy.withholding(&DeviceConditions {
                battery_pct: pct,
                on_mains: mains,
                ..DeviceConditions::spare()
            }),
            None,
            "the invented 0 cannot flip the decision while on mains"
        );
    }

    #[test]
    fn unsensed_signals_hold_their_fail_closed_values() {
        // Thermal and bandwidth are never sensed in v1, so this half is
        // deterministic on every platform.
        let sensor = DeviceConditionSensor::new(StatedConditions::default());
        let (conditions, coverage) = sensor.sense();
        assert_eq!(coverage.thermal, SignalProvenance::Absent);
        assert_eq!(conditions.thermal_c, ABSENT_THERMAL_C);
        assert_eq!(coverage.bandwidth, SignalProvenance::Absent);
        assert_eq!(
            conditions.bandwidth_in_use_kbps,
            ABSENT_BANDWIDTH_IN_USE_KBPS
        );

        // An absent signal must make its rule fire, not sleep.
        let mut thermal_limited = DevicePolicy::permissive();
        thermal_limited.max_thermal_c = 85;
        assert!(
            thermal_limited.withholding(&conditions).is_some(),
            "an absent thermal reading must withhold, not lend"
        );
        let mut bandwidth_limited = DevicePolicy::permissive();
        bandwidth_limited.max_bandwidth_in_use_kbps = 1_000;
        assert!(bandwidth_limited.withholding(&conditions).is_some());
    }

    #[cfg(not(windows))]
    #[test]
    fn with_nothing_sensed_and_nothing_stated_every_signal_is_absent() {
        let sensor = DeviceConditionSensor::new(StatedConditions::default());
        let (conditions, coverage) = sensor.sense();
        assert_eq!(coverage, ConditionCoverage::all_absent());
        assert_eq!(conditions.idle_ms, ABSENT_IDLE_MS);
        assert_eq!(conditions.battery_pct, ABSENT_BATTERY_PCT);
        assert_eq!(conditions.on_mains, ABSENT_ON_MAINS);
        assert_eq!(conditions.thermal_c, ABSENT_THERMAL_C);
        assert_eq!(conditions.network, ABSENT_NETWORK);
        assert_eq!(
            conditions.bandwidth_in_use_kbps,
            ABSENT_BANDWIDTH_IN_USE_KBPS
        );
        assert_eq!(conditions.running_jobs, 0);

        // Every rule a policy can enable fires on that snapshot.
        assert!(
            DevicePolicy::conservative()
                .withholding(&conditions)
                .is_some(),
            "a device that knows nothing must lend nothing"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn stated_values_are_the_whole_reading_off_windows() {
        let sensor = DeviceConditionSensor::new(StatedConditions {
            idle_ms: Some(300_000),
            battery_pct: Some(72),
            on_mains: Some(false),
            thermal_c: Some(51),
            network: Some(NetworkClass::Wifi),
            bandwidth_in_use_kbps: Some(120),
        });
        let (conditions, coverage) = sensor.sense();
        assert_eq!(conditions.idle_ms, 300_000);
        assert_eq!(conditions.battery_pct, 72);
        assert!(!conditions.on_mains);
        assert_eq!(conditions.thermal_c, 51);
        assert_eq!(conditions.network, NetworkClass::Wifi);
        assert_eq!(conditions.bandwidth_in_use_kbps, 120);
        assert_eq!(
            coverage,
            ConditionCoverage {
                idle: SignalProvenance::Stated,
                battery: SignalProvenance::Stated,
                thermal: SignalProvenance::Stated,
                network: SignalProvenance::Stated,
                bandwidth: SignalProvenance::Stated,
            }
        );
        assert_eq!(
            validate_policy_coverage(&DevicePolicy::conservative(), &coverage),
            Ok(()),
            "a fully stated device satisfies every rule it enables"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_real_sensing_pass_is_shaped_like_a_reading() {
        // Shape only. Nothing here may assume anything about the machine that
        // runs it: laptop or desktop, plugged in or not, online or not.
        let sensor = DeviceConditionSensor::new(StatedConditions::default());
        let (conditions, coverage) = sensor.sense();

        assert!(conditions.local_hour < 24, "{}", conditions.local_hour);
        assert!(matches!(
            conditions.network,
            NetworkClass::Offline
                | NetworkClass::Metered
                | NetworkClass::Wifi
                | NetworkClass::Wired
        ));
        assert!(
            conditions.battery_pct <= 100,
            "a sensed percentage is a percentage: {}",
            conditions.battery_pct
        );
        assert_eq!(conditions.running_jobs, 0, "the supervisor owns this");

        // Nothing may be reported as sensed that this build cannot sense.
        assert_eq!(coverage.thermal, SignalProvenance::Absent);
        assert_eq!(coverage.bandwidth, SignalProvenance::Absent);
        // And a signal reported as sensed must not be holding a fail-closed
        // placeholder it never measured.
        if coverage.battery == SignalProvenance::Sensed && !conditions.on_mains {
            assert!(conditions.battery_pct <= 100);
        }
        assert_eq!(
            sensor.coverage(),
            coverage,
            "coverage records the last pass"
        );

        // Positive control. Every assertion above holds just as well when
        // every Win32 call silently fails and the whole reading is Absent —
        // which is exactly how this module would rot into dead code without
        // anyone noticing. On Windows at least one signal must genuinely come
        // back sensed.
        assert!(
            [coverage.idle, coverage.battery, coverage.network].contains(&SignalProvenance::Sensed),
            "nothing was sensed on Windows, so the Win32 layer is not working: {coverage:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_sensed_idle_reading_is_not_a_wrapped_tick_count() {
        // The 32-bit wrap bug reports ~49.7 days of idleness. A test process
        // that just started cannot legitimately be idle that long, and the
        // bound is loose enough that a genuinely long-idle machine still
        // passes: the failure it catches is 4_294_967_295 ms give or take.
        const NEAR_WRAP_MS: u64 = 40 * 24 * 60 * 60 * 1_000;
        let sensor = DeviceConditionSensor::new(StatedConditions::default());
        let (conditions, coverage) = sensor.sense();
        if coverage.idle == SignalProvenance::Sensed {
            assert!(
                conditions.idle_ms < NEAR_WRAP_MS,
                "idle_ms {} looks like a 32-bit tick wrap",
                conditions.idle_ms
            );
        }
    }
}
