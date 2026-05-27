/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable surface identity contracts.

use serde::{Deserialize, Serialize};

/// Forme-assigned surface/tile identity. The durable per-tile key verso
/// realizes against; the host maps `forme::ArrangementNodeId` -> `TileId`
/// (both UUID-backed). Replaces the substrate-era `NodeKey`/`PaneId` keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileId(pub uuid::Uuid);

impl TileId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
    pub fn from_uuid(id: uuid::Uuid) -> Self {
        Self(id)
    }
    pub fn as_uuid(&self) -> uuid::Uuid {
        self.0
    }
}

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
    pub tile: TileId,
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
    Present { host: SurfaceHostId, tile: TileId },
    Retire { host: SurfaceHostId, tile: TileId },
    Focus { host: SurfaceHostId, tile: TileId },
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
    pub tile: TileId,
    pub request: SurfaceRequest,
    pub status: SurfaceCommandStatus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceCommandBacklog {
    deferred: Vec<SurfaceCommand>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileSlot {
    pub index: usize,
    pub is_primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceSlotPlacement {
    pub host: SurfaceHostId,
    pub tile: TileId,
    pub slot: TileSlot,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfacePlacementPlan {
    placements: Vec<SurfaceSlotPlacement>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceCommandSchedule {
    commands: Vec<SurfaceCommand>,
    pub placements: usize,
    pub retries: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceLifecycleState {
    placements: SurfacePlacementPlan,
    backlog: SurfaceCommandBacklog,
}

impl SurfaceCommand {
    pub fn present(host: SurfaceHostId, tile: TileId) -> Self {
        Self::Present { host, tile }
    }

    pub fn retire(host: SurfaceHostId, tile: TileId) -> Self {
        Self::Retire { host, tile }
    }

    pub fn focus(host: SurfaceHostId, tile: TileId) -> Self {
        Self::Focus { host, tile }
    }

    pub fn host(&self) -> &SurfaceHostId {
        match self {
            Self::Present { host, .. } | Self::Retire { host, .. } | Self::Focus { host, .. } => {
                host
            }
        }
    }

    pub fn tile(&self) -> TileId {
        match self {
            Self::Present { tile, .. } | Self::Retire { tile, .. } | Self::Focus { tile, .. } => {
                *tile
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
            tile: self.tile(),
            request: self.request(),
        }
    }

    pub fn outcome(&self, status: SurfaceCommandStatus) -> SurfaceCommandOutcome {
        SurfaceCommandOutcome {
            host: self.host().clone(),
            tile: self.tile(),
            request: self.request(),
            status,
        }
    }
}

impl SurfaceEffect {
    pub fn present(host: SurfaceHostId, tile: TileId) -> Self {
        Self {
            host,
            tile,
            request: SurfaceRequest::Present,
        }
    }

    pub fn retire(host: SurfaceHostId, tile: TileId) -> Self {
        Self {
            host,
            tile,
            request: SurfaceRequest::Retire,
        }
    }

    pub fn focus(host: SurfaceHostId, tile: TileId) -> Self {
        Self {
            host,
            tile,
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
            && self.tile == command.tile()
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

impl TileSlot {
    pub fn new(index: usize, is_primary: bool) -> Self {
        Self { index, is_primary }
    }

    pub fn primary() -> Self {
        Self::new(0, true)
    }

    pub fn secondary(index: usize) -> Self {
        Self::new(index, false)
    }
}

impl SurfaceSlotPlacement {
    pub fn new(host: SurfaceHostId, tile: TileId, slot: TileSlot) -> Self {
        Self { host, tile, slot }
    }

    pub fn present_command(&self) -> SurfaceCommand {
        SurfaceCommand::present(self.host.clone(), self.tile)
    }

    pub fn retire_command(&self) -> SurfaceCommand {
        SurfaceCommand::retire(self.host.clone(), self.tile)
    }
}

impl SurfacePlacementPlan {
    pub fn len(&self) -> usize {
        self.placements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.placements.is_empty()
    }

    pub fn placements(&self) -> &[SurfaceSlotPlacement] {
        &self.placements
    }

    pub fn push(&mut self, placement: SurfaceSlotPlacement) {
        self.placements.push(placement);
    }

    pub fn placement_for_tile(&self, tile: TileId) -> Option<&SurfaceSlotPlacement> {
        self.placements
            .iter()
            .find(|placement| placement.tile == tile)
    }

    pub fn placement_for_command(&self, command: &SurfaceCommand) -> Option<&SurfaceSlotPlacement> {
        self.placements.iter().find(|placement| {
            placement.host == *command.host() && placement.tile == command.tile()
        })
    }

    pub fn retire_command_for_tile(&self, tile: TileId) -> Option<SurfaceCommand> {
        self.placement_for_tile(tile)
            .map(SurfaceSlotPlacement::retire_command)
    }

    pub fn remove_placement_for_command(
        &mut self,
        command: &SurfaceCommand,
    ) -> Option<SurfaceSlotPlacement> {
        let index = self.placements.iter().position(|placement| {
            placement.host == *command.host() && placement.tile == command.tile()
        })?;
        Some(self.placements.remove(index))
    }

    pub fn present_commands(&self) -> Vec<SurfaceCommand> {
        self.placements
            .iter()
            .map(SurfaceSlotPlacement::present_command)
            .collect()
    }
}

impl SurfaceCommandSchedule {
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn commands(&self) -> &[SurfaceCommand] {
        &self.commands
    }
}

impl SurfaceLifecycleState {
    pub fn placements(&self) -> &SurfacePlacementPlan {
        &self.placements
    }

    pub fn placement_for_command(&self, command: &SurfaceCommand) -> Option<&SurfaceSlotPlacement> {
        self.placements.placement_for_command(command)
    }

    pub fn backlog(&self) -> &SurfaceCommandBacklog {
        &self.backlog
    }

    pub fn schedule_placements(&mut self, plan: SurfacePlacementPlan) -> SurfaceCommandSchedule {
        let commands = plan.present_commands();
        let placements = plan.len();
        self.placements = plan;
        SurfaceCommandSchedule {
            commands,
            placements,
            retries: 0,
        }
    }

    pub fn schedule_retire_tile(&self, tile: TileId) -> Option<SurfaceCommandSchedule> {
        let command = self.placements.retire_command_for_tile(tile)?;
        Some(SurfaceCommandSchedule {
            commands: vec![command],
            placements: 0,
            retries: 0,
        })
    }

    pub fn record_outcome(
        &mut self,
        command: &SurfaceCommand,
        outcome: &SurfaceCommandOutcome,
    ) -> bool {
        let recorded = self.backlog.record_outcome(command, outcome);
        if outcome.matches_command(command)
            && command.request() == SurfaceRequest::Retire
            && matches!(
                outcome.status,
                SurfaceCommandStatus::Applied | SurfaceCommandStatus::AlreadySatisfied
            )
        {
            self.placements.remove_placement_for_command(command);
        }
        recorded
    }

    pub fn schedule_retries(&mut self) -> SurfaceCommandSchedule {
        let commands = self.backlog.drain();
        let retries = commands.len();
        SurfaceCommandSchedule {
            commands,
            placements: 0,
            retries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_constructor_sets_present_request() {
        let effect = SurfaceEffect::present(SurfaceHostId::new("desktop"), TileId::new());

        assert_eq!(effect.request, SurfaceRequest::Present);
    }

    #[test]
    fn retire_constructor_sets_retire_request() {
        let effect = SurfaceEffect::retire(SurfaceHostId::new("desktop"), TileId::new());

        assert_eq!(effect.request, SurfaceRequest::Retire);
    }

    #[test]
    fn focus_constructor_sets_focus_request() {
        let effect = SurfaceEffect::focus(SurfaceHostId::new("desktop"), TileId::new());

        assert_eq!(effect.request, SurfaceRequest::Focus);
    }

    #[test]
    fn present_command_sets_present_request() {
        let command = SurfaceCommand::present(SurfaceHostId::new("desktop"), TileId::new());

        assert_eq!(command.request(), SurfaceRequest::Present);
    }

    #[test]
    fn command_helpers_set_tile() {
        let tile = TileId::new();
        let command = SurfaceCommand::present(SurfaceHostId::new("desktop"), tile);

        assert_eq!(command.request(), SurfaceRequest::Present);
        assert_eq!(command.tile(), tile);
    }

    #[test]
    fn command_round_trips_to_effect() {
        let command = SurfaceCommand::retire(SurfaceHostId::new("desktop"), TileId::new());

        let effect = command.to_effect();

        assert_eq!(effect.request, SurfaceRequest::Retire);
        assert_eq!(effect.host, SurfaceHostId::new("desktop"));
    }

    #[test]
    fn command_builds_host_reportable_outcome() {
        let command = SurfaceCommand::focus(SurfaceHostId::new("desktop"), TileId::new());

        let outcome = command.outcome(SurfaceCommandStatus::Applied);

        assert_eq!(outcome.request, SurfaceRequest::Focus);
        assert_eq!(outcome.status, SurfaceCommandStatus::Applied);
        assert_eq!(outcome.host, SurfaceHostId::new("desktop"));
    }

    #[test]
    fn outcome_matches_its_source_command() {
        let command = SurfaceCommand::present(SurfaceHostId::new("desktop"), TileId::new());
        let outcome = command.outcome(SurfaceCommandStatus::Deferred);

        assert!(outcome.is_deferred());
        assert!(outcome.matches_command(&command));
    }

    #[test]
    fn backlog_records_only_matching_deferred_outcomes() {
        let tile = TileId::new();
        let command = SurfaceCommand::present(SurfaceHostId::new("desktop"), tile);
        let applied = command.outcome(SurfaceCommandStatus::Applied);
        let deferred = command.outcome(SurfaceCommandStatus::Deferred);
        let mismatched = SurfaceCommand::retire(SurfaceHostId::new("desktop"), tile)
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
    fn tile_slot_constructors_mark_primary_slot() {
        assert_eq!(TileSlot::primary(), TileSlot::new(0, true));
        assert_eq!(TileSlot::secondary(2), TileSlot::new(2, false));
    }

    #[test]
    fn placement_plan_exposes_present_commands_by_slot() {
        let tile = TileId::new();
        let host = SurfaceHostId::new("desktop");
        let placement = SurfaceSlotPlacement::new(host.clone(), tile, TileSlot::primary());
        let mut plan = SurfacePlacementPlan::default();

        plan.push(placement);

        assert_eq!(plan.len(), 1);
        assert_eq!(
            plan.placement_for_tile(tile).unwrap().slot,
            TileSlot::primary()
        );
        assert_eq!(
            plan.placement_for_command(&SurfaceCommand::present(host.clone(), tile))
                .unwrap()
                .tile,
            tile
        );
        assert_eq!(
            plan.present_commands(),
            vec![SurfaceCommand::present(host, tile)]
        );
        assert_eq!(
            plan.retire_command_for_tile(tile),
            Some(SurfaceCommand::retire(SurfaceHostId::new("desktop"), tile))
        );
    }

    #[test]
    fn lifecycle_state_schedules_placements_and_retries_deferred_commands() {
        let first_tile = TileId::new();
        let second_tile = TileId::new();
        let host = SurfaceHostId::new("desktop");
        let mut plan = SurfacePlacementPlan::default();
        plan.push(SurfaceSlotPlacement::new(
            host.clone(),
            first_tile,
            TileSlot::primary(),
        ));
        plan.push(SurfaceSlotPlacement::new(
            host.clone(),
            second_tile,
            TileSlot::secondary(1),
        ));
        let mut lifecycle = SurfaceLifecycleState::default();

        let schedule = lifecycle.schedule_placements(plan);

        assert_eq!(schedule.placements, 2);
        assert_eq!(schedule.retries, 0);
        assert_eq!(schedule.len(), 2);
        assert_eq!(lifecycle.placements().len(), 2);
        let first_command = schedule.commands()[0].clone();
        let second_command = schedule.commands()[1].clone();
        assert_eq!(
            lifecycle
                .placement_for_command(&first_command)
                .unwrap()
                .tile,
            first_tile
        );
        assert!(lifecycle.record_outcome(
            &first_command,
            &first_command.outcome(SurfaceCommandStatus::Deferred)
        ));
        assert!(!lifecycle.record_outcome(
            &second_command,
            &second_command.outcome(SurfaceCommandStatus::Applied)
        ));

        let retry = lifecycle.schedule_retries();

        assert_eq!(retry.placements, 0);
        assert_eq!(retry.retries, 1);
        assert_eq!(retry.commands(), &[first_command]);
        assert!(lifecycle.backlog().is_empty());
    }

    #[test]
    fn lifecycle_state_schedules_retire_and_removes_applied_placement() {
        let tile = TileId::new();
        let host = SurfaceHostId::new("desktop");
        let mut plan = SurfacePlacementPlan::default();
        plan.push(SurfaceSlotPlacement::new(
            host.clone(),
            tile,
            TileSlot::primary(),
        ));
        let mut lifecycle = SurfaceLifecycleState::default();
        lifecycle.schedule_placements(plan);

        let retire = lifecycle.schedule_retire_tile(tile).unwrap();

        assert_eq!(retire.len(), 1);
        assert_eq!(
            retire.commands(),
            &[SurfaceCommand::retire(host, tile)]
        );
        assert!(!lifecycle.record_outcome(
            &retire.commands()[0],
            &retire.commands()[0].outcome(SurfaceCommandStatus::Applied)
        ));
        assert!(lifecycle.placements().placement_for_tile(tile).is_none());
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
        let command = SurfaceCommand::focus(SurfaceHostId::new("desktop"), TileId::new());

        let outcome = sink.apply_surface_command(&command).unwrap();

        assert_eq!(outcome.status, SurfaceCommandStatus::Applied);
        assert_eq!(sink.outcomes, vec![outcome]);
    }
}
