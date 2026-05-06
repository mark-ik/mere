/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable surface identity contracts.

use graphshell_core::graph::GraphViewId;
use graphshell_core::pane::PaneId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SurfaceTargetId(pub String);

impl SurfaceTargetId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SurfaceHostId(pub String);

impl SurfaceHostId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceEffect {
    pub host: SurfaceHostId,
    pub view: Option<GraphViewId>,
    pub pane: Option<PaneId>,
    pub request: SurfaceRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceRequest {
    Present,
    Retire,
    Focus,
}

impl SurfaceEffect {
    pub fn present(host: SurfaceHostId, view: Option<GraphViewId>, pane: Option<PaneId>) -> Self {
        Self {
            host,
            view,
            pane,
            request: SurfaceRequest::Present,
        }
    }

    pub fn retire(host: SurfaceHostId, view: Option<GraphViewId>, pane: Option<PaneId>) -> Self {
        Self {
            host,
            view,
            pane,
            request: SurfaceRequest::Retire,
        }
    }

    pub fn focus(host: SurfaceHostId, view: Option<GraphViewId>, pane: Option<PaneId>) -> Self {
        Self {
            host,
            view,
            pane,
            request: SurfaceRequest::Focus,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_constructor_sets_present_request() {
        let effect = SurfaceEffect::present(SurfaceHostId::new("desktop"), None, None);

        assert_eq!(effect.request, SurfaceRequest::Present);
    }

    #[test]
    fn retire_constructor_sets_retire_request() {
        let effect = SurfaceEffect::retire(SurfaceHostId::new("desktop"), None, None);

        assert_eq!(effect.request, SurfaceRequest::Retire);
    }

    #[test]
    fn focus_constructor_sets_focus_request() {
        let effect = SurfaceEffect::focus(SurfaceHostId::new("desktop"), None, None);

        assert_eq!(effect.request, SurfaceRequest::Focus);
    }
}
