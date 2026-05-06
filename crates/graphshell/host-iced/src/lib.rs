/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Fresh iced host adapter boundary for Graphshell surfaces.

use std::collections::HashMap;
use std::sync::Arc;

use graphshell_core::graph::NodeKey;
use graphshell_host::HostToolkit;
use graphshell_runtime::{BackendViewportInPixels, HostSurfacePort};

pub const TOOLKIT: HostToolkit = HostToolkit::Iced;

pub type IcedContentCallback = Arc<dyn Fn(&(), BackendViewportInPixels) + Send + Sync>;

#[derive(Default)]
pub struct IcedSurfacePort {
    present_requests: Vec<NodeKey>,
    retire_requests: Vec<NodeKey>,
    content_callbacks: HashMap<NodeKey, IcedContentCallback>,
}

impl IcedSurfacePort {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn present_requests(&self) -> &[NodeKey] {
        &self.present_requests
    }

    pub fn retire_requests(&self) -> &[NodeKey] {
        &self.retire_requests
    }

    pub fn drain_present_requests(&mut self) -> Vec<NodeKey> {
        std::mem::take(&mut self.present_requests)
    }

    pub fn drain_retire_requests(&mut self) -> Vec<NodeKey> {
        std::mem::take(&mut self.retire_requests)
    }

    pub fn callback_count(&self) -> usize {
        self.content_callbacks.len()
    }

    pub fn has_content_callback(&self, node_key: NodeKey) -> bool {
        self.content_callbacks.contains_key(&node_key)
    }
}

impl HostSurfacePort for IcedSurfacePort {
    type BackendContext = ();

    fn present_surface(&mut self, node_key: NodeKey) {
        self.present_requests.push(node_key);
    }

    fn retire_surface(&mut self, node_key: NodeKey) {
        self.retire_requests.push(node_key);
        self.content_callbacks.remove(&node_key);
    }

    fn register_content_callback(&mut self, node_key: NodeKey, callback: IcedContentCallback) {
        self.content_callbacks.insert(node_key, callback);
    }

    fn unregister_content_callback(&mut self, node_key: NodeKey) {
        self.content_callbacks.remove(&node_key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolkit_is_iced() {
        assert_eq!(TOOLKIT, HostToolkit::Iced);
    }

    #[test]
    fn surface_port_queues_present_and_retire_requests() {
        let mut port = IcedSurfacePort::new();
        let first = NodeKey::new(1);
        let second = NodeKey::new(2);

        port.present_surface(first);
        port.present_surface(second);
        port.retire_surface(first);

        assert_eq!(port.present_requests(), &[first, second]);
        assert_eq!(port.retire_requests(), &[first]);
        assert_eq!(port.drain_present_requests(), vec![first, second]);
        assert_eq!(port.drain_retire_requests(), vec![first]);
        assert!(port.present_requests().is_empty());
        assert!(port.retire_requests().is_empty());
    }

    #[test]
    fn retiring_surface_removes_content_callback() {
        let mut port = IcedSurfacePort::new();
        let node = NodeKey::new(3);

        port.register_content_callback(node, Arc::new(|_, _| {}));
        assert!(port.has_content_callback(node));

        port.retire_surface(node);

        assert!(!port.has_content_callback(node));
        assert_eq!(port.callback_count(), 0);
    }

    #[test]
    fn unregister_content_callback_is_idempotent() {
        let mut port = IcedSurfacePort::new();
        let node = NodeKey::new(4);

        port.unregister_content_callback(node);
        port.register_content_callback(node, Arc::new(|_, _| {}));
        port.unregister_content_callback(node);
        port.unregister_content_callback(node);

        assert_eq!(port.callback_count(), 0);
    }
}
