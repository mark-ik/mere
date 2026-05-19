# graphshell

`graphshell/` is a **supercrate directory** — a semantic grouping of the
graph and shell crates that together form the chrome layer of the
[mere](https://crates.io/crates/mere) browser. Each sub-crate is a real
workspace member that can be built and tested in isolation; the top-level
directory itself is not a crate.

## Layout

```text
crates/graphshell/
├── graph/            # The graph half (data + spatial substrate)
│   ├── aether/             — top-level paint/layout primitives
│   ├── cartography/        — ProjectionRequest / ViewIntent / Projection
│   ├── graph-canvas/       — canvas scene IR (CanvasSceneInput, FrameRegion)
│   ├── graph-kernel/       — kernel crate (graph + geometry + identity)
│   ├── graph-layout/       — LayoutStrategy adapters (grid, force-directed, …)
│   ├── node-lineage/       — owner-scoped navigation lineage
│   ├── orrery/             — tiered-graph framework primitives
│   └── spatial-substrate/  — substrate scene + dispatch + camera
│
└── shell/            # The shell half (chrome + runtime + registries)
    ├── domain/
    │   ├── chrome/         — toolbar / omnibar / palette / authorities view-models
    │   └── frame/          — FrameLayout + PaneContent + frame projections
    ├── host-ports/         — host-port trait vocabulary
    ├── session-runtime/    — session graph store, manifest store, view-intent store
    ├── state/              — shell-state crate (re-exports chrome modules + ux probes)
    ├── system/
    │   ├── control-plane/  — control-plane action bus
    │   └── registry/
    │       ├── register-diagnostics/
    │       ├── register-renderer/         — renderer-registry trait + dispatch
    │       └── register-renderer-types/   — wasm32-clean data-only types
    └── ux-events/          — UX event vocabulary
```

The supercrate split is also a semantic split: the **graph half** owns
data and spatial structure (kernel, projection, canvas IR, substrate);
the **shell half** owns chrome view-models, runtime contracts, and the
registries the host plugs renderers into.

## Status

Pre-1.0. The reorganization into the current topology landed in May 2026
(commit `32e7207` and the B1–B7 supercrate naming passes that followed).
