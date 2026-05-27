/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Apply algorithm for [`SurfaceCommandSchedule`].
//!
//! Lives next to the schedule type so "manage surface lifecycle" has one
//! owner. Hosts provide a [`ViewerSurfaceHost`] implementation; the apply
//! algorithm walks each command, allocates or retires the corresponding
//! viewer surface, and records the outcome on the lifecycle state.

use crate::host::{ViewerSurfaceError, ViewerSurfaceHost};

use crate::surface::{
    SurfaceCommand, SurfaceCommandOutcome, SurfaceCommandSchedule, SurfaceCommandStatus,
    SurfaceLifecycleState, SurfaceRequest,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SurfaceScheduleApplyReport {
    pub outcomes: Vec<SurfaceCommandOutcome>,
    pub allocated: usize,
    pub retired: usize,
    pub already_satisfied: usize,
    pub deferred: usize,
    pub unsupported: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceScheduleApplyError {
    MissingPlacement(SurfaceCommand),
    Viewer(ViewerSurfaceError),
}

pub fn apply_viewer_surface_schedule<Registry, Host>(
    lifecycle: &mut SurfaceLifecycleState,
    schedule: &SurfaceCommandSchedule,
    registry: &mut Registry,
    host: &mut Host,
) -> Result<SurfaceScheduleApplyReport, SurfaceScheduleApplyError>
where
    Host: ViewerSurfaceHost<Registry>,
{
    let mut report = SurfaceScheduleApplyReport::default();
    for command in schedule.commands() {
        let outcome = match command.request() {
            SurfaceRequest::Present => apply_present_command(lifecycle, command, registry, host)?,
            SurfaceRequest::Retire => apply_retire_command(lifecycle, command, registry, host)?,
            _ => {
                report.unsupported += 1;
                command.outcome(SurfaceCommandStatus::Deferred)
            }
        };
        report.record(command, &outcome);
        lifecycle.record_outcome(command, &outcome);
        report.outcomes.push(outcome);
    }
    Ok(report)
}

impl SurfaceScheduleApplyReport {
    fn record(&mut self, command: &SurfaceCommand, outcome: &SurfaceCommandOutcome) {
        match outcome.status {
            SurfaceCommandStatus::Applied if command.request() == SurfaceRequest::Present => {
                self.allocated += 1;
            }
            SurfaceCommandStatus::Applied if command.request() == SurfaceRequest::Retire => {
                self.retired += 1;
            }
            SurfaceCommandStatus::Applied => {}
            SurfaceCommandStatus::AlreadySatisfied => {
                self.already_satisfied += 1;
            }
            SurfaceCommandStatus::Deferred => {
                self.deferred += 1;
            }
        }
    }
}

fn apply_present_command<Registry, Host>(
    lifecycle: &SurfaceLifecycleState,
    command: &SurfaceCommand,
    registry: &mut Registry,
    host: &mut Host,
) -> Result<SurfaceCommandOutcome, SurfaceScheduleApplyError>
where
    Host: ViewerSurfaceHost<Registry>,
{
    let placement = lifecycle
        .placement_for_command(command)
        .ok_or_else(|| SurfaceScheduleApplyError::MissingPlacement(command.clone()))?;
    if host.has_surface(registry, placement.tile) {
        return Ok(command.outcome(SurfaceCommandStatus::AlreadySatisfied));
    }
    host.allocate_surface(registry, placement.tile)
        .map_err(SurfaceScheduleApplyError::Viewer)?;
    Ok(command.outcome(SurfaceCommandStatus::Applied))
}

fn apply_retire_command<Registry, Host>(
    lifecycle: &SurfaceLifecycleState,
    command: &SurfaceCommand,
    registry: &mut Registry,
    host: &mut Host,
) -> Result<SurfaceCommandOutcome, SurfaceScheduleApplyError>
where
    Host: ViewerSurfaceHost<Registry>,
{
    let placement = lifecycle
        .placement_for_command(command)
        .ok_or_else(|| SurfaceScheduleApplyError::MissingPlacement(command.clone()))?;
    if !host.has_surface(registry, placement.tile) {
        return Ok(command.outcome(SurfaceCommandStatus::AlreadySatisfied));
    }
    host.retire_surface(registry, placement.tile);
    Ok(command.outcome(SurfaceCommandStatus::Applied))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::surface::{SurfaceHostId, SurfacePlacementPlan, SurfaceSlotPlacement, TileId, TileSlot};

    #[derive(Default)]
    struct MockRegistry {
        tiles: HashSet<TileId>,
    }

    #[derive(Default)]
    struct MockHost;

    impl ViewerSurfaceHost<MockRegistry> for MockHost {
        fn allocate_surface(
            &mut self,
            registry: &mut MockRegistry,
            tile: TileId,
        ) -> Result<(), ViewerSurfaceError> {
            registry.tiles.insert(tile);
            Ok(())
        }

        fn retire_surface(&mut self, registry: &mut MockRegistry, tile: TileId) {
            registry.tiles.remove(&tile);
        }

        fn has_surface(&self, registry: &MockRegistry, tile: TileId) -> bool {
            registry.tiles.contains(&tile)
        }
    }

    #[test]
    fn present_schedule_allocates_viewer_surfaces_by_placement_tile() {
        let tile = TileId::new();
        let mut plan = SurfacePlacementPlan::default();
        plan.push(SurfaceSlotPlacement::new(
            SurfaceHostId::new("desktop"),
            tile,
            TileSlot::primary(),
        ));
        let mut lifecycle = SurfaceLifecycleState::default();
        let schedule = lifecycle.schedule_placements(plan);
        let mut registry = MockRegistry::default();
        let mut host = MockHost;

        let report =
            apply_viewer_surface_schedule(&mut lifecycle, &schedule, &mut registry, &mut host)
                .unwrap();

        assert_eq!(report.allocated, 1);
        assert!(registry.tiles.contains(&tile));
        assert!(lifecycle.backlog().is_empty());
    }

    #[test]
    fn present_schedule_reports_already_satisfied_without_reallocating() {
        let tile = TileId::new();
        let mut plan = SurfacePlacementPlan::default();
        plan.push(SurfaceSlotPlacement::new(
            SurfaceHostId::new("desktop"),
            tile,
            TileSlot::primary(),
        ));
        let mut lifecycle = SurfaceLifecycleState::default();
        let schedule = lifecycle.schedule_placements(plan);
        let mut registry = MockRegistry::default();
        registry.tiles.insert(tile);
        let mut host = MockHost;

        let report =
            apply_viewer_surface_schedule(&mut lifecycle, &schedule, &mut registry, &mut host)
                .unwrap();

        assert_eq!(report.already_satisfied, 1);
        assert_eq!(
            report.outcomes[0].status,
            SurfaceCommandStatus::AlreadySatisfied
        );
    }

    #[test]
    fn retire_schedule_retires_existing_surface_and_clears_placement() {
        let tile = TileId::new();
        let mut plan = SurfacePlacementPlan::default();
        plan.push(SurfaceSlotPlacement::new(
            SurfaceHostId::new("desktop"),
            tile,
            TileSlot::primary(),
        ));
        let mut lifecycle = SurfaceLifecycleState::default();
        lifecycle.schedule_placements(plan);
        let schedule = lifecycle.schedule_retire_tile(tile).unwrap();
        let mut registry = MockRegistry::default();
        registry.tiles.insert(tile);
        let mut host = MockHost;

        let report =
            apply_viewer_surface_schedule(&mut lifecycle, &schedule, &mut registry, &mut host)
                .unwrap();

        assert_eq!(report.retired, 1);
        assert!(!registry.tiles.contains(&tile));
        assert!(lifecycle.placements().placement_for_tile(tile).is_none());
    }

    #[test]
    fn retire_schedule_reports_already_satisfied_for_absent_surface() {
        let tile = TileId::new();
        let mut plan = SurfacePlacementPlan::default();
        plan.push(SurfaceSlotPlacement::new(
            SurfaceHostId::new("desktop"),
            tile,
            TileSlot::primary(),
        ));
        let mut lifecycle = SurfaceLifecycleState::default();
        lifecycle.schedule_placements(plan);
        let schedule = lifecycle.schedule_retire_tile(tile).unwrap();
        let mut registry = MockRegistry::default();
        let mut host = MockHost;

        let report =
            apply_viewer_surface_schedule(&mut lifecycle, &schedule, &mut registry, &mut host)
                .unwrap();

        assert_eq!(report.already_satisfied, 1);
        assert_eq!(
            report.outcomes[0].status,
            SurfaceCommandStatus::AlreadySatisfied
        );
        assert!(lifecycle.placements().placement_for_tile(tile).is_none());
    }
}
