/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::marker::PhantomData;

use graphshell_core::graph::NodeKey;
use graphshell_core::viewer_host::{ViewerSurfaceError, ViewerSurfaceHost};

pub trait ViewerSurfaceRegistryHost {
    type Surface;

    fn get_or_insert_surface_with<F>(&mut self, node_key: NodeKey, create_surface: F)
    where
        F: FnOnce() -> Self::Surface;

    fn retire_surface(&mut self, node_key: NodeKey);

    fn has_surface(&self, node_key: NodeKey) -> bool;
}

impl<Surface> ViewerSurfaceRegistryHost for HashMap<NodeKey, Surface> {
    type Surface = Surface;

    fn get_or_insert_surface_with<F>(&mut self, node_key: NodeKey, create_surface: F)
    where
        F: FnOnce() -> Self::Surface,
    {
        self.entry(node_key).or_insert_with(create_surface);
    }

    fn retire_surface(&mut self, node_key: NodeKey) {
        self.remove(&node_key);
    }

    fn has_surface(&self, node_key: NodeKey) -> bool {
        self.contains_key(&node_key)
    }
}

pub struct SurfaceFactoryHost<Surface, Allocate>
where
    Allocate: FnMut() -> Surface,
{
    allocate_surface: Allocate,
    _surface: PhantomData<fn() -> Surface>,
}

impl<Surface, Allocate> SurfaceFactoryHost<Surface, Allocate>
where
    Allocate: FnMut() -> Surface,
{
    pub fn new(allocate_surface: Allocate) -> Self {
        Self {
            allocate_surface,
            _surface: PhantomData,
        }
    }
}

impl<Registry, Surface, Allocate> ViewerSurfaceHost<Registry>
    for SurfaceFactoryHost<Surface, Allocate>
where
    Registry: ViewerSurfaceRegistryHost<Surface = Surface>,
    Allocate: FnMut() -> Surface,
{
    fn allocate_surface(
        &mut self,
        registry: &mut Registry,
        node_key: NodeKey,
    ) -> Result<(), ViewerSurfaceError> {
        registry.get_or_insert_surface_with(node_key, || (self.allocate_surface)());
        Ok(())
    }

    fn retire_surface(&mut self, registry: &mut Registry, node_key: NodeKey) {
        registry.retire_surface(node_key);
    }

    fn has_surface(&self, registry: &Registry, node_key: NodeKey) -> bool {
        registry.has_surface(node_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_factory_host_allocates_once_per_node() {
        let mut next_surface = 0;
        let mut host = SurfaceFactoryHost::new(|| {
            next_surface += 1;
            next_surface
        });
        let node = NodeKey::new(4);
        let other = NodeKey::new(5);
        let mut registry = HashMap::new();

        host.allocate_surface(&mut registry, node).unwrap();
        host.allocate_surface(&mut registry, node).unwrap();
        host.allocate_surface(&mut registry, other).unwrap();

        assert_eq!(registry.len(), 2);
        assert_eq!(registry.get(&node), Some(&1));
        assert_eq!(registry.get(&other), Some(&2));
        assert!(host.has_surface(&registry, node));
    }

    #[test]
    fn surface_factory_host_retires_registry_surfaces() {
        let mut host = SurfaceFactoryHost::new(|| "surface".to_string());
        let node = NodeKey::new(8);
        let mut registry = HashMap::new();
        host.allocate_surface(&mut registry, node).unwrap();

        host.retire_surface(&mut registry, node);

        assert!(!host.has_surface(&registry, node));
        assert!(registry.is_empty());
    }
}
