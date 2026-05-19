# verso-tile

`verso-tile` owns the rendering-surface layer for the
[mere](https://crates.io/crates/mere) browser: surface identity, tile-slot
placement, and the lifecycle that moves surfaces between *Present*, *Retire*,
and *Focus* states. Hosts implement small traits to apply commands;
verso-tile owns the schedule shape and the apply algorithm.

In the printing-press metaphor: *verso* is the back of the printed leaf, the
page that catches the impression. `verso-tile` is the surface that receives
engine output (routed by [`inker`](https://crates.io/crates/inker), composed
by [`platen`](https://crates.io/crates/platen)) and places it into tile
slots.

## What's in the crate

- **`surface`** — identity, command vocabulary, and lifecycle state.
  - **Identity**: `SurfaceTargetId`, `SurfaceHostId`.
  - **Commands**: `SurfaceCommand` (`Present` / `Retire` / `Focus` variants),
    `SurfaceRequest`, `SurfaceEffect`.
  - **Outcomes**: `SurfaceCommandOutcome`, `SurfaceCommandStatus` (`Applied`,
    `AlreadySatisfied`, `Deferred`).
  - **Lifecycle**: `SurfaceLifecycleState` (placement + outcome tracking),
    `SurfaceCommandSchedule`, `SurfaceCommandBacklog` (deferred-command retry
    bookkeeping).
  - **Placement**: `TileSlot`, `SurfaceSlotPlacement`, `SurfacePlacementPlan`.
  - **Host port**: `SurfaceCommandSink` — the trait hosts implement to apply
    commands.

- **`apply`** — apply algorithm for `SurfaceCommandSchedule`.
  - `apply_viewer_surface_schedule()` — walks a schedule, allocates / retires
    viewer surfaces via the host, records outcomes back on the lifecycle
    state, returns a report.
  - `SurfaceScheduleApplyReport` — counters for allocated / retired /
    already-satisfied / deferred / unsupported.
  - `SurfaceScheduleApplyError` — `MissingPlacement` and `Viewer` variants.

## How it relates to other workspace crates

verso-tile sits below the press-stack peers (inker, platen) and above
concrete host adapters; it's the layer that turns "this surface should
exist" decisions into actual viewer allocations.

```text
   inker  ───── SurfaceContract.target ────────┐
                                                │
   platen ───── SurfacePlacementPlan ───────────┤
                                                │
   graphshell ─ SurfaceCommand ─────────────────┤
                (WorkspaceEffect)               │
                                                ▼
                                      SurfaceLifecycleState
                                                │
                                                ▼
                                      SurfaceCommandSchedule
                                                │ apply_viewer_surface_schedule()
                                                ▼
                                  host: SurfaceCommandSink
                                      + ViewerSurfaceHost
```

- [`inker`](https://crates.io/crates/inker) — `SurfaceContract.target` is
  verso-tile's `SurfaceTargetId` (re-exported through `inker::routing` for
  convenience).
- [`platen`](https://crates.io/crates/platen) — produces
  `SurfacePlacementPlan`s composed of `SurfaceSlotPlacement` + `TileSlot`;
  verso-tile's `SurfaceLifecycleState::schedule_placements()` consumes them.
- [`graphshell`](https://crates.io/crates/graphshell) — emits
  `SurfaceCommand` effects via `WorkspaceEffect::RequestSurface`; the
  `SurfaceHost` trait in graphshell is a specialization of
  `SurfaceCommandSink`.
- **`mere-kernel`** (workspace-internal) — the apply algorithm uses
  `mere_kernel::viewer_host::ViewerSurfaceHost` as its host-port trait
  bound; concrete host adapters implement this.

## Status

Pre-1.0. Surface identity, command vocabulary, lifecycle scheduling
(present and retire), placement plans, deferred-command backlog, and the
apply algorithm are all in place. Concrete host adapters consume the surface
through `SurfaceCommandSink` and `ViewerSurfaceHost`.

## Fun Fact

The name "verso-tile" was chosen because of the archived experimental browser
project for servo, `verso` (the associated company was `versotile`). It was cool!
Led to a lot of learnings that benefitted this project. This is merely an homage,
but I am happy to change the name if needed.

## License

MPL-2.0.
