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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceRequest {
    Present,
    Retire,
    Focus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceCommand {
    Present {
        host: SurfaceHostId,
        view: Option<GraphViewId>,
        pane: Option<PaneId>,
    },
    Retire {
        host: SurfaceHostId,
        view: Option<GraphViewId>,
        pane: Option<PaneId>,
    },
    Focus {
        host: SurfaceHostId,
        view: Option<GraphViewId>,
        pane: Option<PaneId>,
    },
}

pub trait SurfaceCommandSink {
    type Error;

    fn apply_surface_command(&mut self, command: &SurfaceCommand) -> Result<(), Self::Error>;
}

impl SurfaceCommand {
    pub fn present(host: SurfaceHostId, view: Option<GraphViewId>, pane: Option<PaneId>) -> Self {
        Self::Present { host, view, pane }
    }

    pub fn retire(host: SurfaceHostId, view: Option<GraphViewId>, pane: Option<PaneId>) -> Self {
        Self::Retire { host, view, pane }
    }

    pub fn focus(host: SurfaceHostId, view: Option<GraphViewId>, pane: Option<PaneId>) -> Self {
        Self::Focus { host, view, pane }
    }

    pub fn host(&self) -> &SurfaceHostId {
        match self {
            Self::Present { host, .. } | Self::Retire { host, .. } | Self::Focus { host, .. } => {
                host
            }
        }
    }

    pub fn view(&self) -> Option<GraphViewId> {
        match self {
            Self::Present { view, .. } | Self::Retire { view, .. } | Self::Focus { view, .. } => {
                *view
            }
        }
    }

    pub fn pane(&self) -> Option<PaneId> {
        match self {
            Self::Present { pane, .. } | Self::Retire { pane, .. } | Self::Focus { pane, .. } => {
                *pane
            }
        }
    }

    pub fn request(&self) -> SurfaceRequest {
        match self {
            Self::Present { .. } => SurfaceRequest::Present,
            Self::Retire { .. } => SurfaceRequest::Retire,
            Self::Focus { .. } => SurfaceRequest::Focus,
        }
    }

    pub fn to_effect(&self) -> SurfaceEffect {
        SurfaceEffect {
            host: self.host().clone(),
            view: self.view(),
            pane: self.pane(),
            request: self.request(),
        }
    }
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

    #[test]
    fn present_command_sets_present_request() {
        let command = SurfaceCommand::present(SurfaceHostId::new("desktop"), None, None);

        assert_eq!(command.request(), SurfaceRequest::Present);
    }

    #[test]
    fn command_round_trips_to_effect() {
        let command = SurfaceCommand::retire(SurfaceHostId::new("desktop"), None, None);

        let effect = command.to_effect();

        assert_eq!(effect.request, SurfaceRequest::Retire);
        assert_eq!(effect.host, SurfaceHostId::new("desktop"));
    }

    #[test]
    fn command_sink_accepts_surface_commands() {
        #[derive(Default)]
        struct RecordingSink {
            commands: Vec<SurfaceCommand>,
        }

        impl SurfaceCommandSink for RecordingSink {
            type Error = ();

            fn apply_surface_command(
                &mut self,
                command: &SurfaceCommand,
            ) -> Result<(), Self::Error> {
                self.commands.push(command.clone());
                Ok(())
            }
        }

        let mut sink = RecordingSink::default();
        let command = SurfaceCommand::focus(SurfaceHostId::new("desktop"), None, None);

        sink.apply_surface_command(&command).unwrap();

        assert_eq!(sink.commands, vec![command]);
    }
}
