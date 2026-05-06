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
| `graphshell::app_state` | Reducer-owned `GraphWorkspace`, pure reducers, transient/durable UI state, typed effects, temporary service traits | Concrete services, renderer handles, task runtimes, host widgets, engine lifecycle, storage implementations | Keep Phase 3 code here only while it is pure state/reducer/selector logic |
| `graphshell` host crates | Native/desktop/mobile host adapters, event translation, surface effect application, accessibility bridge | Graph truth mutation, engine selection policy, private memory persistence | Hosts emit intents/effects and consume projections; they do not bypass reducers |
| `inker` | Engine choice, URI/content routing policy, engine lifecycle, engine-output contracts | Graphshell chrome, GraphWorkspace mutation, tile placement | Current `EngineRouter` types are a Graphshell-side seam; move or mirror real policy types into `inker` once implementation begins |
| `platen` | Graph-aware composition/layout policy, frame/tile arrangement projection, layout constraints, renderable workbench model | Engine lifecycle, host widget handles, durable graph mutation | The donor arrangement graph bridge and richer composition logic should graduate here, not stay in `app_state::composition` |
| `verso-tile` | Rendering surface identity, tile-slot placement, surface receiving/lifecycle between inker/platen/host | Engine selection, GraphWorkspace graph mutation, product chrome | Replace opaque `SurfaceHostId` with narrower verso-tile surface contracts when the surface layer becomes real |
| `mnem` | Private local memory: graph snapshots, traversal logs, settings/cache/index persistence | Mere transport state, Moothold community flora, host UI state | `MnemStore` is temporary vocabulary until a concrete `mnem` module/crate owns the contract |
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
- `SurfaceHost`: host/adapter surface request sink. Hosts consume reducer
  effects and emit intents; they do not mutate graph truth directly.
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

## 3. Near-Term Next Slice

The portable spine slice is complete enough to make the next slice app-state
and reducer work. Move code in this order:

1. Introduce the `GraphWorkspace` target state and trait vocabulary in Mere,
   using placeholder/stub implementations where needed.
2. Move the pure reducer seams in their current module-cut shape:
   `intent_system`, `graph_runtime`, `workspace_routing`, `composition`, and
   `app_ux`.
3. Move only the persistence-facing types needed by those seams. Concrete
   stores and old `PersistenceFacade` methods stay behind traits until the Mnem
   lane is explicit.
4. Add app-state tests that exercise reducers and persistence trait fakes
   without launching a host.
5. Run `cargo test --workspace` from `repos/mere` after each narrow move.

Do **not** move `GraphBrowserApp`, host adapters, old `verso`, `middlenet-*`, or
renderer adapters in this slice.

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
- Routing validates graph truth before creating view state and never gives host adapters a graph mutation path. Surface hosts remain opaque `SurfaceHostId`s consumed through `WorkspaceEffect::RequestSurface`; frame and pane state stays reducer-owned.
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
