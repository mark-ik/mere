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

    fn apply_surface_command(
        &mut self,
        command: &SurfaceCommand,
    ) -> Result<SurfaceCommandOutcome, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceCommandStatus {
    Applied,
    AlreadySatisfied,
    Deferred,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceCommandOutcome {
    pub host: SurfaceHostId,
    pub view: Option<GraphViewId>,
    pub pane: Option<PaneId>,
    pub request: SurfaceRequest,
    pub status: SurfaceCommandStatus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceCommandBacklog {
    deferred: Vec<SurfaceCommand>,
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

    pub fn present_pane(host: SurfaceHostId, view: GraphViewId, pane: PaneId) -> Self {
        Self::present(host, Some(view), Some(pane))
    }

    pub fn retire_pane(host: SurfaceHostId, view: GraphViewId, pane: PaneId) -> Self {
        Self::retire(host, Some(view), Some(pane))
    }

    pub fn focus_pane(host: SurfaceHostId, view: GraphViewId, pane: PaneId) -> Self {
        Self::focus(host, Some(view), Some(pane))
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

    pub fn outcome(&self, status: SurfaceCommandStatus) -> SurfaceCommandOutcome {
        SurfaceCommandOutcome {
            host: self.host().clone(),
            view: self.view(),
            pane: self.pane(),
            request: self.request(),
            status,
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

impl SurfaceCommandOutcome {
    pub fn is_deferred(&self) -> bool {
        self.status == SurfaceCommandStatus::Deferred
    }

    pub fn matches_command(&self, command: &SurfaceCommand) -> bool {
        self.host == *command.host()
            && self.view == command.view()
            && self.pane == command.pane()
            && self.request == command.request()
    }
}

impl SurfaceCommandBacklog {
    pub fn len(&self) -> usize {
        self.deferred.len()
    }

    pub fn is_empty(&self) -> bool {
        self.deferred.is_empty()
    }

    pub fn deferred_commands(&self) -> &[SurfaceCommand] {
        &self.deferred
    }

    pub fn push_deferred(&mut self, command: SurfaceCommand) {
        self.deferred.push(command);
    }

    pub fn record_outcome(
        &mut self,
        command: &SurfaceCommand,
        outcome: &SurfaceCommandOutcome,
    ) -> bool {
        if outcome.is_deferred() && outcome.matches_command(command) {
            self.push_deferred(command.clone());
            true
        } else {
            false
        }
    }

    pub fn take_next(&mut self) -> Option<SurfaceCommand> {
        if self.deferred.is_empty() {
            None
        } else {
            Some(self.deferred.remove(0))
        }
    }

    pub fn drain(&mut self) -> Vec<SurfaceCommand> {
        std::mem::take(&mut self.deferred)
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
    fn pane_command_helpers_set_view_and_pane() {
        let view = GraphViewId::new();
        let pane = PaneId::new();
        let command = SurfaceCommand::present_pane(SurfaceHostId::new("desktop"), view, pane);

        assert_eq!(command.request(), SurfaceRequest::Present);
        assert_eq!(command.view(), Some(view));
        assert_eq!(command.pane(), Some(pane));
    }

    #[test]
    fn command_round_trips_to_effect() {
        let command = SurfaceCommand::retire(SurfaceHostId::new("desktop"), None, None);

        let effect = command.to_effect();

        assert_eq!(effect.request, SurfaceRequest::Retire);
        assert_eq!(effect.host, SurfaceHostId::new("desktop"));
    }

    #[test]
    fn command_builds_host_reportable_outcome() {
        let command = SurfaceCommand::focus(SurfaceHostId::new("desktop"), None, None);

        let outcome = command.outcome(SurfaceCommandStatus::Applied);

        assert_eq!(outcome.request, SurfaceRequest::Focus);
        assert_eq!(outcome.status, SurfaceCommandStatus::Applied);
        assert_eq!(outcome.host, SurfaceHostId::new("desktop"));
    }

    #[test]
    fn outcome_matches_its_source_command() {
        let command = SurfaceCommand::present(SurfaceHostId::new("desktop"), None, None);
        let outcome = command.outcome(SurfaceCommandStatus::Deferred);

        assert!(outcome.is_deferred());
        assert!(outcome.matches_command(&command));
    }

    #[test]
    fn backlog_records_only_matching_deferred_outcomes() {
        let command = SurfaceCommand::present(SurfaceHostId::new("desktop"), None, None);
        let applied = command.outcome(SurfaceCommandStatus::Applied);
        let deferred = command.outcome(SurfaceCommandStatus::Deferred);
        let mismatched = SurfaceCommand::retire(SurfaceHostId::new("desktop"), None, None)
            .outcome(SurfaceCommandStatus::Deferred);
        let mut backlog = SurfaceCommandBacklog::default();

        assert!(!backlog.record_outcome(&command, &applied));
        assert!(!backlog.record_outcome(&command, &mismatched));
        assert!(backlog.record_outcome(&command, &deferred));

        assert_eq!(backlog.len(), 1);
        assert_eq!(backlog.take_next(), Some(command));
        assert!(backlog.is_empty());
    }

    #[test]
    fn command_sink_accepts_surface_commands() {
        #[derive(Default)]
        struct RecordingSink {
            outcomes: Vec<SurfaceCommandOutcome>,
        }

        impl SurfaceCommandSink for RecordingSink {
            type Error = ();

            fn apply_surface_command(
                &mut self,
                command: &SurfaceCommand,
            ) -> Result<SurfaceCommandOutcome, Self::Error> {
                let outcome = command.outcome(SurfaceCommandStatus::Applied);
                self.outcomes.push(outcome.clone());
                Ok(outcome)
            }
        }

        let mut sink = RecordingSink::default();
        let command = SurfaceCommand::focus(SurfaceHostId::new("desktop"), None, None);

        let outcome = sink.apply_surface_command(&command).unwrap();

        assert_eq!(outcome.status, SurfaceCommandStatus::Applied);
        assert_eq!(sink.outcomes, vec![outcome]);
    }
}
