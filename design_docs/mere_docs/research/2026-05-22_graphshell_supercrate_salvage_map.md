# graphshell Supercrate Salvage Map

**Date**: 2026-05-22
**Status**: Decomposition inventory of the in-workspace `crates/graphshell/` supercrate.
**Scope note**: this maps the **current in-workspace supercrate**, not the archive-bound donor repo at `repos/graphshell` (that one is covered by [`2026-05-24_external_deps_topology_brief.md`](../../2026-05-24_external_deps_topology_brief.md)).
**Companion**: the [retired host-stack salvage map](2026-05-22_retired_host_stack_salvage_map.md) (the host/renderer cut) and the [app re-scaffold](../technical_architecture/2026-05-21_app_architecture_rescaffold.md).

---

## Why this exists

`crates/graphshell/` is the supercrate the graphshell → Mere migration left behind: a grab-bag of ~18 crates spanning the graph layer, the shell domain, and the substrate-host system. As the substrate-host era ends (the re-scaffold), the supercrate is decomposing. This map classifies every member so the decomposition is deliberate, and so the eventual shrink (kernel + cartography re-home out; `graphshell` collapses toward `mere-domain/graphshell` + `mere-host/graphshell`) has a starting inventory.

**Classification principle:** *superseded → cut; not-yet-rewired → keep.* A crate the re-scaffold explicitly replaced is cut (with salvage recorded). A crate the new host will plausibly want but has not wired yet is kept as latent salvage, the same call made for `session-runtime`.

## Live path / canonical (keep; relocate out of the supercrate eventually)

These are reached from `mere/app` (via `platen`, `frame`) or are canonical by intent. The standing direction is to re-home `kernel` and `cartography` out from under `graphshell` (kernel-under-chrome is upside down) as the supercrate shrinks.

| Crate | LOC | Role | Note |
|---|---|---|---|
| `kernel` (graph-kernel) | 12631 | the identity / authority / mutation kernel | **relocate out** — canonical, should not live under a shell supercrate |
| `cartography` | 1016 | projection layer, `LayoutStrategy` | used via `platen`; relocate candidate |
| `graph-canvas` | 9608 | scene derivation, camera, hit-test, render-packet IR | used via `platen`; overlaps the new hand-rolled GraphCanvas widget (future reconciliation) |
| `graph-layout` | 9643 | layout algorithms (`Layout<N>`, analytic + streaming) | used via `cartography` |
| `frame` | 880 | savable resizable-pane layout + `GraphId`, uxtree projection | direct `mere/app` dep |
| `orrery` | 144 | projects kernel graph into uxtree (a11y / automation) | reached via `frame`; domain layer |
| `node-lineage` | 1540 | owner-scoped navigation lineage (visit-level branching) | keeper by intent (lineage/forme rename plan); currently latent |

## Superseded — cut with the host stack

Detail and per-module salvage live in the [retired host-stack salvage map](2026-05-22_retired_host_stack_salvage_map.md).

| Crate | LOC | Cut because |
|---|---|---|
| `spatial-substrate` | 3444 | the SubstrateScene IR; masonry scene replaces it (salvage: external-texture, LOD) |
| `host-ports` | 3239 | host-abstraction ports; masonry is the port layer |
| `register-renderer` (+`-types`) | 1704 | renderer-registry contract; masonry `Widget` replaces it |
| `control-plane` | 665 | the action bus; Xilem messages replace it (cut deferred; salvage: permission gates) |

## Not-yet-rewired — keep as latent salvage

The host cut leaves these consumer-less (their only dependents were the host stack), but the re-scaffold did **not** supersede them: a real Xilem host will want chrome view-models, UX telemetry, and a diagnostics taxonomy. Keep them in place; rewire or relocate (toward `mere-domain` / `mere-host`) when their slice lands, rather than delete.

| Crate | LOC | Substance | Revive for |
|---|---|---|---|
| `chrome` | 2446 | toolbar / omnibar / app-menu / window-chrome view-models | `mere/app`'s real chrome (omnibar, toolbar) |
| `shell-state` | 176 | shell session state aggregator (focus, palette, omnibar, toolbar, frame view-models) | with `chrome` |
| `ux-events` | 1998 | UX event taxonomy + telemetry (`UxEvent` / `UxObserver` / `UxProbe`, diagnostics bridge) | instrumenting the new chrome |
| `register-diagnostics` | 2934 | keystone diagnostics registry: 253 channel-name constants + `DiagnosticsRegistry` | the new host's instrumentation; was the unblocker for other registry extractions |
| `aether` | 394 | rapier-backed physics (bodies / forces / fields bound to nodes) | streaming / force-directed layout in the GraphCanvas |

`session-runtime` (2569 LOC, session manifests / view-intent / engine-profile / switcher-thumbnail) is also keep-and-rewire, but it sits on the host-stack boundary and is tracked there.

## Trajectory

- **Now:** cut the superseded host stack (separate pass; see companion map). The not-yet-rewired cluster goes orphaned-but-kept.
- **As slices land:** rewire the latent crates into `mere/app` (chrome, diagnostics, telemetry) or relocate them toward `mere-domain` / `mere-host`; re-home `kernel` + `cartography` out of the supercrate.
- **End state:** `graphshell` shrinks to a `mere-domain/graphshell` (domain concept) + `mere-host/graphshell` (host wiring) pair, with the canonical graph + layout crates living at the top level, not under a shell.
