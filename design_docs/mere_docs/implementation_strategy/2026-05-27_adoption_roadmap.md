# Mere Adoption Roadmap — sequencing the harvested design value

**Date**: 2026-05-27
**Status**: Synthesis / sequencing. Turns the design-pass *inventories* into an
ordered adoption plan: which mapped value lands when, gated by what, unblocking
what. Companion to — and consumer of — five docs:

- [composition spine](../technical_architecture/2026-05-21_mere_composition_spine.md) — the ontology (`kernel → forme → platen → verso → inker → host`).
- [component fit-map](../technical_architecture/2026-05-26_component_fit_map.md) — where each component sits *today* (live / latent / stub) + reconciliation questions.
- [donor docs harvest](../research/2026-05-27_graphshell_docs_full_harvest.md) — 633 donor design docs → design *value* + mere home.
- [donor repo salvage map](../research/2026-05-27_donor_graphshell_repo_salvage_map.md) — donor *code* → already-salvaged / latent / cut.
- [understory orrery-graduation brief](../research/2026-05-27_understory_orrery_graduation_brief.md) — borrow-boundaries for the scene/interaction layer.

---

## 1. The one idea this doc adds: *pulled ≠ wired*

The harvest and salvage map are **inventories** — they say *what* is valuable and
*where* it belongs, not *when* to adopt it. They also appear to give opposite
directives:

- The salvage map: **"don't wait for consumers — pull now"** (and it did: the
  whole `register-*` cluster, `misfin`, `webfinger`, `import`, JSON-Feed,
  Spartan/Titan are in the workspace, green).
- The fit-map / verso plan: **"don't build realization machinery ahead of a
  consumer."**

These aren't in conflict once you split the verb. **Pulling** portable code that
compiles and tests in isolation is cheap, reversible, and de-risks the
about-to-be-archived donor — do it eagerly (done). **Wiring** that code into the
live host — making a registry actually route, a preset actually drive physics, a
spec actually constrain a surface — is the part with real cost and real risk, and
*that* is what waits for a consumer slice. So:

> **Most of the value is already *pulled* and sitting latent. This roadmap
> sequences the *wiring*.**

Everything below is a wiring/adoption slice, not a salvage task. Salvage is done.

## 2. State of the three layers of value

| Layer | What it is | Status |
|---|---|---|
| **Live path** | What the running `mere-app` actually exercises | `kernel(+store) → forme(+store) → platen::{project_tree,layout} → panes.rs → inker → nematic`; hand-rolled orrery (`camera`/`graph_canvas`); **scrying web tile** (built this session); `kernel::store`/`forme::store` persistence |
| **Pulled, latent** | In-workspace, green, no live caller | `register-*` cluster (8: viewer/protocol/theme/lens/knowledge/input/layout/mod-loader/diagnostics), `import`, `misfin`, `webfinger`; verso-core/tile-state (keyed on `TileId` now); cartography/graph-layout/aether; document-canvas; the domain a11y crates (orrery/workbench/gloss/apparatus → uxtree) |
| **Doc-mapped** | Specs/invariants harvested, not yet code | the harvest's §1–§10 — a11y/temporal/permission invariants, focus/event/UxTree specs, frame-assembly/viewer-presentation, verse governance, etc. |

The roadmap's job: move things up this table in a sensible order.

## 3. The rungs (each a triggered slice, not a strict linear order)

Rungs are grouped by **trigger** (the consumer that pulls them live). R0 has no
gate; R1–R5 are ordered by my recommended near-term sequence, but R1–R3 are
substantially **parallelizable** — they touch different crates.

### R0 — Invariant contracts

Policy/contract language that **prevents silent corruption** and constrains
everything downstream. These are doc + type-level guardrails, not host wiring.
Highest leverage per the harvest §1.

**Feasibility finding (2026-05-29).** The original framing ("adopt now, no
consumer gate") held for only two of the five. Verifying each against the actual
crates showed three carry a real gate: the target crate is not in the workspace,
the type the contract constrains is not yet promoted, or the thing to constrain
does not exist yet (so it is net-new design, not a contract to adopt). Two were
adopted; three are gated with their trigger named below.

| Adopt | From (harvest) | Into | Status |
|---|---|---|---|
| Capability-declaration + non-silent-degradation + cross-surface-parity | `SUBSYSTEM_ACCESSIBILITY.md` | `inker` (consumer = verso/uxtree a11y bridge, R2) | **Done** (`0f884ae`). `inker::a11y::A11yCapability` {Opaque, Partial, Full}; `Engine` defaults Full, `SurfaceEngine` defaults Opaque. Non-breaking and already correct (nematic Full, scrying Opaque). Adopted into `inker`, not `platen` (platen disclaims a11y projection; the uxtree bridge consumes it). |
| Temporal-integrity + replay-isolation + shared-projection (Recent = projection, not a 2nd store) | `SUBSYSTEM_HISTORY.md` | `node-lineage` + `eidetic` | **Done** (`9a52852`). Doc-contract naming the three invariants both crates already embody (Engram immutability + content hash; visits own the tree, edges projected). Cross-referenced, donor cited. |
| Register-layer composition contract (explicit-bridge, no hidden cross-registry calls, diagnosable routing) | `register_layer_spec.md` | `register-*` cluster | **Gated**: the `register-*` crates are not in the workspace (the salvage map lists them as *to-pull*, not pulled). Trigger: adopt when the cluster is pulled (precedes R3 wiring). |
| Undoable / SoftUndoable / NotUndoable as **structural** | `command_semantics_matrix.md` | `kernel` mutation bus | **Gated**: `GraphMutation` is not yet promoted into kernel (`intents.rs` carries only portable primitives; the full vocab is in the donor's entangled `app/intents.rs`, slices 57b/c/d pending). Trigger: adopt at `GraphMutation` promotion, when it can shape the type. |
| Settings/permissions five-scope + **narrowing rule** | `settings_and_permissions_spine_spec.md` | `kernel` + `verso` | **Reclassified**: no permission model exists in kernel or verso, so this is net-new design (its own slice), not a contract to pin onto an existing type. Moves out of R0 to a dedicated permissions slice, triggered when surfaces/mods first need gating. |

Done (for the two adopted) = the contract language lives in the crate's docs/types
as invariants, donor cited per the DOC_POLICY incremental rule. The three gated
rows are tracked here with their triggers; they re-enter as their gate lifts.

### R1 — Orrery graduation (the next big *visible* slice)

**Trigger**: retire the hand-rolled `camera.rs` + `graph_canvas.rs` scaffolding
(the fit-map's named overlap: ~24 KB widget vs the 9.6k `graph-canvas` crate).
This rung converges the most pulled assets, so it's the highest-value next move.

**Prerequisite finding (2026-05-29).** The gating spike below *cannot lead*: it
measures over "a live physics layout," and that layout does not exist yet.
`aether` (394 LOC) is an unwired, field-less scaffold: `mere-app` has no `aether`
dependency, no `Field` is implemented (`NodeExclusion`/`EdgeSpring`/`Boundary` are
only named in a doc comment), and the world ticks empty (bodies settle under
damping alone). `query_pipeline` is held and fed to the step, but not exposed for
hit-test queries. The running app uses the hand-rolled canvas. So R1's true first
step is **R1a — aether graduation**, and only then **R1b — the spike**.

- **R1a — aether graduation** (new gate for the spike): depend on `aether` from
  `mere-app`; implement the core fields so a layout actually settles
  (`NodeExclusion` repulsion, `EdgeSpring` attraction, `Boundary` containment);
  expose `QueryPipeline` hit-test (point/AABB) on `Simulation`; drive a tick in
  the app. One confirmed-by-inspection asset: every node is already a rapier
  collider (`ColliderBuilder::ball`), so `QueryPipeline` hit-tests nodes for free.
- **Decide** graph-canvas-crate vs hardened hand-rolled widget (fit-map Q3) —
  *before* building, run the understory comparison the brief defines.
- **R1b — Spike** (gating, needs R1a): the rapier-`QueryPipeline` vs
  `understory_index` hit-test/cull bake-off over the now-live physics layout,
  measured against `canvas_behavior_contract` metrics (`crossing_density`,
  `label_overlap_ratio`, `edge_len_cv`). Decides whether a spatial index earns its
  place in the hot phase. (Question 2 — node/rapier redundancy — is already
  half-answered: nodes are colliders, so `understory_index` would earn its keep
  for non-collider visuals + cull, not node hit-testing, unless it wins as a
  single all-visuals index.)
- **Wire** `cartography` + `graph-layout` for real layout (replaces the seeded
  ring); `register-lens` presets onto the `scene_physics` runtime (the salvage
  map's named integration); `register-theme` edge styling (`edge_visual_encoding_spec`);
  `aether` (rapier) for the live/settling phase.
- **Borrow** (understory, steal-the-shape): `view2d` camera boundary; the
  box-tree/index seam *if the spike favours it*; `responder`/`focus` routing for
  in-canvas interaction (reconciled with Masonry: Masonry routes the chrome tree,
  this routes the scene inside the canvas widget).

Done = the orrery runs real layout + presets through a settled internal layering,
hand-rolled scaffolding retired or explicitly hardened-and-kept.

### R2 — Verso realization deepening

**Trigger**: multi-tile workbench / the scrying tile generalizing. Verso P0
(forme `TileId` reshape) and a first external surface (scrying) are **done**;
this continues the [verso adoption plan](2026-05-27_verso_adoption_plan.md).

- `WorkbenchTiling` widget (verso P1) — `platen::layout_plan` rects → placed,
  clipped child tiles (masonry-harness placement/hit tests).
- Focus/event/UxTree specs: `focus_and_region_navigation_spec` +
  `focus_state_machine_spec` (nine-region model, capture-stack survives modal/
  pane lifecycle), `semantic_event_pipeline_spec` (pure `GraphSemanticEvent →
  RuntimeEvent`, load-bearing for the scrying webview), `ux_tree_and_probe_spec`
  (three-layer `UxNode`, per-frame snapshot rebuild, LOD cutoff) → `verso` +
  AccessKit bridge.
- The domain a11y crates (orrery/workbench/gloss/apparatus → uxtree) wired to the
  bridge; `register-viewer` for content-kind → render-mode resolution
  (`viewer_presentation_and_fallback_spec`).

### R3 — Renderer/protocol/mod registry wiring

**Trigger**: a host dispatch seam + a second engine (serval) or mod lane. Makes
the latent `register-*` cluster *live*.

- `register-viewer` (content-kind → renderer routing) + `register-protocol`
  (scheme dispatch) into host/inker dispatch; `frame_assembly_and_compositor_spec`
  (three-pass Chrome→Content→Overlay, four render modes — the scrying tile is
  already the `CompositedTexture`/`NativeOverlay` case).
- `register-mod-loader` (six-phase lifecycle, rollback-first per
  `mod_lifecycle_integrity_spec`) for the native/WASM mod lane.
- `register-knowledge` (UDC tag validation) onto kernel `tags`; `register-input`
  as the host keybinding registry.

### R4 — Structural relocations (cleanup, do when convenient)

- Relocate `kernel` + `cartography` (+ `graph-layout`) out from under the
  `crates/graphshell/` supercrate (fit-map Q4 — kernel-under-chrome is upside
  down). Pure move; no behaviour change. Schedule between bigger slices.

### R5+ — Federation / intelligence tiers (later milestones)

- Verse governance model (harvest §7, ~70% transfers): VGCP structural-edges +
  privacy-filter-before-sign → `kernel`/`murm`; Genesis/Threshold/Delegated rule
  systems + revocation-as-read-time-projection → `persona` + `mooting`;
  `TransferProfile`/engram → `eidetic`; reputation ledger → `kernel` trust graph.
- Bilateral sync (verso harvest §8): VersionVector deltas / SyncUnit / conflict
  resolution → `tile-state` sync; presence overlay → `verso-core`.
- Intelligence taxonomy (four axes) → `intel` / `moothold` to prevent bucket-collapse.

These wire at their own tier milestones; listed so the roadmap is complete, not
because they're near.

## 4. Recommended near-term order

1. **R0 contracts** — two adopted (a11y `0f884ae`, temporal `9a52852`); the
   other three are gated on their triggers (see the R0 table) and re-enter as
   those lift, so they no longer block. R0 is effectively cleared for now.
2. **R1 orrery graduation** — most visible, converges the most pulled assets;
   lead with the understory/rapier spike since it gates the canvas-layer decision.
   This is the next move.
3. **R2 verso deepening** — parallelizable with R1 (different crates); the scrying
   tile already proved the seam.
4. **R3 registry wiring** — when a second engine or mod lane gives it a consumer.
5. **R4 relocations** — opportunistic cleanup between slices.
6. **R5+ federation/intel** — own milestones, not near.

## 5. The standing principles that gate all of it

- **Spine ontology is fixed**: adoption sharpens `kernel → forme → platen → verso
  → inker → host`; nothing here adds a parallel spine (the understory "no second
  presentation tree" call is the canonical example).
- **Pull eagerly, wire on consumer** (§1): the reconciliation above.
- **Borrow boundaries, not crates, when youth is a risk** (understory): steal the
  shape unless a probe earns a pin.
- **Incremental migration** (DOC_POLICY): pull spec detail from the cited donor
  doc *when a slice lands*, not up front; the harvest/salvage maps are the index.
- **Adopt / retire / keep-latent is a deliberate call per overlap** (fit-map):
  the hand-rolled pieces are scaffolding to retire as the crates wire in.
