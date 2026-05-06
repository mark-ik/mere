# Graphshell Migration Plan

**Date**: 2026-05-06
**Status**: Active / canonical migration direction
**Scope**: Move the current `graphshell` codebase into the `mere` workspace and associated crates without preserving obsolete product naming, renderer ownership, or host/runtime coupling. This plan is the bridge between the existing `graphshell` decomposition work and the new Mere crate taxonomy.

**Related**:

- [`../../../README.md`](../../../README.md) — current Mere workspace crate roles
- [`../../2026-05-04_lexicon_brief.md`](../../2026-05-04_lexicon_brief.md) — current product/component vocabulary
- [`../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`](../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md) — protocol/identity/transport architecture that Graphshell must consume, not duplicate
- Inherited: [`../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/2026-05-01_workspace_architecture_proposal.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/2026-05-01_workspace_architecture_proposal.md) — current root-crate decomposition plan and registrar/system-layer receipts
- Inherited: [`../../../../serval/docs/2026-05-05_serval_netrender_cut_plan.md`](../../../../serval/docs/2026-05-05_serval_netrender_cut_plan.md) — Serval/Netrender imposed renderer shape
- Inherited: [`../../../../netrender/netrender-notes/2026-05-04_feature_roadmap.md`](../../../../netrender/netrender-notes/2026-05-04_feature_roadmap.md) — Netrender compositor handoff roadmap
- [`2026-05-06_graphbrowserapp_donor_inventory.md`](2026-05-06_graphbrowserapp_donor_inventory.md) — migration gate for classifying donor `GraphBrowserApp` methods before import

---

## 0. Licensing Baseline

Mere and the associated crates are **MPL-2.0**.

This matches the existing `graphshell` crate license and removes the biggest mechanical/legal obstacle to moving files across: migrated Graphshell files can keep their existing MPL headers and remain under the same file-level copyleft model. The previous `MIT OR Apache-2.0` scaffold was a crate-reservation default, not the intended long-term project license.

Practical meaning:

- Modifications to MPL-covered files stay MPL when distributed.
- Larger applications can still link/use the crates without the whole larger work becoming MPL.
- The migration can preserve existing file headers instead of rewriting legal boilerplate.
- New files in this workspace should use the same MPL header once they contain substantive code.

---

## 1. Architectural Refinements to Impose

### 1.1 Mere Is the Proof Root

`repos/mere` is the build/test proof root for migration work. The old `repos/graphshell` workspace is the donor until its portable pieces compile inside Mere.

Reason: the Mere workspace already builds and tests cleanly, while the old Graphshell workspace is mid-refactor and still carries stale path assumptions around the Servo/Serval transition.

Done condition for each migration slice: `cargo test --workspace` passes in `repos/mere`, or the slice records a precise blocker here.

### 1.2 Portable Crates Before App, App Before Host

Migration order is structural:

1. Portable primitives and registries.
2. Pure workspace/reducer/app state.
3. Services behind traits.
4. Host adapters and desktop shell.
5. Renderer/engine adapters last.

No desktop host module should move before the portable crate it depends on has a Mere home. No old root-crate `GraphBrowserApp` service should move until its durable state and side effects are split.

### 1.3 Keep the Existing App Module Cuts

The current Graphshell refactor already grouped the app layer into useful seams:

- `intent_system`
- `graph_runtime`
- `workspace_routing`
- `persistence`
- `composition`
- `app_ux`
- `storage_interop`

Those groups become the migration map. Do not flatten them back into a root `graph_app.rs`, and do not split them into many crates unless a real consumer needs a smaller dependency surface.

### 1.4 Reassign Old Verso Responsibilities

Old `graphshell/crates/verso` is not moved wholesale.

Destination by responsibility:

| Old responsibility | Mere home |
| --- | --- |
| Engine choice, content routing, engine lifecycle | `inker` |
| Graph-aware layout/composition | `platen` |
| Tile/surface receiving and placement in `GraphTree` | `verso-tile` |
| Host integration and GUI surface contracts | `graphshell` / host crates |

This preserves the printing-press architecture instead of carrying the old overloaded `verso` crate forward.

### 1.5 Nematic Preserves Source Semantics

The old `middlenet-*` crates migrate under the `nematic` family, but the migration must preserve the protocol-faithful document lanes:

- Direct/source-faithful lane first for Gemini, Gopher, Scroll, Markdown, feeds, and other smolweb formats.
- HTML lane only where HTML is actually the right intermediate.
- Serval/Wry fallback only for browser-managed content.

Do not turn Nematic into "everything becomes HTML."

### 1.6 Netrender/Serval Replaces the Old Render Stack

Do not port Graphshell's old GL/WebRender/surfman render assumptions. The renderer path is:

```text
inker -> Serval or Nematic or Wry -> platen -> verso-tile -> graphshell host
```

For full web content, Serval's target is Netrender-driven rendering with native-compositor handoff. Graphshell should consume that surface; it should not preserve obsolete WebRender ownership or compatibility plumbing.

### 1.7 Reducer-Owned Durable Mutation

All durable graph mutation continues to flow through typed deltas/reducers. `GraphDelta` and the arrangement bridge are the model to extend. Host adapters, renderer callbacks, and protocol mods must not gain direct write access to graph truth.

Known seam to watch during migration: persisted navigation memory and history updates must stay on the canonical mutation lane.

### 1.8 Protocol Crates Own Protocol Work

Graphshell does not absorb protocol runtime again.

- `mere-identity` owns identity vault/storage.
- `mere-transport` owns iroh/blobs/gossip/docs surfaces.
- `murm` / `murmuring` own Cable and bilateral/co-op comms.
- `moothold` / `mooting` own community/federation/primitive moot nodes.
- Graphshell owns the UI shell and applets that consume those crates.

### 1.9 Ownership Boundaries During Migration

The migration should preserve donor module seams, but the seams are not all
long-term owners. Treat the first Mere-side app-state modules as proof slices
and anti-corruption boundaries until the destination crate has enough real
surface area to own the behavior directly.

| Owner | Owns | Must not own | Migration implication |
| --- | --- | --- | --- |
| `graphshell-core` | Portable graph/domain model, shell identities, pane IDs, address/content classifications, serializable shell state primitives | App reducers, concrete hosts, engine routing policy, persistence stores | Keep graph truth and stable IDs here when they are reusable below the app layer |
| `graph-canvas` / `graph-tree` | Canvas scene packet types, projection math inputs, tree topology/navigation primitives | `GraphWorkspace`, product chrome, engine/host decisions | Graphshell may select state into these types; these crates should not learn Graphshell app state |
| `graphshell::app_state` | Reducer-owned `GraphWorkspace`, pure reducers, transient/durable UI state, typed effects, temporary service traits | Concrete services, renderer handles, task runtimes, host widgets, engine lifecycle, storage implementations | Keep Phase 3 code here only while it is pure state/reducer/selector logic; `app_state::services` now proves the full pending-effect queue can dispatch through traits without importing donor `GraphBrowserApp` |
| `graphshell` host crates | Native/desktop/mobile host adapters, event translation, surface command application, accessibility bridge | Graph truth mutation, engine selection policy, private memory persistence | Hosts emit intents/effects and consume projections; they do not bypass reducers |
| `inker` | Engine choice, URI/content routing policy, engine lifecycle, engine-output contracts | Graphshell chrome, GraphWorkspace mutation, tile placement | `inker::routing` now owns the portable engine-route request/decision vocabulary and a default scheme-based `EngineRoutePolicy`; concrete engine implementations still remain outside the policy |
| `platen` | Graph-aware composition/layout policy, frame/tile arrangement projection, layout constraints, renderable workbench model | Engine lifecycle, host widget handles, durable graph mutation | `platen::canvas_scene` and `platen::workbench` now own the extracted graph-to-canvas derivation core, frame/pane model, active-frame/root-view selectors, pane/frame binding helpers, active-workbench projection packets, frame arrangement snapshots, and the projection from arrangements into hosted surface placements; Graphshell should keep shrinking toward wrappers around those selectors/helpers |
| `verso-tile` | Rendering surface identity, tile-slot placement, surface receiving/lifecycle between inker/platen/host | Engine selection, GraphWorkspace graph mutation, product chrome | `SurfaceTargetId`, `SurfaceHostId`, `SurfaceEffect`, `SurfaceRequest`, `SurfaceCommand`, `SurfaceCommandOutcome`, `SurfaceCommandBacklog`, `TileSlot`, `SurfaceSlotPlacement`, `SurfacePlacementPlan`, and the generic `SurfaceCommandSink` host seam now live here; grow richer surface lifecycle/types here instead of pushing them back into `inker` or Graphshell |
| `mnem` | Private local memory: graph snapshots, traversal logs, settings/cache/index persistence | Mere transport state, Moothold community flora, host UI state | The contract now lives in the dedicated `graphshell::mnem` module; a narrower crate boundary can come later if it proves necessary |
| `mere` | Product entrypoint, top-level service wiring, crate orchestration, feature assembly | Low-level graph primitives, renderer internals, protocol implementations | `mere` composes the system; it should not become the old root-crate catch-all |

Naming warning: the donor `composition` module is broader than the long-term
Mere owner. The initial `graphshell::app_state::composition` module should stay
limited to pure selectors such as workspace-to-canvas scene input. Anything
that decides arrangement, split topology, surface placement, or renderable
workbench layout should move toward `platen` / `verso-tile` instead.

---

## 2. Migration Sequence

### Phase 0 — Baseline and License Correction

| Task | Outcome |
| --- | --- |
| Switch Mere workspace license to MPL-2.0 | Cargo workspace and README reflect intended license |
| Add MPL license text | Repo has a concrete license file for new consumers |
| Record this plan in DOC_README | Migration has a canonical planning surface |
| Confirm Mere workspace tests | `cargo test --workspace` passes before code movement |

### Phase 1 — Portable Graphshell Crates

Move or recreate these first, preserving names unless Mere terminology requires a crate rename:

- `graph-memory`
- `graph-tree`
- `graph-canvas`
- `graph-cartography`
- `graphshell-core`
- `graphshell-runtime`
- `graphshell-comms` only after deciding what remains UI-side versus `murm` / `moothold`
- `registrar/*`

Acceptance: each crate compiles in Mere without depending on old `graphshell` root modules.

### Phase 2 — Graphshell Facade Crate

Replace the current placeholder `crates/graphshell` with a facade over the migrated portable crates:

- shell/workbench/Navigator vocabulary
- host/runtime boundary re-exports
- frame input/view-model contracts
- no desktop host dependency by default

Acceptance: `crates/graphshell` is still light enough to compile as a library target without Serval, Wry, iced, GPUI, or platform WebView dependencies.

### Phase 3 — App State and Reducer

Move the pure app layer in the current module-cut shape:

- `intent_system`
- `graph_runtime`
- `workspace_routing`
- `composition`
- `app_ux`

Keep `GraphWorkspace` ahead of `GraphBrowserApp`. `GraphWorkspace` is the durable/serializable state; `GraphBrowserApp` remains composition/service glue until services are trait-separated.

Acceptance: app-state tests run without launching a desktop host.

#### Phase 3 Preparatory Contract — GraphWorkspace First

Before moving app code, give the reducer slice a target shape. The next
migration layer is app state, not host composition.

`GraphWorkspace` becomes the reducer-owned state envelope. It should be
constructible, serializable where appropriate, and unit-testable without a
desktop host, renderer, Serval, Wry, iced, GPUI, concrete storage backend, or
protocol worker.

`GraphWorkspace` may own:

- canonical graph/domain state and reducer-owned arrangement/tree projection
  state;
- reducer queues and pending effects expressed as typed intents/effects, not
  service handles;
- durable user preferences and workbench/frame/session descriptions expressed
  with portable IDs;
- view/runtime state once each field is classified as durable, session-only, or
  transient.

`GraphWorkspace` must not own:

- concrete persistence stores, sync worker channels, client-storage managers,
  engine instances, or host widget handles;
- renderer/WebView identities except through portable shell IDs such as
  `GraphViewId`, `SurfaceHostId`, or later `verso-tile` surface IDs;
- mutation paths that let host callbacks, renderer callbacks, protocol mods, or
  engine adapters bypass typed reducers/deltas.

Target buckets for the move:

| Bucket | Shape | Migration rule |
| --- | --- | --- |
| Domain graph truth | Graph memory, graph/tree arrangement, reducer state, durable graph metadata | Serializable and authoritative; reducers own durable mutation |
| View and workbench session | Navigator projection, graph-view state, frame/workbench layout, selection, cameras | Use portable IDs; split durable session state from transient per-frame caches |
| Chrome and app UX | Command surfaces, settings panes, shortcut/user-style preferences, current overlay state | Durable preferences go through settings persistence; overlay-open flags are runtime state |
| Pending effects | App commands, host requests, storage requests, engine requests | Not persisted; emitted by reducers and consumed by composition glue |
| Service handles | Persistence, import, sync, engine routing, diagnostics, timers/tasks | Stay beside `GraphBrowserApp` or a service container until trait-separated |

The first extracted state surface should preserve the existing donor module cuts
(`intent_system`, `graph_runtime`, `workspace_routing`, `composition`,
`app_ux`). Do not crate-split those seams during the first move; crate-split
only when a real consumer or dependency boundary requires it.

Service and persistence traits should be introduced before moving concrete
implementations:

- `WorkspaceRepository`: load/save durable workspace snapshots, frame/workbench
  snapshots, and reducer checkpoints.
- `GraphMutationJournal`: append/replay typed graph deltas and traversal events
  without giving reducers direct access to a concrete store.
- `MnemStore`: private local memory lane for graph snapshots, traversal logs,
  local browsing memory, settings, caches, and indexes. This is not Mere
  transport state and not Moothold/moot flora.
- `SettingsStore`: typed settings read/write surface for chrome and workspace
  preferences.
- `EngineRouter`: Inker-facing route decision contract. Graphshell asks for an
  engine decision and receives a host-neutral surface contract; it does not own
  Servo/WebRender/GL compatibility plumbing.
- `SurfaceHost`: Graphshell specialization of `verso-tile`'s surface command
  sink. Hosts consume reducer surface commands and emit intents; they do not
  mutate graph truth directly.
- `DiagnosticsSink`, `Clock`, and `TaskRuntime`: small runtime services needed
  for deterministic app-state tests.

`GraphBrowserApp` remains service-composition glue during this phase. Its job is
to wire concrete stores, runtime services, engine routers, and host adapters to
the reducer-owned `GraphWorkspace` contract, then shrink as side effects move
behind traits.

### Phase 4 — Persistence and Mnem Boundary

Separate private local memory/persistence into the Mnem lane:

- graph snapshots
- traversal/archive logs
- settings persistence
- local browsing memory
- cache/index persistence where private-user-scoped

`mnem` may start as a module inside `mere` or `graphshell`, but the plan should keep the concept distinct from moot flora and protocol state.

Acceptance: `graphshell` can construct a workspace from a trait-backed persistence provider; concrete fjall/redb/rkyv storage is not hardwired into UI shell state.

### Phase 5 — Engine and Rendering Re-home

Move old engine routing and viewer decisions by responsibility:

- old `verso` routing policy -> `inker`
- old `middlenet-*` -> `nematic` family
- old tile/surface placement -> `verso-tile`
- host adapters -> `graphshell` host crates

Serval/Netrender integration stays aligned with the Serval cut plan and Netrender compositor roadmap.

Acceptance: Graphshell can ask Inker for an engine decision and receive a host-neutral route/surface contract without importing old Graphshell renderer internals.

### Phase 6 — Host Adapters

Only after portable/app layers compile:

- iced host
- Wry host overlay adapter
- future GPUI/html-css/mobile hosts

Acceptance: host crates depend on `graphshell` and `inker`/`verso-tile`, not on old root crate internals.

---

## 3. Current Position and Next Slices

The original app-state/reducer slice is no longer future work; it has landed in
Mere and is covered by reducer, persistence, `platen`, `inker`, and
`verso-tile` tests. The active migration edge is Phase 5 ownership: keep
shrinking `graphshell::app_state` toward reducer orchestration while moving
surface lifecycle, workbench composition, and engine route vocabulary into the
crates that own those domains.

Primary next slices:

1. Grow `verso-tile` from command vocabulary into surface lifecycle reporting:
    host acknowledgement, deferred-command backlog semantics, and portable
    surface-slot placement packets are in place, so the next edge is host
    allocation/retry scheduling.
2. Keep route policy in `inker`: the default scheme policy is in place, and the
   next route-policy move should add content/runtime signals only when a real
   engine implementation needs them.
3. Continue extracting renderable workbench selectors into `platen`: active
    frame projection, frame arrangement snapshots, and surface-placement
    projection are in place; the next edge is reducer-safe arrangement
    reconciliation if a real donor method needs it.
4. Use the donor inventory as the `GraphBrowserApp` import gate: each old method
   must be classified before migration, and obsolete renderer/service residue
   should be replaced rather than copied.
5. Extend the trait-backed `GraphWorkspace` service container only when new
    pending-effect kinds appear; do not import concrete donor `GraphBrowserApp`
    code until each method has been classified as reducer, service glue, host
    adapter, or obsolete.
6. Run `cargo test --workspace` from `repos/mere` after every narrow move.

Fruitful sidequests:

- Audit the protocol architecture plan against this migration: the
  `verso-tile` / server-self-hosting language still needs a concrete boundary
  with `mere-transport`, `moothold`, and host crates.
- Grow the `GraphBrowserApp` donor inventory only as methods are touched, so it
  stays a migration gate instead of becoming a stale catalog.
- Decompose only oversized portable files touched by these slices; do not let
  file-size cleanup interrupt the ownership migration unless a touched file is
  actively blocking comprehension.

Pitfalls:

- Do not split Navigator into multiple instances; maintain the single surface
  with configurable scope/form factor.
- Do not let host adapters mutate graph truth directly while implementing
  surface command outcomes.
- Do not import concrete desktop/webview/renderer dependencies into
  `graphshell::app_state`, `platen`, `inker`, or `verso-tile` while proving the
  portable contracts.
- Do not treat `verso-tile` as old `verso`; engine routing belongs in `inker`,
  and graph-aware composition belongs in `platen`.

Closest-to-completion plan: this Graphshell migration plan is closest. Its
portable spine, app-state reducers, persistence/Mnem boundary, and first Phase
5 ownership moves are already compiling in Mere. The protocol architecture and
iced jump-ship plans remain broader programs with larger unimplemented
host/runtime surfaces.

---

## 4. Findings

### 2026-05-06 — Migration Review

- Mere already has real foundation code: `mere-identity`, `mere-transport`, `murm`, and `murmuring` are not placeholders.
- `cargo test --workspace` in `repos/mere` passes before migration work begins.
- Old `repos/graphshell` is a donor, not the proof root. Its root workspace is in active decomposition and still contains stale Servo/Serval path assumptions.
- The current app module cuts in old Graphshell are good migration seams and should be preserved.
- The most important architectural refinement is not to carry old `verso` forward as-is. It must be split across `inker`, `platen`, and `verso-tile`.

---

## 5. Progress

### 2026-05-06

- Plan created.
- Workspace license corrected to MPL-2.0.
- `graph-memory` and `graph-tree` migrated into `crates/graph/`.
- `graph-canvas` migrated into `crates/graph/`.
- Removed the donor-side optional Vello/Rapier adapter dependencies from migrated `graph-canvas`; Cargo resolves optional dependencies into the workspace lock even with `--no-default-features`, so renderer and rigid-body adapters need future sibling crates instead of feature flags inside the portable canvas authority.
- `graphshell-core` migrated into `crates/shell/`.
- `register-diagnostics` migrated into `crates/registry/` to unblock runtime without preserving the old `registrar/*` donor path.
- `graphshell-runtime` migrated into `crates/shell/`.
- `crates/graphshell` now re-exports the migrated surfaces as `graphshell::canvas`, `graphshell::core`, `graphshell::memory`, `graphshell::runtime`, and `graphshell::tree`.
- Full `cargo test --workspace` passed after the first portable Graphshell spine landed in Mere.
- Phase 3 preparatory `GraphWorkspace` target shape and service/persistence trait boundaries recorded. Next implementation work should start with the app-state/reducer slice, not host adapters.
- Added `graphshell::app_state` as the first portable `GraphWorkspace` contract in Mere. It defines reducer-owned domain/view/workbench/chrome state, non-persisted pending effects, snapshot envelopes, and trait seams for `WorkspaceRepository`, `GraphMutationJournal`, `MnemStore`, `SettingsStore`, `EngineRouter`, `SurfaceHost`, `DiagnosticsSink`, `Clock`, and `TaskRuntime`.
- `GraphWorkspace` is intentionally live-state-first: it owns `graphshell_core::graph::Graph` and snapshots through the existing `GraphSnapshot` API, while side effects leave through typed effects and traits. `GraphTree<NodeKey>` remains inside view state without requiring extra equality bounds from `graph-tree`.
- Verification: `cargo test -p graphshell` passed with 3 app-state tests; `cargo test --workspace` passed across Mere; `cargo fmt` completed cleanly.
- Added `graphshell::app_state::intent_system` as the first portable reducer slice. It defines `WorkspaceIntent`, `ReducerOutcome`, `ReducerError`, and `reduce_workspace_intent` for graph-view creation/focus, projection lens selection, active-node state, frame upsert/activation, dirty-state tracking, chrome/preference updates, and typed effect emission.
- This intentionally does not port the donor `GraphBrowserApp` intent machinery wholesale. The moved surface is the pure subset that mutates only `GraphWorkspace` or appends `WorkspaceEffect`s; service execution remains outside the reducer.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 8 tests; `cargo test --workspace` passed across Mere.
- Added `graphshell::app_state::graph_runtime` as the first portable graph-runtime reducer slice. It defines `GraphRuntimeIntent`, `GraphRuntimeOutcome`, `GraphRuntimeError`, and `reduce_graph_runtime_intent` for durable node creation/removal plus title, URL, pinned-state, and tag changes.
- `WorkspaceDomainState` now owns a monotonic mutation sequence that is preserved in `GraphWorkspaceSnapshot`. Graph-runtime reducers mutate only `GraphWorkspace.domain.graph` through `graphshell-core` graph APIs and append `WorkspaceEffect::AppendGraphMutation`; concrete journal persistence remains outside reducer state.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 12 tests; `cargo test --workspace` passed across Mere.
- Added `graphshell::app_state::workspace_routing` as the first host-neutral routing reducer slice. It defines `WorkspaceRoutingIntent`, `WorkspaceRoutingOutcome`, `WorkspaceRoutingError`, and `reduce_workspace_routing_intent` for binding graph nodes to portable panes, aligning view/frame pane bindings, updating the `GraphTree` member state, and queuing surface present/retire effects.
- Routing validates graph truth before creating view state and never gives host adapters a graph mutation path. Surface hosts remain opaque `SurfaceHostId`s consumed through `WorkspaceEffect::RequestSurface`; frame and pane state stays reducer-owned while workbench-shaped binding helpers can move outward into `platen`.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 16 tests; `cargo test --workspace` passed across Mere.
- Added `graphshell::app_state::composition` as the first pure composition selector slice. It defines `CanvasSceneOptions`, `CompositionError`, `build_canvas_scene_input`, and `graph_view_id_to_canvas` for deriving `graph-canvas` scene input from reducer-owned `GraphWorkspace` state.
- Composition now covers the host-neutral graph-to-canvas projection path: node/edge scene packets, visible-node masking, scene mode, view-dimension projection, and stable canvas view IDs. It does not move the donor arrangement graph bridge, host composition glue, or renderer backend adapters.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 20 tests; `cargo test --workspace` passed across Mere.
- Added `graphshell::app_state::app_ux` as the first pure app-UX reducer slice. It defines `AppUxState`, `ModalSurface`, `ActionSurfaceState`, `ActionScope`, `ScopeTarget`, `Anchor`, `AppUxIntent`, and `reduce_app_ux_intent` for mutually-exclusive modal/action surface state.
- App-UX state is threaded into `ChromeState` as runtime chrome state. The legacy `command_palette_open` flag remains as a compatibility mirror while new reducer calls drive the richer `ActionSurfaceState`. Renderer-scoped clip capture, diagnostics emission, focus restoration, and clip-node creation remain outside this pure slice.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 25 tests; `cargo test --workspace` passed across Mere.
- Phase 3's first pure reducer/selector pass is now complete enough to move next into persistence-facing types and the Mnem boundary. Do not move concrete stores, `GraphBrowserApp`, or host adapters yet.
- Added `graphshell::app_state::persistence` as the first pure persistence/Mnem boundary slice. It defines `PersistenceIntent`, `PersistenceOutcome`, `MnemRequest`, and `reduce_persistence_intent` for workspace-save requests, saved-state acknowledgement, preference-save requests, and private local blob load/save requests.
- `WorkspaceEffect` now has typed `PersistPreferences` and `RequestMnem` variants alongside existing workspace snapshot and mutation journal effects. Concrete `PersistenceFacade`, fjall/redb/rkyv stores, autosave timers, and GraphBrowserApp wiring remain outside reducer-owned state.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 29 tests; `cargo test --workspace` passed across Mere; editor diagnostics are clean on the touched app-state files.
- Added trait-backed persistence helper functions in `graphshell::app_state::persistence`: `hydrate_workspace`, `checkpoint_workspace`, `save_preferences`, and `dispatch_mnem_request`. These prove that `GraphWorkspace` can be loaded from and saved through `WorkspaceRepository`, `SettingsStore`, and `MnemStore` without importing the donor `GraphStore`, startup timeout thread, diagnostics emitters, or `GraphBrowserApp` facade methods.
- Hydration overlays durable settings after restoring an optional workspace snapshot; checkpointing snapshots reducer-owned state and clears the dirty flag only after the repository write succeeds; Mnem dispatch returns typed blob responses while preserving trait error propagation.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 35 tests; `cargo test --workspace` passed across Mere; editor diagnostics are clean on the touched persistence file.
- Added pure persistence coordination helpers in `graphshell::app_state::persistence`: `consume_persistence_effects`, `replay_graph_journal`, and `hydrate_workspace_with_replay`. Persistence effects for workspace snapshots, preference saves, journal appends, and Mnem requests can now be drained and executed through trait-backed services while unrelated effects remain queued for other composition glue.
- Startup hydration now has a reducer-safe replay lane: after loading an optional snapshot and settings, Graphshell can replay `GraphMutationJournal` records encoded as typed `GraphRuntimeIntent` JSON on top of the restored workspace without re-enqueuing journal writes. Opaque journal payloads are rejected explicitly instead of being replayed ambiguously.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 38 tests; `cargo test --workspace` passed across Mere; editor diagnostics are clean on the touched persistence and plan files.
- Expanded the durable persistence contract for workbench identity and named layout/tree documents. `WorkbenchState` now carries reducer-owned workbench persistence state, and `WorkspaceRepository` now defines stable workbench view ID, graph-tree document, and named workspace-layout document operations instead of leaving those donor persistence-facade concepts implicit.
- Added portable persistence vocabulary and helpers in `graphshell::app_state::persistence`: `WorkbenchPersistenceState`, `WorkspaceLayoutName`, `GraphTreeDocument`, `WorkspaceLayoutDocument`, `ensure_workbench_view_id`, `load_workbench_graph_tree`, `save_workbench_graph_tree`, `load_named_workspace_layout`, `save_named_workspace_layout`, and `list_named_workspace_layouts`. This keeps view/layout persistence typed and host-neutral without importing the donor `GraphStore` or raw facade methods.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 43 tests; `cargo test --workspace` passed across Mere; editor diagnostics are clean on the touched app-state and plan files.
- Gave Mnem a real Mere-side home by extracting `MnemRequest`, `MnemResponse`, `MnemStore`, and `dispatch_mnem_request` into the dedicated `graphshell::mnem` module. `app_state` and `app_state::persistence` now depend on that module instead of owning the Mnem vocabulary themselves.
- This keeps private local-memory concerns explicit and separable from the broader persistence coordinator: app-state still emits `WorkspaceEffect::RequestMnem`, but the request/response types and store trait now live outside the reducer slice in a module that can later graduate into a narrower crate boundary if needed.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 43 tests; `cargo test --workspace` passed across Mere; editor diagnostics are clean on the touched Mnem, app-state, persistence, and plan files.
- Moved the portable engine-routing vocabulary out of `graphshell::app_state` and into the new `inker::routing` module. `EngineRouteRequest`, `EngineRouteDecision`, `SurfaceContract`, `SurfaceContractMode`, `WorkspaceRouteId`, and `SurfaceTargetId` now live under `inker`, and `graphshell::app_state` only re-exports the types to preserve the existing reducer-facing seam.
- This is the smallest real Phase 5 ownership handoff: Graphshell still owns the `EngineRouter` trait boundary and route effects, but it no longer owns the route request/decision contract types themselves. The new `SurfaceTargetId` is an intentionally temporary bridge until `verso-tile` owns concrete surface identity.
- Verification: `cargo fmt` completed; `cargo test -p graphshell` passed with 43 tests; `cargo test --workspace` passed across Mere.
- Moved `SurfaceTargetId` out of `inker::routing` and into the new `verso_tile::surface` module. `inker::routing` now re-exports the type from `verso-tile`, so the route contract points at a surface-layer identity without making Graphshell depend on host or tile internals.
- Started the `platen` extraction with a real `platen::canvas_scene` module. The graph-to-canvas derivation core now lives in `platen`, with direct crate tests, while `graphshell::app_state::composition` has been reduced to a thin workspace/view wrapper that preserves the existing reducer-facing API and missing-view guard.
- Verification: `cargo fmt` completed; `cargo test -p graphshell -p platen -p inker -p verso-tile` passed with 43 Graphshell tests and 2 Platen tests; `cargo test --workspace` passed across Mere.
- Moved the remaining surface-facing Graphshell seam into `verso_tile::surface`. `SurfaceHostId`, `SurfaceEffect`, and `SurfaceRequest` now live beside `SurfaceTargetId`; Graphshell now depends on those surface-layer contracts instead of owning them locally.
- Extracted the next workbench/frame slice into `platen::workbench`. `FrameId`, `PaneBinding`, `FrameState`, and the first active-frame selector now live in `platen`, while `graphshell::app_state::composition` provides only a thin wrapper over the selector for reducer-owned workspace state.
- Verification: `cargo fmt` completed; `cargo test -p graphshell -p platen -p verso-tile` passed with 44 Graphshell tests and 4 Platen tests; `cargo test --workspace` passed across Mere.
- Moved surface-effect assembly itself into `verso_tile::surface` by adding `SurfaceEffect::present`, `retire`, and `focus` constructors. `graphshell::app_state::workspace_routing` now emits surface effects through those helpers instead of constructing the request shape inline.
- Extracted the next workbench selector into `platen::workbench` with `select_active_root_view`, and `graphshell::app_state::composition` now wraps both the active-frame and active-root-view selectors for reducer-owned workspace state.
- Verification: `cargo fmt` completed; `cargo test -p graphshell -p platen -p verso-tile` passed with 45 Graphshell tests, 5 Platen tests, and 3 Verso-Tile tests; `cargo test --workspace` passed across Mere.
- Moved the next reducer-adjacent workbench helpers into `platen::workbench`. Pane-binding upsert/removal, frame root-view assignment, frame-pane assignment, and binding surface-host updates now live in `platen`, and `graphshell::app_state::workspace_routing` has been reduced further toward orchestration and error mapping.
- Introduced a narrower host-facing `verso_tile::surface::SurfaceCommand` seam. `graphshell::app_state::WorkspaceEffect::RequestSurface` and the `SurfaceHost` trait now speak `SurfaceCommand`, while `SurfaceEffect` remains the raw envelope available inside `verso-tile` through `SurfaceCommand::to_effect()`.
- Verification: `cargo fmt` completed; `cargo test -p graphshell -p platen -p verso-tile` passed with 45 Graphshell tests, 7 Platen tests, and 5 Verso-Tile tests; `cargo test --workspace` passed across Mere.
- Moved the next view/frame synchronization recipes into `platen::workbench` with `assign_view_and_frame_pane` and `remove_view_and_frame_pane`. `graphshell::app_state::workspace_routing` now keeps GraphTree/projection mutation locally but delegates the remaining pane-binding synchronization across view and frame state to `platen`.
- Tightened the Graphshell-facing surface seam one step further by removing raw `SurfaceEffect` from `graphshell::app_state` re-exports. Graphshell reducers and hosts now speak `SurfaceCommand`, `SurfaceHostId`, and `SurfaceRequest`, while the raw effect envelope stays internal to `verso-tile`.
- Verification: `cargo fmt` completed; `cargo test -p graphshell -p platen -p verso-tile` passed with 45 Graphshell tests, 9 Platen tests, and 5 Verso-Tile tests; `cargo test --workspace` passed across Mere.
- Moved the final pane surface-host synchronization helper out of `graphshell::app_state::workspace_routing` and into `platen::workbench` as `set_view_and_frame_surface_host`. Workspace routing now delegates all view/frame pane-binding synchronization to `platen`, keeping only graph truth validation, GraphTree/projection mutation, and surface command emission locally.
- Added `verso_tile::surface::SurfaceCommandSink` as the generic host-side command application seam. `graphshell::app_state::SurfaceHost` is now a Graphshell error-specialized marker over that `verso-tile` trait instead of owning the command application method itself.
- Verification: `cargo fmt` completed; `cargo test -p graphshell -p platen -p verso-tile` passed with 45 Graphshell tests, 10 Platen tests, and 6 Verso-Tile tests; `cargo test --workspace` passed across Mere.
- Moved pane-scoped surface-command construction into `verso_tile::surface` with `SurfaceCommand::present_pane`, `retire_pane`, and `focus_pane`. `graphshell::app_state::workspace_routing` now emits prebuilt pane commands from `verso-tile` instead of wrapping `GraphViewId` and `PaneId` into optional command fields locally.
- Verification: `cargo fmt` completed; `cargo test -p graphshell -p platen -p verso-tile` passed with 45 Graphshell tests, 10 Platen tests, and 7 Verso-Tile tests; `cargo test --workspace` passed across Mere.
- Added `verso_tile::surface::SurfaceCommandOutcome` and `SurfaceCommandStatus` so hosts can report applied / already-satisfied / deferred command results with the same surface identity vocabulary as the command. `SurfaceCommandSink::apply_surface_command` now returns the outcome instead of `()`, and Graphshell re-exports the outcome/status without owning the lifecycle vocabulary.
- Refactored this plan's stale near-term section into a current position map covering where the migration stands, where it is going, fruitful sidequests, pitfalls, and closest-to-completion status. The Graphshell migration plan is currently the closest to completion among the active attached plans because its portable spine, reducers, persistence/Mnem boundary, and first Phase 5 ownership moves all compile in Mere.
- Verification: `cargo test -p graphshell -p platen -p verso-tile` passed with 45 Graphshell tests, 10 Platen tests, and 8 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Added `graphshell::app_state::services` as the first trait-backed `GraphWorkspace` service container. It dispatches every current `WorkspaceEffect` variant through `WorkspaceRepository`, `SettingsStore`, `GraphMutationJournal`, `MnemStore`, `EngineRouter`, `SurfaceHost`, `DiagnosticsSink`, and `TaskRuntime`, returning a report with route decisions, surface outcomes, Mnem responses, and side-effect counts.
- This completes the plan's immediate service-container proof before donor `GraphBrowserApp` import: reducers can emit a full pending-effect queue, and composition/service glue can execute that queue without concrete stores, host widgets, renderer handles, or engine implementations entering `app_state`.
- Verification: `cargo test -p graphshell app_state::services` passed; `cargo test -p graphshell -p platen -p verso-tile` passed with 46 Graphshell tests, 10 Platen tests, and 8 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Added `inker::routing::EngineRoutePolicy` and `EngineRouteRule` as the first concrete route-policy home outside Graphshell. The default policy sends full web schemes to `serval.web`, smolweb schemes to `nematic.smolweb`, local files to `nematic.file`, internal Mere/Graphshell routes to a headless internal engine, and unknown protocols to a headless external-protocol handoff instead of guessing a webview.
- Added `platen::workbench::WorkbenchProjection` and `ProjectedPane` plus `project_frame` / `project_active_workbench` so active frame/pane projection is plain data owned by Platen. `graphshell::app_state::composition` now wraps that projection instead of owning the packet shape.
- Added `2026-05-06_graphbrowserapp_donor_inventory.md` as the import gate for old `GraphBrowserApp` methods. The seed inventory classifies runtime lifecycle, arrangement bridge, persistence facade, sync/storage handles, clip capture, and history areas as reducer, service glue, host adapter, Platen projection, Inker policy, Verso-Tile lifecycle, or obsolete residue before any method body can be copied into Mere.
- Verification: `cargo test -p inker -p platen -p graphshell` passed with 5 Inker tests, 12 Platen tests, and 47 Graphshell tests; `cargo fmt` completed; `cargo test -p inker -p platen -p graphshell -p verso-tile` passed with 5 Inker tests, 12 Platen tests, 47 Graphshell tests, and 8 Verso-Tile tests; `cargo test --workspace` passed across Mere.
- Added `verso_tile::surface::SurfaceCommandBacklog` plus outcome helpers so deferred host command results can be retained for retry as surface-layer lifecycle data instead of being lost in Graphshell service glue. The backlog records only matching `Deferred` outcomes and exposes simple inspect/take/drain operations for future host schedulers.
- Added `platen::workbench::ArrangementSnapshot`, `ArrangementContainer`, `ArrangementMember`, and `TileSlot`, with active-frame snapshot helpers. This preserves the donor arrangement bridge's useful plain-data boundary while moving the snapshot vocabulary to Platen and leaving graph mutation/reconciliation out of the projection layer.
- `graphshell::app_state::composition` now wraps Platen's active arrangement snapshot just like it wraps active workbench projection. Graphshell remains the reducer-owned workspace selector, not the owner of the arrangement packet shape.
- Verification: `cargo test -p graphshell -p platen -p verso-tile` passed with 48 Graphshell tests, 13 Platen tests, and 10 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Moved `TileSlot` into `verso_tile::surface` and added `SurfaceSlotPlacement` / `SurfacePlacementPlan` so tile-slot placement has a portable surface-layer packet instead of living as a Platen-local index. Placement plans can expose hosted placements and derive present commands without owning host widgets.
- Added `platen::workbench::project_surface_placements` and `project_active_surface_placements` so Platen maps frame arrangement snapshots into Verso-Tile placement plans while filtering unhosted members. `graphshell::app_state::composition` remains a thin wrapper over that projection for reducer-owned workspace state.
- Updated the root workspace members to follow the relocated `moothold` and `mooting` crates under `crates/moot/`; this preserves the user's crate move and restores Cargo workspace loading.
- Verification: `cargo test -p graphshell -p platen -p verso-tile` passed with 49 Graphshell tests, 14 Platen tests, and 12 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
