/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable workbench/frame model and selectors.

use std::collections::HashMap;

use graphshell_core::graph::{GraphViewId, NodeKey};
use graphshell_core::pane::PaneId;
use serde::{Deserialize, Serialize};
use verso_tile::surface::SurfaceHostId;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameId(pub String);

impl FrameId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable pane-to-graph binding consumed by hosts and frame projections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneBinding {
    pub pane_id: PaneId,
    pub node: NodeKey,
    pub surface_host: Option<SurfaceHostId>,
}

/// One named frame/workbench composition.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameState {
    pub id: FrameId,
    pub label: String,
    pub root_view: Option<GraphViewId>,
    pub panes: Vec<PaneBinding>,
}

/// Select the currently active frame when it exists in the frame map.
pub fn select_active_frame<'a>(
    frames: &'a HashMap<FrameId, FrameState>,
    active_frame: Option<&FrameId>,
) -> Option<&'a FrameState> {
    active_frame.and_then(|frame_id| frames.get(frame_id))
}

/// Select the active root view for workbench-aware composition.
pub fn select_active_root_view(
    frames: &HashMap<FrameId, FrameState>,
    active_frame: Option<&FrameId>,
) -> Option<GraphViewId> {
    select_active_frame(frames, active_frame).and_then(|frame| frame.root_view)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn select_active_frame_returns_requested_frame() {
        let frame_id = FrameId::new("main");
        let frame = FrameState {
            id: frame_id.clone(),
            label: "Main".to_string(),
            root_view: None,
            panes: Vec::new(),
        };
        let mut frames = HashMap::new();
        frames.insert(frame_id.clone(), frame.clone());

        let selected = select_active_frame(&frames, Some(&frame_id));

        assert_eq!(selected, Some(&frame));
    }

    #[test]
    fn select_active_frame_returns_none_when_missing() {
        let frames = HashMap::new();
        let frame_id = FrameId::new("missing");

        assert_eq!(select_active_frame(&frames, Some(&frame_id)), None);
        assert_eq!(select_active_frame(&frames, None), None);
    }

    #[test]
    fn select_active_root_view_returns_frame_root_view() {
        let frame_id = FrameId::new("main");
        let root_view = GraphViewId::from_uuid(uuid::Uuid::from_u128(7));
        let frame = FrameState {
            id: frame_id.clone(),
            label: "Main".to_string(),
            root_view: Some(root_view),
            panes: Vec::new(),
        };
        let frames = HashMap::from([(frame_id.clone(), frame)]);

        assert_eq!(
            select_active_root_view(&frames, Some(&frame_id)),
            Some(root_view)
        );
    }
}
