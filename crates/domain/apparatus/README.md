# apparatus

Apparatus domain layer for the [mere](https://crates.io/crates/mere) browser:
the peripheral system-inspector strip. Package `mere-apparatus`, library
`apparatus`.

## API

| Item | Role |
| --- | --- |
| `project_skeleton() -> uxtree::UxTree` | Emits the v0 skeleton subtree. Takes no input. |
| `VERSION`, `STAGE` | Crate version string and lifecycle marker (`"pre-alpha"`). |

## Node shape

```text
apparatus (Role::Group, label "Apparatus")
  ├─ tracing events               (Role::Group, empty)
  ├─ register-diagnostics channels (Role::Group, empty)
  ├─ uxtree                       (Role::Group, empty)
  └─ accesskit                    (Role::Group, empty)
```

Node ids come from `uxtree::node_id_for_path`: `apparatus` for the root,
`apparatus/section/{label}` for each section. Ids are stable across runs.

## Dependencies

`accesskit`, `uxtree`, `tracing`.

## Status

Pre-1.0. Sections are empty placeholders. Each fills in as its host bridge
lands: a tracing-subscriber bridge for events, `register_diagnostics`
channel taps, a snapshot of the running uxtree, and a walk of the current
accesskit tree.
