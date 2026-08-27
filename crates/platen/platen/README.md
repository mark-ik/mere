# platen

The pane composition home for the [mere](https://crates.io/crates/mere)
browser. Platen compiles a `forme::Arrangement` into the presentation plans a
host renders panes from: the workbench split tree, its tree projection, the
projection-geometry sidecar, and the document-canvas scene input for a pane
holding a document tile.

Platen is geometry-free. Concrete rectangles come from the host's genet layout;
platen emits structure and ratios.

## Modules

| Module | Contents |
| --- | --- |
| `tree_projection` | `ProjectionKind`, `TilePlan`, `PlanSlot`, `WorkbenchPlan`, `project_tree`, `tile_tree_from_plan` |
| `workbench` | `Workbench`, `SlotView`; the live split tree, plus the forme bridge |
| `projection_geometry` | `Axis`, `TreeGeometry`, `TreeBranch` |
| `document_scene` | `build_document_scene` |

Root re-exports: `build_document_scene`, `Axis`, `TreeBranch`, `TreeGeometry`,
`PlanSlot`, `ProjectionKind`, `TilePlan`, `WorkbenchPlan`, `project_tree`,
`SlotView`, `Workbench`, plus the `VERSION` and `STAGE` consts.

## Key entry points

| Item | Signature / role |
| --- | --- |
| `project_tree(&Arrangement) -> WorkbenchPlan` | Root-level tile-bearing members into ordered slots; `StackedWith` members collapse into a tab-stack. |
| `tile_tree_from_plan(&WorkbenchPlan, impl FnMut(&TilePlan) -> Tile) -> Option<TileTree>` | Projects a plan onto `genet_host_api::tile::TileTree`. One slot maps to a bare stack; several map to an even `Row` split. |
| `Workbench::to_tile_tree(impl FnMut(GraphMemberId) -> Tile) -> Option<TileTree>` | Same seam from the live split tree, preserving nesting and fractions. |
| `Workbench::slot_views() -> impl Iterator<Item = SlotView<'_>>` | Flattened leaf stacks (members, active index, parent fraction) for the a11y / automation projection. |
| `Workbench::to_arrangement() -> (Arrangement, Option<TreeGeometry>)` | Derives the canonical persisted pair; `from_arrangement` rebuilds the tree from it. |
| `Workbench::to_persisted_json` / `from_persisted_json` | JSON form of that pair, written beside the session graph. |
| `build_document_scene(&EngineDocument, Viewport, &DocumentStyleSheet) -> LaidOutDocument` | Wraps `document_canvas::layout_document` for a pane holding a document tile. |

`Workbench` mutation API covers `open_tile`, `open_split`, `open_stack`,
`open_in_slot_of`, `close_tile`, `activate`, `move_to_slot_of`, `split_beside`,
`split_beside_axis`, `split_out`, `stack_all`, `split_all`, `clear_tiles`, and
the divider reads/writes `weights` / `set_weights` / `split_fractions` /
`set_split_fractions`. Mode state is `mode`, `set_mode`, `toggle_mode`,
`is_tiled`, `ensure_tiled`; a new workbench starts in
`ProjectionKind::Cartography`.

## Dependencies

| Crate | Why |
| --- | --- |
| `forme` | Owns the `Arrangement` platen projects. Platen never mutates it. |
| `genet-host-api` (git, genet `main`) | `tile::{Tile, TileTree, TileBranch, SplitAxis, TileId, ContentSource}`, the host tile-surface contract platen projects onto. |
| `document-canvas` (git, genet `main`) | Document layout behind `build_document_scene`. |
| `inker` (git, genet `main`) | `EngineDocument`, the document scene input. |
| `serde` / `serde_json` | Plans and persisted `(Arrangement, TreeGeometry)` pairs. |
| `uuid` | `GraphMemberId`. |

## Decomposition, 2026-07-09

The graph-scene paint lane left platen for the `canvas` crate: `scene_paint`,
`cartography_scene`, `coupling_paint`, the underlay, and cartography geometry
now live in `crates/canvas/canvas`. Platen keeps the pane lane.

## Status

Pre-1.0 (`STAGE = "pre-alpha"`). `project_tree` surfaces root-level members
only; tiles nested inside an arrangement `Group` are not yet projected. Sibling
projections (lattice, corridor) are added when a surface needs one.

## License

MPL-2.0 (see LICENSE).
