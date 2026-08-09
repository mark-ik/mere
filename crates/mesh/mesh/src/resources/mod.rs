// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The adapters this crate ships.
//!
//! Nothing here is privileged: a built-in adapter registers through the same
//! [`ResourceRegistry::register`](crate::registry::ResourceRegistry::register)
//! a host or test uses, and adding one touches neither `wire.rs` nor
//! `JobBoard::fold`.

pub mod legacy;
pub mod lexical;
pub mod lexical_codec;

use std::sync::Arc;

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
pub fn register_builtin(registry: &mut ResourceRegistry) -> Result<(), RegistryError> {
    registry.register(Arc::new(EchoResource::new()))?;
    registry.register(Arc::new(Blake3Resource::new()))?;
    registry.register(Arc::new(LexicalEmbedResource::new()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::MeshResource;

    #[test]
    fn every_builtin_registers_under_the_id_it_declares() {
        let registry = ResourceRegistry::builtin();
        assert_eq!(registry.len(), 3);
        for id in registry.resources() {
            let adapter = registry.get(id).expect("keyed by its own id");
            assert_eq!(&adapter.descriptor().resource, id);
            assert!(adapter.descriptor().implementation.validate().is_ok());
        }
        assert!(registry.get(&legacy_resource_id(JobKind::Echo)).is_some());
        assert!(registry.get(&legacy_resource_id(JobKind::Blake3)).is_some());
    }
}
