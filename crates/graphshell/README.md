# graphshell

`graphshell` is the portable shell layer for the
[mere](https://crates.io/crates/mere) browser. It owns the workbench, the
tile tree, the Navigator, and the contracts to whichever GUI framework hosts
the app on a given platform, so the same shell semantics ship across native
desktop, browser-extension, browser-tab/PWA, and mobile envelopes.

## What's in the crate

- **`app_state`** — reducer-owned `GraphWorkspace`, typed pending effects,
  and the service-trait surface that hosts implement. Sub-modules:
  `intent_system`, `graph_runtime`, `workspace_routing`, `composition`,
  `app_ux`, `persistence`, `services`.

- **Service traits**: `WorkspaceRepository`, `SettingsStore`,
  `GraphMutationJournal`, `EngineRouter`, `SurfaceHost`, `DiagnosticsSink`,
  `Clock`, `TaskRuntime`. Concrete stores, engine routers, and host adapters
  implement these traits; reducers stay pure.

- **Re-exports of the workspace-internal shell-substrate crates** (consumed
  inside the mere workspace; not separately published):
  - `graphshell::core` ← `mere-kernel` — portable vocabulary + port-trait
    definitions.
  - `graphshell::runtime` ← `mere-host-contract` — runtime + host-port
    surface, frame projections, finalize-action plumbing.
  - `graphshell::memory` ← `graph-memory` — owner-scoped graph memory model.
  - `graphshell::tree` ← `graph-tree` — graphlet-native tile tree.

## How it relates to other workspace crates

graphshell sits between [`mere`](https://crates.io/crates/mere) above and the
press-stack peers below; host adapters plug in from the side.

```text
                            mere
                              │ composes
                              ▼
       host adapters  ──→  graphshell  ──→  mere-kernel / -runtime
       (iced, gpui, …)         │            (workspace-internal substrate)
                ┌──────────────┼──────────────┐
                ▼              ▼              ▼
              inker          platen        verso-tile
                                                │
                                                ▼
                                            eidetic
                                       (via app_state effects)
```

- [`mere`](https://crates.io/crates/mere) — composes graphshell into the
  product entrypoint.
- [`inker`](https://crates.io/crates/inker) — `app_state` emits
  `EngineRouteRequest` effects; inker returns `EngineRouteDecision` /
  `SurfaceContract`.
- [`platen`](https://crates.io/crates/platen) — `app_state` consumes platen's
  `WorkbenchProjection` and `ArrangementSnapshot` selectors over reducer-owned
  state.
- [`verso-tile`](https://crates.io/crates/verso-tile) — `app_state` emits
  `SurfaceCommand` effects and tracks `SurfaceLifecycleState`.
- [`eidetic`](https://crates.io/crates/eidetic) — `app_state` emits
  `eidetic::Request` effects routed through the `eidetic::Store` service trait.
- **Host adapters** (iced, gpui, html/css, makepad, egui) — implement
  graphshell's service traits; they consume graphshell rather than appearing
  as deps.

## Status

Pre-1.0. The reducer + service-trait surface is in place and tested
in-workspace; concrete host adapters land once the portable contracts
stabilize.

## Fun Fact

This crate's name was the prior name of this browser project for months!
I couldn't come up with a good name for the browser, so I adopted
servoshell's naming semantics, since I'd forked it from servoshell
in the first place. Now it is appropriately the name of the shell component
in the browser, and the browser itself has a much better name.

## License

MPL-2.0.
