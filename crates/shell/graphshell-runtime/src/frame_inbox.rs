/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral frame-bound relay set for shell-facing async signals.
//!
//! The shell subscribes long-lived registry/lifecycle signal streams once at
//! startup. Per-frame consumers want to coalesce those into either a flag (any
//! signal observed since last frame) or an ordered drain (queue all values).
//! That state is just `mpsc::Receiver<T>` plus drain helpers — no host coupling
//! — so it lives here. The shell-side spawn constructor that wires the
//! receivers to a `ControlPanel`-driven subscription stays in graphshell main.

use std::sync::mpsc::Receiver;

/// Typed frame-bound relay set for shell-facing async signals.
pub struct FrameInboxState {
    semantic_index_updates: FrameSignalRelay<usize>,
    workbench_projection_refreshes: FrameSignalRelay<()>,
    settings_route_requests: FrameSignalRelay<String>,
    profile_invalidations: FrameSignalRelay<()>,
}

impl FrameInboxState {
    pub fn new(
        semantic_index_updates: Receiver<usize>,
        workbench_projection_refreshes: Receiver<()>,
        settings_route_requests: Receiver<String>,
        profile_invalidations: Receiver<()>,
    ) -> Self {
        Self {
            semantic_index_updates: FrameSignalRelay::new(semantic_index_updates),
            workbench_projection_refreshes: FrameSignalRelay::new(workbench_projection_refreshes),
            settings_route_requests: FrameSignalRelay::new(settings_route_requests),
            profile_invalidations: FrameSignalRelay::new(profile_invalidations),
        }
    }

    pub fn take_semantic_index_refresh(&self) -> bool {
        self.semantic_index_updates.drain_flag()
    }

    pub fn take_workbench_projection_refresh(&self) -> bool {
        self.workbench_projection_refreshes.drain_flag()
    }

    pub fn take_settings_routes(&self) -> Vec<String> {
        self.settings_route_requests.drain_all()
    }

    pub fn take_profile_invalidation(&self) -> bool {
        self.profile_invalidations.drain_flag()
    }
}

struct FrameSignalRelay<T> {
    rx: Receiver<T>,
}

impl<T> FrameSignalRelay<T> {
    fn new(rx: Receiver<T>) -> Self {
        Self { rx }
    }

    fn drain_flag(&self) -> bool {
        let mut saw_update = false;
        while self.rx.try_recv().is_ok() {
            saw_update = true;
        }
        saw_update
    }

    fn drain_all(&self) -> Vec<T> {
        let mut items = Vec::new();
        while let Ok(item) = self.rx.try_recv() {
            items.push(item);
        }
        items
    }
}

#[cfg(test)]
mod tests {
    use super::FrameInboxState;
    use std::sync::mpsc::channel;

    #[test]
    fn frame_inbox_coalesces_flag_relays_per_frame() {
        let (semantic_tx, semantic_rx) = channel();
        let (projection_tx, projection_rx) = channel();
        let (settings_tx, settings_rx) = channel();
        let (profile_tx, profile_rx) = channel();
        let inbox = FrameInboxState::new(semantic_rx, projection_rx, settings_rx, profile_rx);

        semantic_tx.send(1).expect("semantic update");
        semantic_tx.send(2).expect("semantic update");
        projection_tx.send(()).expect("projection refresh");
        profile_tx.send(()).expect("profile invalidation");
        profile_tx.send(()).expect("profile invalidation");
        drop(settings_tx);

        assert!(inbox.take_semantic_index_refresh());
        assert!(!inbox.take_semantic_index_refresh());
        assert!(inbox.take_workbench_projection_refresh());
        assert!(!inbox.take_workbench_projection_refresh());
        assert!(inbox.take_profile_invalidation());
        assert!(!inbox.take_profile_invalidation());
    }

    #[test]
    fn frame_inbox_drains_settings_routes_in_order() {
        let (semantic_tx, semantic_rx) = channel();
        let (projection_tx, projection_rx) = channel();
        let (settings_tx, settings_rx) = channel();
        let (profile_tx, profile_rx) = channel();
        let inbox = FrameInboxState::new(semantic_rx, projection_rx, settings_rx, profile_rx);

        drop(semantic_tx);
        drop(projection_tx);
        drop(profile_tx);
        settings_tx
            .send("verso://settings/appearance".to_string())
            .expect("settings route");
        settings_tx
            .send("verso://settings/search".to_string())
            .expect("settings route");

        assert_eq!(
            inbox.take_settings_routes(),
            vec![
                "verso://settings/appearance".to_string(),
                "verso://settings/search".to_string(),
            ]
        );
        assert!(inbox.take_settings_routes().is_empty());
    }
}
