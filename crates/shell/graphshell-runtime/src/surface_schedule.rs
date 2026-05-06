/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Runtime adapter for applying portable surface schedules to host viewers.

use graphshell_core::viewer_host::{ViewerSurfaceError, ViewerSurfaceHost};
use verso_tile::surface::{
    SurfaceCommand, SurfaceCommandOutcome, SurfaceCommandSchedule, SurfaceCommandStatus,
    SurfaceLifecycleState, SurfaceRequest,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SurfaceScheduleApplyReport {
    pub outcomes: Vec<SurfaceCommandOutcome>,
    pub allocated: usize,
    pub already_present: usize,
    pub deferred: usize,
    pub unsupported: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceScheduleApplyError {
    MissingPlacement(SurfaceCommand),
    Viewer(ViewerSurfaceError),
}

pub fn apply_present_surface_schedule<Registry, Host>(
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
            _ => {
                report.unsupported += 1;
                command.outcome(SurfaceCommandStatus::Deferred)
            }
        };
        if outcome.status == SurfaceCommandStatus::Applied {
            report.allocated += 1;
        } else if outcome.status == SurfaceCommandStatus::AlreadySatisfied {
            report.already_present += 1;
        } else if outcome.status == SurfaceCommandStatus::Deferred {
            report.deferred += 1;
        }
        lifecycle.record_outcome(command, &outcome);
        report.outcomes.push(outcome);
    }
    Ok(report)
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
    if host.has_surface(registry, placement.node) {
        return Ok(command.outcome(SurfaceCommandStatus::AlreadySatisfied));
    }
    host.allocate_surface(registry, placement.node)
        .map_err(SurfaceScheduleApplyError::Viewer)?;
    Ok(command.outcome(SurfaceCommandStatus::Applied))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use graphshell_core::{graph::GraphViewId, graph::NodeKey, pane::PaneId};
    use verso_tile::surface::{
        SurfaceHostId, SurfacePlacementPlan, SurfaceSlotPlacement, TileSlot,
    };

    use super::*;

    #[derive(Default)]
    struct MockRegistry {
        nodes: HashSet<NodeKey>,
    }

    #[derive(Default)]
    struct MockHost;

    impl ViewerSurfaceHost<MockRegistry> for MockHost {
        fn allocate_surface(
            &mut self,
            registry: &mut MockRegistry,
            node_key: NodeKey,
        ) -> Result<(), ViewerSurfaceError> {
            registry.nodes.insert(node_key);
            Ok(())
        }

        fn retire_surface(&mut self, registry: &mut MockRegistry, node_key: NodeKey) {
            registry.nodes.remove(&node_key);
        }

        fn has_surface(&self, registry: &MockRegistry, node_key: NodeKey) -> bool {
            registry.nodes.contains(&node_key)
        }
    }

    #[test]
    fn present_schedule_allocates_viewer_surfaces_by_placement_node() {
        let view = GraphViewId::new();
        let node = NodeKey::new(3);
        let pane = PaneId::new();
        let mut plan = SurfacePlacementPlan::default();
        plan.push(SurfaceSlotPlacement::new(
            SurfaceHostId::new("desktop"),
            Some(view),
            pane,
            node,
            TileSlot::primary(),
        ));
        let mut lifecycle = SurfaceLifecycleState::default();
        let schedule = lifecycle.schedule_placements(plan);
        let mut registry = MockRegistry::default();
        let mut host = MockHost;

        let report =
            apply_present_surface_schedule(&mut lifecycle, &schedule, &mut registry, &mut host)
                .unwrap();

        assert_eq!(report.allocated, 1);
        assert!(registry.nodes.contains(&node));
        assert!(lifecycle.backlog().is_empty());
    }

    #[test]
    fn present_schedule_reports_already_present_without_reallocating() {
        let view = GraphViewId::new();
        let node = NodeKey::new(4);
        let pane = PaneId::new();
        let mut plan = SurfacePlacementPlan::default();
        plan.push(SurfaceSlotPlacement::new(
            SurfaceHostId::new("desktop"),
            Some(view),
            pane,
            node,
            TileSlot::primary(),
        ));
        let mut lifecycle = SurfaceLifecycleState::default();
        let schedule = lifecycle.schedule_placements(plan);
        let mut registry = MockRegistry::default();
        registry.nodes.insert(node);
        let mut host = MockHost;

        let report =
            apply_present_surface_schedule(&mut lifecycle, &schedule, &mut registry, &mut host)
                .unwrap();

        assert_eq!(report.already_present, 1);
        assert_eq!(
            report.outcomes[0].status,
            SurfaceCommandStatus::AlreadySatisfied
        );
    }
}
