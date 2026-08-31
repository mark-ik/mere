# workbench

Temporary compatibility crate for the former Workbench domain projection.

Platen now owns the tiled-layout split tree and its
[AccessKit](https://accesskit.dev)/`uxtree` structural projection. This package remains only so
existing callers can keep importing `workbench::project_workbench` until Genet's component crate
replaces it.

## API

| Item | Role |
| --- | --- |
| `project_workbench(&platen::Workbench) -> uxtree::UxTree` | Compatibility re-export of `platen::accessibility::project_tile_layout`. |
| `VERSION`, `STAGE` | Crate version string and lifecycle marker (`"pre-alpha"`). |

## Dependencies

`platen` only.

## Status

Temporary. Remove this shim when the Genet `workbench` component is available in the unified
integration.
