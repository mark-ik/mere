# workbench

Workbench domain layer for the [mere](https://crates.io/crates/mere) browser.
Projects `platen::Workbench` (the tiling split tree) into a subtree of
[AccessKit](https://accesskit.dev) nodes for `uxtree`.

## API

| Item | Role |
| --- | --- |
| `project_workbench(&platen::Workbench) -> uxtree::UxTree` | Builds the subtree. The host stitches `tree.root` under its application or window root. |
| `VERSION`, `STAGE` | Crate version string and lifecycle marker (`"pre-alpha"`). |

## Node shape

Built from `Workbench::slot_views()`, so the tree is flattened to leaf stacks in
order.

```text
workbench (Role::Group, label "Workbench")
  └─ slot (Role::Group, label "Slot {index}")       one per leaf stack
       └─ tab (Role::Tab, label "Tile {member}")    one per member in the stack
```

The active tab of each slot carries `description = "active"`.

Node ids come from `uxtree::node_id_for_path` over these domain paths:

| Node | Path |
| --- | --- |
| Root | `workbench` |
| Slot | `workbench/slot/{slot_index}` |
| Tab | `workbench/slot/{slot_index}/tab/{member}` |

The same workbench always produces the same ids.

## Dependencies

`accesskit`, `platen`, `uxtree`, `tracing`. Dev-dependencies: `kernel`, `uuid`.

## Sibling domain crates

`mere-apparatus` (inspector strip), `mere-gloss` (peripheral outline and
minimap), `mere-roster`, `mere-trail`. Each owns one UX concept's role mapping
and uxtree projection; the host merges them with `uxtree::stitch`. Graph-canvas
a11y is host-side.

## Status

Pre-1.0. Structure only: slots, tabs, and the active marker. Resolved titles and
URLs from the graph are not projected yet, and bounds are filled in by the host
after layout.
