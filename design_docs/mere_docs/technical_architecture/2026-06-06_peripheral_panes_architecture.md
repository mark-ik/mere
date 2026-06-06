# Peripheral panes — the dock architecture

**Date**: 2026-06-06
**Status**: Architecture decision (design seed; the catalog will grow). Generalizes the docked-pane pattern the [comms shell plan](../implementation_strategy/2026-06-05_comms_shell_plan.md) established into the full pane system. Per the [composition spine](2026-05-21_mere_composition_spine.md) and the [modular integration plan](../implementation_strategy/2026-06-02_modular_integration_plan.md) (gaps #6 panes, #7 comms).

## The one idea

Peripheral panes are **contingent projections of the graph**, sharing **one dock mechanism**, but they are **not one product object** — each is its own thing with **one primary authority**. They are emphatically **not graph tiles** (a tile projects a node's media; a pane projects selection / runtime / social state), and **none invents its own panel system** — they all ride the same dock contract and the same domain → host render pattern as `chrome` and `comms`.

This keeps the spine's invariant intact: the graph is root, the orrery is its spatial surface, tiles project nodes, and panes hang off the graph as contingent peripheral surfaces. **No pane is ever the application root.**

## The dock contract

A pane is described by a small, shared contract — the first dock API:

- **`PaneKind`** — which pane (Gloss, Apparatus, Steward, …).
- **open / closed**
- **side** — dock edge (left / right / bottom).
- **width** (or height for a bottom dock).
- **focus** — whether it holds input focus.
- **context binding** (optional) — the selection / graph the pane follows, for the graph-contingent panes.

Panes are **per-window** (like the [multi-window synced panels](../../mere_docs/implementation_strategy/2026-06-04_multi_window_plan.md) model: window-specific, graph-bound, tearable), reusing the chrome's separate-roots discipline (a pane is a docked root composited beside the content-root, not a tile inside it).

## The catalog (grouped by primary authority)

**Graph / selection authority** (follow the selection):
- **Gloss** — selected-node / selected-graphlet marginalia: notes, annotations, quotes, backlinks, reader comments. Graph-contingent, follows selection. Expresses the Verse/Mere thesis better than generic browser chrome.
- **Inspector** — selected object's properties: node fields, URL / content identity, lineage, edge taxonomy, permissions, provenance, local cache state. The "what is this thing?" pane graph-first UX needs always available. (Felt missing; important.)
- **Index / Spine** — navigational outline: graphlet tree, open tabs, history, saved workbenches, search results, recently touched. More persistent than the command palette.

**Runtime / actor authority**:
- **Steward** — user-facing async operations: fetches, uploads, sync, pinning, storage repair, model jobs, mesh jobs, background tabs, failed retries. Operational/status UI — makes async Mere legible.
- **Apparatus** — diagnostic/developer trace: event log, actor deaths/restarts, fetch/sync traces, render timings, protocol warnings. "What is the machine doing?" Distinct from Steward (status vs trace) and from Settings (not config).

**Social / protocol authority**:
- **Comms** — conversations + social streams: murm cabals, misfin, future mooting adapters. (First docked pane; see the comms shell plan.)
- **Ledger / Concord** — trust + governance: tessera, reciprocal credit, peer standing, capabilities granted/received, moot membership. May start inside Steward; long-term may earn its own pane.

**User / config authority**:
- **Settings** — persistent configuration. Small settings stay an overlay; repeated configuration graduates to a docked utility pane.
- **Lens / Filters** — persistent view controls over the graph: edge-family visibility toggles, the active layout strategy, the lens. The dock-able slice of what was loosely "Tools" (may instead fold into Index/Spine). The *rest* of "Tools" is not a pane (below).

## Not panes: the mode band and the palette

"Tools" as one pane conflates three things; only the first earns dock citizenship:

- **View lens / filters** — persistent, user-managed (above): a real pane (or folded into Index/Spine).
- **Mode manipulators** — graph-editing handles and per-mode controls. These are **owned by the active scene-mode** (the Browse / Arrange / Simulate FormFactor), not the dock: they appear and change *with the mode*, so they render as a contextual **band** near the content, not a side-docked pane. A `PaneKind` whose open/closed and contents are driven by the mode (rather than the user + its own authority) would break the dock contract. The band is to the *mode* what panes are to the *graph*.
- **Verbs** (import / export, …) — actions, not surfaces. They live in the command palette (and context menus), not a tool pane.

The test: if a surface's authority is "whatever the mode says," it is the mode's band, not a dock citizen.

## Sequencing

After **Comms** (in flight), the first three:
1. **Steward** — makes async Mere legible (reads the armillary actor constellation: content/fetch/sync actors + pool; per the [real-sync-feedback rule](../../) it must show genuine status, never a placebo).
2. **Inspector** — graph-first UX needs "what is this thing?" always available (and it's where the node-lineage work surfaces: within-node history + the lineage forest).
3. **Gloss** — the Verse/Mere thesis.

**Apparatus** can be built early but hidden behind a debug flag — it will save constant time as the actor/runtime work deepens. Its cleanest first live feed is `ux-events`, not the raw 253-channel `register-diagnostics` catalog.

## Donor harvest per pane (shapes, not crates to revive)

- **register-theme → harvest now** for edge styling (P3 of the [nav-lineage plan](../implementation_strategy/2026-06-05_node_navigation_lineage_wiring_plan.md)). `edge_style.rs` (`EdgeStyleToken`, non-color signatures, endpoint markers, validation) is directly useful. **Correction:** the live kernel has **six** families (incl. `Provenance`); the donor has five — map forward, don't copy the enum. First insertion: `platen/platen/src/canvas_scene.rs` (~`family_color`), which already keeps `RelationKind`/tags.
- **register-diagnostics → Apparatus** (not bulk adoption): the `emit` scaffold is serviceable; prune the old-host channel set around current `meerkat`/`armillary`/`inker`/`platen` terms. `ux-events` is the cleaner first feed.
- **register-input → latent** (real binding/remap/conflict machinery): adopt when **Settings** grows shortcut editing, not before.
- **register-lens → latent but valuable**: translate the named physics profiles into `gyre` / `graph-layout` settings; do not recreate a lens registry as an authority (gyre owns runtime force integration).
- **register-mod-loader → reference only**: good future vocabulary (manifests, dependency order, rollback, quarantine), but no extension system exists yet — importing now would be fake architecture.
- **register-viewer → reconcile then delete**: `inker::routing` stays canonical; harvest only magic-byte fallback + capability/conformance metadata, not its selector.
- **register-renderer-types → cut**: a manifest pointing at a missing `src/lib.rs`, encoding the killed renderer-registry/host-port model; no live owner.

## Open wrinkle (affects edge-styling panes / P3)

`platen::orrery` currently collapses all relations between a node pair into **one** drawn line (`platen/platen/src/orrery.rs`). Family-accurate edge styling needs either preserving `RelationKind` into the projection, or accepting "one representative style per pair" for now. Start P3 in `canvas_scene` (it keeps `RelationKind`/tags); fix the orrery collapse when layered relation edges matter visually.
