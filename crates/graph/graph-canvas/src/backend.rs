/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Backend trait and capability model.
//!
//! A `CanvasBackend` consumes a `ProjectedScene` and renders it. The trait is
//! defined here so that portable code can reason about backend capabilities
//! without depending on any concrete backend.

use crate::camera::CanvasViewport;
use crate::packet::ProjectedScene;
use serde::{Deserialize, Serialize};
use std::hash::Hash;

/// Capability flags reported by a backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CanvasBackendCapabilities {
    /// Backend supports 2.5D depth-cue rendering.
    pub two_point_five: bool,
    /// Backend supports isometric projection rendering.
    pub isometric: bool,
    /// Backend supports image/texture rendering.
    pub images: bool,
    /// Backend supports text/label rendering.
    pub labels: bool,
    /// Backend supports anti-aliased stroke rendering.
    pub anti_aliased_strokes: bool,
}

/// The rendering backend contract.
///
/// Backends receive a `ProjectedScene` (the complete draw list and hit
/// proxies for one frame) and a viewport, then render. The backend must not
/// define graph semantics — it only draws what the scene packet describes.
///
/// Phase 0 defines the trait only. Implementations arrive in:
/// - Phase 3 (egui host bridge — transitional)
/// - Phase 4 (Vello backend)
/// - future iced host bridge
pub trait CanvasBackend<N: Eq + Hash> {
    type FrameHandle;

    /// Prepare GPU resources for the given scene and viewport.
    fn prepare(&mut self, scene: &ProjectedScene<N>, viewport: &CanvasViewport);

    /// Render the prepared scene into the provided frame.
    fn render(&mut self, frame: &mut Self::FrameHandle);

    /// Report what this backend can do.
    fn capabilities(&self) -> CanvasBackendCapabilities;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_default_all_false() {
        let caps = CanvasBackendCapabilities::default();
        assert!(!caps.two_point_five);
        assert!(!caps.isometric);
        assert!(!caps.images);
        assert!(!caps.labels);
        assert!(!caps.anti_aliased_strokes);
    }

    #[test]
    fn serde_roundtrip_capabilities() {
        let caps = CanvasBackendCapabilities {
            two_point_five: true,
            isometric: true,
            images: true,
            labels: true,
            anti_aliased_strokes: false,
        };
        let json = serde_json::to_string(&caps).unwrap();
        let back: CanvasBackendCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, back);
    }
}
