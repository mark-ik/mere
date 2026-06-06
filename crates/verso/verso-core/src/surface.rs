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
mod tests;
