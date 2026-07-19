# platen

`platen` is the workbench composition surface for the
[mere](https://crates.io/crates/mere) browser: it compiles a forme
arrangement into a presentation plan and renders graph projections into
paint. In the printing-press metaphor the platen is the press; the host
prints what it presses.

## What's in the crate

- **`tree_projection`** — platen's core role under the composition spine:
  compile a forme `Arrangement` into a `WorkbenchPlan` (splits of
  tab-stacks: `PlanSlot`, `TilePlan`, `project_tree`).
- **`workbench`** — the tiled-workbench model (`Workbench`, `SlotView`):
  slots of tab-stacks, active tab per stack, projection mode. Geometry-free;
  concrete rects come from `platen-view`'s flex DOM laid out by genet.
- **`cartography_scene`** — cartography projection-request derivation and
  layout-strategy dispatch (the `arrangements` catalog seam).
- **`scene_paint`** / **`orrery`** — render a cartography `Projection` /
  the session graph into a host-agnostic `paint_list_api` paint list (the
  orrery's scene underlay, consumed by netrender on any host).
- **`coupling_paint`** — visual couplings as paint overlays (the
  aether→platen seam).
- **`document_scene`** — wraps `document-canvas` so a pane holding a
  document tile (a nematic engine's output) gets a render packet with the
  same shape as a graph swatch.

## How it relates to other workspace crates

```text
 forme::Arrangement ──► platen (tree_projection / workbench)
                          │ WorkbenchPlan / Workbench
                          ▼
                     platen-view ──► genet flex DOM ──► netrender
 kernel::Graph ──► platen (orrery / scene_paint)
                          │ paint list (underlay)
                          ▼
                     netrender Scene
```

- **`forme`** — owns the arrangement platen projects; platen never mutates it.
- **`platen-view`** (`crates/platen/view`) — the genet-coupled view layer;
  platen-core stays engine-free.
- **`paint_list_api`** — the engine-neutral paint vocabulary platen emits.
- **`document-canvas`** / **`cartography`** / **`arrangements`** — the
  swatch and projection engines platen dispatches into.
- *Verso* is not a layer below platen: it names the engine-flip /
  compatibility-view seam (see `design_docs/verso_docs/`).

## Status

Pre-1.0. Tree projection, the workbench model, and the orrery paint
underlay are live in the meerkat host; sibling projections (lattice,
corridor) are added when a surface needs one.

## License

MIT OR Apache-2.0.
