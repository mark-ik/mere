# workbench

Workbench domain module for the [mere](https://crates.io/crates/mere) browser.

Projects [`platen::WorkbenchProjection`] (the user's active frame, root
view, and pane bindings) into a subtree of
[AccessKit](https://accesskit.dev) nodes for consumption by `uxtree`.
This is the layer where mere's *user-facing* concept of a workbench gets
its accessibility / automation identity — distinct from platen, which
owns the data shape, and verso-core, which owns the surface lifecycle.

## What it produces

- **Workbench root** (`Role::Group`, label = active frame label or
  `"Workbench"`)
  - **Frame** (`Role::Group`, label = frame label) — present when
    `WorkbenchProjection.active_frame` is set
    - **Pane** (`Role::Group`, label = `"Pane <pane_id>"`) — one per
      `ProjectedPane`. The primary pane carries `description = "primary"`.

Each node gets a deterministic `accesskit::NodeId` from its domain path:

- Workbench root → `workbench`
- Frame → `workbench/frame/{frame_id}`
- Pane → `workbench/frame/{frame_id}/pane/{pane_id}`

## Sibling modules

`mere-domain/workbench` is the first of several mere-domain modules:

- `workbench` (this crate) — tiles + frames
- `orrery` (planned) — graph node / edge / canvas
- `gloss` (planned) — peripheral context strip
- system, murm, moot, etc. — as their UX surfaces land

Each module owns its UX concept's role mapping + uxtree projection.

## Status

Pre-1.0. Initial projection covers `WorkbenchProjection` blocks. Surface-
host references are exposed as a node `value`; the host fills in
bounds after layout.
