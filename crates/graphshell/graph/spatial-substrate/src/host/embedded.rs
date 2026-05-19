// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Embedded-frame producer lifecycle on [`super::SubstrateHost`].
//!
//! Split out of `host.rs` to keep the parent module under the
//! workspace's 600-LOC ceiling. Producer caches + AccessKit subtree
//! grafting + node release all run through here.

use register_renderer::{CompositionMode, NodeIdentity};

use crate::external_texture::ExternalTextureCompositor;
use crate::scene::SubstrateScene;

use super::SubstrateHost;

impl SubstrateHost {
    /// Pre-warm producer caches for every `EmbeddedFrame` node in
    /// `scene`. Useful for tests that want to exercise the substrate's
    /// `EmbeddedFrameRenderer`-aware paths (AccessKit subtree merge,
    /// `release_node`, hot-swap) without going through `render_scene`'s
    /// GPU code.
    ///
    /// Returns the number of producers newly created. Existing
    /// producers for the same `(node_identity, renderer_id)` pair are
    /// left in place.
    pub fn ensure_embedded_producers(&mut self, scene: &SubstrateScene) -> usize {
        let mut created = 0;
        let identities: Vec<NodeIdentity> = scene.iter().map(|n| n.identity).collect();
        for identity in identities {
            let Some(node) = scene.get(identity) else {
                continue;
            };
            let node_ref = node.as_ref();
            let Some(id) = self.registry.select(&node_ref) else {
                continue;
            };
            let mode = self
                .registry
                .get(&id)
                .expect("selector returned id not in registry")
                .composition_mode();
            if mode != CompositionMode::EmbeddedFrame {
                continue;
            }
            if let Some((cached_id, _)) = self.producers.get(&identity)
                && cached_id == &id
            {
                continue;
            }
            let Some(producer) = self
                .registry
                .get_mut(&id)
                .and_then(|r| r.as_embedded_frame())
            else {
                continue;
            };
            let handle = producer.ensure_producer(&node_ref);
            self.producers.insert(identity, (id.clone(), handle));
            created += 1;
        }
        created
    }

    /// Drain the pending AccessKit subtree from the
    /// `EmbeddedFrameRenderer` registered for `node_identity`, if any.
    /// Returns `None` if the node isn't in the producer cache or the
    /// renderer has no pending update this frame.
    ///
    /// Used by `accessibility::collect_accesskit_updates`. Hosts
    /// rarely call this directly.
    pub fn take_subtree_for_node(
        &mut self,
        node_identity: NodeIdentity,
    ) -> Option<accesskit::TreeUpdate> {
        let (renderer_id, handle) = {
            let entry = self.producers.get(&node_identity)?;
            (entry.0.clone(), entry.1)
        };
        let renderer = self.registry.get_mut(&renderer_id)?;
        let producer = renderer.as_embedded_frame()?;
        producer.take_accesskit_subtree(handle)
    }

    /// Release any per-node state the host holds for an embedded-frame
    /// node — the renderer's producer handle and the compositor's
    /// texture registration. Call before dropping a node from the scene
    /// to keep vello's image-override table tidy.
    pub fn release_node(
        &mut self,
        node_identity: NodeIdentity,
        compositor: &mut ExternalTextureCompositor,
        vello_renderer: &mut vello::Renderer,
    ) {
        if let Some((id, handle)) = self.producers.remove(&node_identity) {
            if let Some(renderer) = self.registry.get_mut(&id) {
                if let Some(producer) = renderer.as_embedded_frame() {
                    producer.release(handle);
                }
            }
        }
        compositor.unregister(vello_renderer, node_identity);
    }
}
