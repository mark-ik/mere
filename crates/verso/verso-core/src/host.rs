/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable viewer-surface lifecycle seam.

use crate::surface::TileId;

/// Host-owned viewer surface lifecycle.
///
/// The registry/bookkeeping type is generic because the portable core cannot
/// name the shell crate's concrete viewer-surface registry.
pub trait ViewerSurfaceHost<Registry> {
    fn allocate_surface(
        &mut self,
        registry: &mut Registry,
        tile: TileId,
    ) -> Result<(), ViewerSurfaceError>;

    fn retire_surface(&mut self, registry: &mut Registry, tile: TileId);

    fn has_surface(&self, registry: &Registry, tile: TileId) -> bool;
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
    use crate::surface::TileId;

    #[derive(Default)]
    struct MockViewerSurfaceRegistry {
        surfaces: HashSet<TileId>,
    }

    #[derive(Default)]
    struct MockViewerSurfaceHost;

    impl ViewerSurfaceHost<MockViewerSurfaceRegistry> for MockViewerSurfaceHost {
        fn allocate_surface(
            &mut self,
            registry: &mut MockViewerSurfaceRegistry,
            tile: TileId,
        ) -> Result<(), ViewerSurfaceError> {
            registry.surfaces.insert(tile);
            Ok(())
        }

        fn retire_surface(&mut self, registry: &mut MockViewerSurfaceRegistry, tile: TileId) {
            registry.surfaces.remove(&tile);
        }

        fn has_surface(&self, registry: &MockViewerSurfaceRegistry, tile: TileId) -> bool {
            registry.surfaces.contains(&tile)
        }
    }

    #[test]
    fn viewer_surface_host_tracks_allocated_surfaces() {
        let mut host = MockViewerSurfaceHost;
        let mut registry = MockViewerSurfaceRegistry::default();
        let tile = TileId::new();

        host.allocate_surface(&mut registry, tile)
            .expect("surface allocation should succeed");

        assert!(host.has_surface(&registry, tile));

        host.retire_surface(&mut registry, tile);

        assert!(!host.has_surface(&registry, tile));
    }
}
