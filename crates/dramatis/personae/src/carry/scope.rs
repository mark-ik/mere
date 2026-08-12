// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The capability-scope convention for device grants.
//!
//! A device grant is a delegation like any other: a persona delegating a
//! scoped, expiring capability to a device it controls. This module fixes how
//! such a grant addresses itself in [`CapabilityScope`] so that every issuer
//! and every verifier builds the same scope for the same device.

use std::collections::BTreeSet;

use crate::delegation::CapabilityScope;

use super::DeviceId;

/// The application domain owning device-authority capabilities.
///
/// Separate from any persona-level domain on purpose. `attenuates` compares
/// domains before anything else, so a device grant can never be mistaken for
/// a narrowing of an unrelated persona capability.
pub const DEVICE_AUTHORITY_DOMAIN: &str = "mere.device";

/// The path prefix every device scope carries.
///
/// Device grants have no path dimension: the grant addresses a device, and
/// the action set carries the whole capability. This is a fixed placeholder
/// so the scope satisfies `CapabilityScope`'s well-formedness rule.
///
/// It is a leaf rather than a root. `personae::delegation::path_covers` only
/// extends a prefix when the remainder begins with `/`, so this value covers
/// itself and nothing beneath it. Giving device capabilities a real path
/// dimension later means choosing a different prefix, not nesting under this
/// one.
pub const DEVICE_SCOPE_PATH: &str = "/";

/// Act as the persona: sign and present identity on its behalf.
pub const ACTION_IDENTITY_ACT: &str = "identity.act";
/// Read already-issued private-lane content.
pub const ACTION_PRIVATE_READ: &str = "private.read";
/// Carry traffic outward as an egress or availability anchor.
pub const ACTION_TRANSPORT_EGRESS: &str = "transport.egress";

/// Build the capability scope addressing one device's grant.
pub fn device_capability_scope<I, S>(device: DeviceId, actions: I) -> CapabilityScope
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    CapabilityScope {
        domain: DEVICE_AUTHORITY_DOMAIN.to_string(),
        resource: device.as_uuid().as_bytes().to_vec(),
        path_prefix: DEVICE_SCOPE_PATH.to_string(),
        actions: actions.into_iter().map(Into::into).collect::<BTreeSet<_>>(),
    }
}

#[cfg(test)]
mod tests {
    use crate::delegation::path_covers;

    use super::*;

    fn device() -> DeviceId {
        DeviceId::from_uuid(uuid::Uuid::from_u128(0x2026_0812))
    }

    /// `attenuates` runs `is_well_formed` on both sides, so a scope that
    /// attenuates itself is a scope this grammar accepts.
    #[test]
    fn a_device_scope_is_well_formed() {
        let scope = device_capability_scope(device(), [ACTION_TRANSPORT_EGRESS]);
        assert!(scope.attenuates(&scope));
    }

    #[test]
    fn dropping_an_action_attenuates() {
        let parent =
            device_capability_scope(device(), [ACTION_IDENTITY_ACT, ACTION_TRANSPORT_EGRESS]);
        let child = device_capability_scope(device(), [ACTION_TRANSPORT_EGRESS]);
        assert!(child.attenuates(&parent));
        assert!(!parent.attenuates(&child));
    }

    #[test]
    fn a_different_device_never_attenuates() {
        let parent = device_capability_scope(device(), [ACTION_TRANSPORT_EGRESS]);
        let other = device_capability_scope(DeviceId::new(), [ACTION_TRANSPORT_EGRESS]);
        assert!(!other.attenuates(&parent));
    }

    #[test]
    fn a_foreign_domain_never_attenuates() {
        let parent = device_capability_scope(device(), [ACTION_TRANSPORT_EGRESS]);
        let mut impostor = parent.clone();
        impostor.domain = "moot".into();
        assert!(!impostor.attenuates(&parent));
    }

    /// Pins the surprise rather than trusting it: the device scope path is a
    /// leaf. Anyone who later reads `"/"` as a root and nests beneath it gets
    /// a scope that covers nothing, and this test says so first.
    #[test]
    fn the_device_scope_path_is_a_leaf_not_a_root() {
        assert!(path_covers(DEVICE_SCOPE_PATH, DEVICE_SCOPE_PATH));
        assert!(!path_covers(DEVICE_SCOPE_PATH, "/anything"));
    }
}
