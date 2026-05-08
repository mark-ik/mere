/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

use graphshell_core::graph::{GraphViewId, NodeKey};
use graphshell_core::pane::PaneId;
use graphshell_host::SurfaceFactoryHost;
use verso_tile::apply_viewer_surface_schedule;
use verso_tile::surface::{
    SurfaceHostId, SurfaceLifecycleState, SurfacePlacementPlan, SurfaceSlotPlacement, TileSlot,
};

#[derive(Debug, PartialEq, Eq)]
struct TestSurface {
    id: usize,
}

#[test]
fn surface_factory_host_applies_runtime_present_and_retire_schedules() {
    let view = GraphViewId::new();
    let pane = PaneId::new();
    let node = NodeKey::new(9);
    let mut next_surface_id = 0;
    let mut host = SurfaceFactoryHost::new(|| {
        next_surface_id += 1;
        TestSurface {
            id: next_surface_id,
        }
    });
    let mut registry = HashMap::<NodeKey, TestSurface>::new();
    let mut placements = SurfacePlacementPlan::default();
    placements.push(SurfaceSlotPlacement::new(
        SurfaceHostId::new("desktop"),
        Some(view),
        pane,
        node,
        TileSlot::primary(),
    ));
    let mut lifecycle = SurfaceLifecycleState::default();

    let present_schedule = lifecycle.schedule_placements(placements);
    let present_report =
        apply_viewer_surface_schedule(&mut lifecycle, &present_schedule, &mut registry, &mut host)
            .unwrap();

    assert_eq!(present_report.allocated, 1);
    assert_eq!(registry.get(&node), Some(&TestSurface { id: 1 }));

    let retire_schedule = lifecycle.schedule_retire_pane(pane).unwrap();
    let retire_report =
        apply_viewer_surface_schedule(&mut lifecycle, &retire_schedule, &mut registry, &mut host)
            .unwrap();

    assert_eq!(retire_report.retired, 1);
    assert!(!registry.contains_key(&node));
    assert!(lifecycle.placements().placement_for_pane(pane).is_none());
}
