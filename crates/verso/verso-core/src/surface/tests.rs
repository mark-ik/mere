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
