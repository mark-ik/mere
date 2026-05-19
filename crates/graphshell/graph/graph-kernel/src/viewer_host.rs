/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable viewer-surface lifecycle seam.

use crate::graph::NodeKey;

/// Host-owned viewer surface lifecycle.
///
/// The registry/bookkeeping type is generic because the portable core cannot
/// name the shell crate's concrete viewer-surface registry.
pub trait ViewerSurfaceHost<Registry> {
    fn allocate_surface(
        &mut self,
        registry: &mut Registry,
        node_key: NodeKey,
    ) -> Result<(), ViewerSurfaceError>;

    fn retire_surface(&mut self, registry: &mut Registry, node_key: NodeKey);

    fn has_surface(&self, registry: &Registry, node_key: NodeKey) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerSurfaceError {
    ResourceExhausted,
    InvalidViewer,
    HostShuttingDown,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{ViewerSurfaceError, ViewerSurfaceHost};
    use crate::graph::NodeKey;

    #[derive(Default)]
    struct MockViewerSurfaceRegistry {
        surfaces: HashSet<NodeKey>,
    }

    #[derive(Default)]
    struct MockViewerSurfaceHost;

    impl ViewerSurfaceHost<MockViewerSurfaceRegistry> for MockViewerSurfaceHost {
        fn allocate_surface(
            &mut self,
            registry: &mut MockViewerSurfaceRegistry,
            node_key: NodeKey,
        ) -> Result<(), ViewerSurfaceError> {
            registry.surfaces.insert(node_key);
            Ok(())
        }

        fn retire_surface(&mut self, registry: &mut MockViewerSurfaceRegistry, node_key: NodeKey) {
            registry.surfaces.remove(&node_key);
        }

        fn has_surface(&self, registry: &MockViewerSurfaceRegistry, node_key: NodeKey) -> bool {
            registry.surfaces.contains(&node_key)
        }
    }

    #[test]
    fn viewer_surface_host_tracks_allocated_surfaces() {
        let mut host = MockViewerSurfaceHost;
        let mut registry = MockViewerSurfaceRegistry::default();
        let node_key = NodeKey::new(7);

        host.allocate_surface(&mut registry, node_key)
            .expect("surface allocation should succeed");

        assert!(host.has_surface(&registry, node_key));

        host.retire_surface(&mut registry, node_key);

        assert!(!host.has_surface(&registry, node_key));
    }
}
