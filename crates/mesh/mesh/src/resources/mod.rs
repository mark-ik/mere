// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The adapters this crate ships.
//!
//! Nothing here is privileged: a built-in adapter registers through the same
//! [`ResourceRegistry::register`](crate::registry::ResourceRegistry::register)
//! a host or test uses, and adding one touches neither `wire.rs` nor
//! `JobBoard::fold`.

pub mod delayed;
pub mod legacy;
pub mod lexical;
pub mod lexical_codec;

use std::sync::Arc;

pub use delayed::DelayedResource;
pub use legacy::{Blake3Resource, EchoResource};
pub use lexical::LexicalEmbedResource;
pub use lexical_codec::{CodecError, LexicalBatch, LexicalVectors};

use crate::ident::ResourceId;
use crate::registry::{RegistryError, ResourceRegistry};
use crate::wire::JobKind;

/// The resource id an M1 [`JobKind`] maps onto. Legacy asks and V2 asks reach
/// the same adapter.
pub fn legacy_resource_id(kind: JobKind) -> ResourceId {
    let raw = match kind {
        JobKind::Echo => "mesh.echo/v1",
        JobKind::Blake3 => "mesh.blake3/v1",
    };
    ResourceId::parse(raw).expect("built-in resource id is well formed")
}

/// Register everything this crate ships.
///
/// [`DelayedResource`] is included deliberately: the lease plan gates any
/// long-running GPU or remote adapter behind a real owner-reclaim receipt, and
/// this is the resource that receipt runs against. Its cost is bounded by the
/// posted spec, and a device that does not want to offer it says so through
/// [`DevicePolicy::allowed_resources`](crate::policy::DevicePolicy).
pub fn register_builtin(registry: &mut ResourceRegistry) -> Result<(), RegistryError> {
    registry.register(Arc::new(EchoResource::new()))?;
    registry.register(Arc::new(Blake3Resource::new()))?;
    registry.register(Arc::new(LexicalEmbedResource::new()))?;
    registry.register(Arc::new(DelayedResource::default()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_registers_under_the_id_it_declares() {
        let registry = ResourceRegistry::builtin();
        assert_eq!(registry.len(), 4);
        for id in registry.resources() {
            let adapter = registry.get(id).expect("keyed by its own id");
            assert_eq!(&adapter.descriptor().resource, id);
            assert!(adapter.descriptor().implementation.validate().is_ok());
        }
        assert!(registry.get(&legacy_resource_id(JobKind::Echo)).is_some());
        assert!(registry.get(&legacy_resource_id(JobKind::Blake3)).is_some());
    }
}
