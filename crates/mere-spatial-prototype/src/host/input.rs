// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Input dispatch on [`super::SubstrateHost`].
//!
//! Split out of `host.rs` to keep the parent module under the
//! workspace's 600-LOC ceiling. `deliver_input` (target-by-identity)
//! and `deliver_input_at` (hit-test from host coordinates) live here,
//! along with the pointer-position rewriter helper.

use kurbo::{Affine, Point};
use mere_renderer_registry::{
    CompositionMode, DispatchError, InputDisposition, InputEvent, NodeIdentity,
};

use crate::scene::SubstrateScene;

use super::SubstrateHost;

impl SubstrateHost {
    /// Deliver an input event to a specific node by identity.
    ///
    /// Resolves the node's renderer, dispatches based on
    /// `composition_mode()`. For `InScenePaint` renderers this calls
    /// the registry's in-scene input helper; for `EmbeddedFrame` the
    /// host's cached `ProducerHandle` is needed (returns
    /// `Ok(Some(Passthrough))` if no producer has been ensured yet —
    /// hosts typically render before delivering input). Overlay-mode
    /// nodes pass through (not yet wired).
    ///
    /// The event's coordinates are assumed to already be in tile-local
    /// space; use [`Self::deliver_input_at`] when the caller has
    /// host-space coordinates and wants hit-test + translation in one
    /// step.
    pub fn deliver_input(
        &mut self,
        scene: &SubstrateScene,
        target: NodeIdentity,
        event: &InputEvent,
    ) -> Result<Option<InputDisposition>, DispatchError> {
        let Some(node) = scene.get(target) else {
            return Ok(None);
        };
        let node_ref = node.as_ref();
        let Some(id) = self.registry.select(&node_ref) else {
            return Ok(None);
        };
        let mode = self
            .registry
            .get(&id)
            .expect("selector returned id not in registry")
            .composition_mode();
        match mode {
            CompositionMode::InScenePaint => {
                self.registry.deliver_in_scene_input(&node_ref, event)
            }
            CompositionMode::EmbeddedFrame => {
                let Some(&(ref stored_id, handle)) = self.producers.get(&target) else {
                    return Ok(Some(InputDisposition::Passthrough));
                };
                if stored_id != &id {
                    return Ok(Some(InputDisposition::Passthrough));
                }
                let renderer = self
                    .registry
                    .get_mut(&id)
                    .expect("selector returned id not in registry");
                let Some(producer) = renderer.as_embedded_frame() else {
                    return Err(DispatchError::WrongCompositionMode { id });
                };
                Ok(Some(producer.deliver_input(handle, event)))
            }
            CompositionMode::Overlay => Ok(Some(InputDisposition::Passthrough)),
        }
    }

    /// Spatial input router: hit-test `host_pos` against `scene`,
    /// translate the position to the hit node's tile-local coordinates,
    /// and dispatch via [`Self::deliver_input`].
    ///
    /// `host_pos` is the source of truth for the dispatched event's
    /// position — any `position` field on `event` is overwritten. The
    /// caller passes `event` to carry the non-positional payload (kind,
    /// modifiers, etc.); fields like `InputEvent::Pointer { position:
    /// Point::ZERO, ... }` are typical.
    ///
    /// Returns:
    /// - `Ok(None)` if no node was hit at `host_pos`, or if a hit node
    ///   has no registered renderer.
    /// - `Ok(Some(disposition))` on successful dispatch.
    /// - `Err(DispatchError)` for renderer composition-mode bugs.
    pub fn deliver_input_at(
        &mut self,
        scene: &SubstrateScene,
        host_pos: Point,
        event: &InputEvent,
    ) -> Result<Option<InputDisposition>, DispatchError> {
        // Pointer routing targets nodes specifically — edges don't have
        // registry-resolved renderers to deliver_input through. Hosts
        // wanting edge-click semantics call `host.scene_pos_from_host`
        // and `scene.hit_test` themselves.
        //
        // Pull the click back through the camera into scene space, then
        // hit-test. The pointer event's final position lands in
        // tile-local coordinates via the (camera * placement) composite.
        let scene_pos = self.scene_pos_from_host(host_pos);
        let Some(target) = scene.hit_test_node(scene_pos) else {
            return Ok(None);
        };
        let Some(node) = scene.get(target) else {
            return Ok(None);
        };
        let effective_transform = self.camera * node.placement.transform;
        let local_event = rewrite_pointer_position(event, host_pos, effective_transform);
        self.deliver_input(scene, target, &local_event)
    }
}

/// For `InputEvent::Pointer`, replace the position with
/// `node_transform.inverse() * host_pos`. Non-pointer events pass
/// through unchanged. Degenerate transforms leave `host_pos`
/// unchanged in the dispatched event.
fn rewrite_pointer_position(
    event: &InputEvent,
    host_pos: Point,
    node_transform: Affine,
) -> InputEvent {
    if let InputEvent::Pointer {
        kind, modifiers, ..
    } = event
    {
        let local = if node_transform.determinant() == 0.0 {
            host_pos
        } else {
            node_transform.inverse() * host_pos
        };
        return InputEvent::Pointer {
            position: local,
            kind: *kind,
            modifiers: *modifiers,
        };
    }
    event.clone()
}
