# Graphshell Migration Plan

**Date**: 2026-05-06
**Status**: Active / canonical migration direction
**Scope**: Move the current `graphshell` codebase into the `mere` workspace and associated crates without preserving obsolete product naming, renderer ownership, or host/runtime coupling. This plan is the bridge between the existing `graphshell` decomposition work and the new Mere crate taxonomy.

**Related**:

- [`../../../README.md`](../../../README.md) — current Mere workspace crate roles
- [`../../2026-05-04_lexicon_brief.md`](../../2026-05-04_lexicon_brief.md) — current product/component vocabulary
- [`../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`](../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md) — protocol/identity/transport architecture that Graphshell must consume, not duplicate
- Inherited: [`../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/2026-05-01_workspace_architecture_proposal.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/2026-05-01_workspace_architecture_proposal.md) — current root-crate decomposition plan and registrar/system-layer receipts
- Inherited: [`../../../../genet/docs/2026-05-05_genet_netrender_cut_plan.md`](../../../../genet/docs/2026-05-05_genet_netrender_cut_plan.md) — Genet/Netrender imposed renderer shape
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

Reason: the Mere workspace already builds and tests cleanly, while the old Graphshell workspace is mid-refactor and still carries stale path assumptions around the Servo/Genet transition.

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
- Genet/Wry fallback only for browser-managed content.

Do not turn Nematic into "everything becomes HTML."

### 1.6 Netrender/Genet Replaces the Old Render Stack

Do not port Graphshell's old GL/WebRender/surfman render assumptions. The renderer path is:

```text
inker -> Genet or Nematic or Wry -> platen -> verso-tile -> graphshell host
```

For full web content, Genet's target is Netrender-driven rendering with native-compositor handoff. Graphshell should consume that surface; it should not preserve obsolete WebRender ownership or compatibility plumbing.

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
| `graphshell::app_state` | Reducer-owned `GraphWorkspace`, pure reducers, transient/durable UI state, typed effects, temporary service traits | Concrete services, renderer handles, task runtimes, host widgets, engine lifecycle, storage implementations | Keep Phase 3 code here only while it is pure state/reducer/selector logic; `app_state::services` now proves the full pending-effect queue and surface lifecycle schedules can dispatch through traits without importing donor `GraphBrowserApp` |
| `graphshell` host crates | Native/desktop/mobile host adapters, event translation, surface command application, accessibility bridge | Graph truth mutation, engine selection policy, private memory persistence | Hosts emit intents/effects and consume projections; they do not bypass reducers. `graphshell-host` owns the registry-backed viewer-surface host adapter and fresh-host toolkit order; `graphshell-host-iced` starts the first new host boundary without importing old GraphBrowserApp mutation assumptions |
| `inker` | Engine choice, URI/content routing policy, engine lifecycle, engine-output contracts | Graphshell chrome, GraphWorkspace mutation, tile placement | `inker::routing` now owns the portable engine-route request/decision vocabulary and a default scheme-based `EngineRoutePolicy`; concrete engine implementations still remain outside the policy |
| `platen` | Graph-aware composition/layout policy, frame/tile arrangement projection, layout constraints, renderable workbench model | Engine lifecycle, host widget handles, durable graph mutation | `platen::canvas_scene` and `platen::workbench` now own the extracted graph-to-canvas derivation core, frame/pane model, active-frame/root-view selectors, pane/frame binding helpers, active-workbench projection packets, frame arrangement snapshots, and the projection from arrangements into hosted surface placements; Graphshell should keep shrinking toward wrappers around those selectors/helpers |
| `verso-tile` | Rendering surface identity, tile-slot placement, surface receiving/lifecycle between inker/platen/host | Engine selection, GraphWorkspace graph mutation, product chrome | `SurfaceTargetId`, `SurfaceHostId`, `SurfaceEffect`, `SurfaceRequest`, `SurfaceCommand`, `SurfaceCommandOutcome`, `SurfaceCommandBacklog`, `TileSlot`, node-carrying `SurfaceSlotPlacement`, `SurfacePlacementPlan`, `SurfaceCommandSchedule`, `SurfaceLifecycleState`, and the generic `SurfaceCommandSink` host seam now live here; grow richer surface lifecycle/types here instead of pushing them back into `inker` or Graphshell |
| `graphshell-runtime` | Portable host/runtime adapters, schedule application helpers, frame projections, runtime ports | Reducer-owned graph truth, concrete desktop widgets, engine routing policy | `surface_schedule` now applies Verso-Tile present schedules to the existing `ViewerSurfaceHost` seam by placement `NodeKey`, giving future fresh host crates a portable adapter point instead of making app state allocate viewer surfaces |
| `eidetic` | Private local memory: graph snapshots, traversal logs, settings/cache/index persistence | Mere transport state, Moothold community flora, host UI state | Extracted as a top-level crate 2026-05-06. Defines `eidetic::Request`/`Response`/`Store`/`dispatch` plus an owned `Error` type that downstream crates `From`-convert into their own service errors. Storage backends (fjall, redb, IndexedDB, in-memory) implement `Store` outside this crate. |
| `mere` | Product entrypoint, top-level service wiring, crate orchestration, feature assembly | Low-level graph primitives, renderer internals, protocol implementations | `mere` composes the system; it should not become the old root-crate catch-all |

Naming warning: the donor `composition` module is broader than the long-term
Mere owner. The initial `graphshell::app_state::composition` module should stay
limited to pure selectors such as workspace-to-canvas scene input. Anything
that decides arrangement, split topology, surface placement, or renderable
workbench layout should move toward `platen` / `verso-tile` instead.

### 1.10 The Verb Test

The donor Graphshell was a prototype. Its code is evidence about behavior and
failure modes, not a crate map to photocopy. The crate graph is efficient when
each crate owns a *product verb* — a question the application has to answer.
Asking "which crate owns this?" should resolve to a single predictable answer.

Mere's product verbs (so far): browse, route content to engine, compose graph
layout, place renderable surfaces, host through a concrete GUI, remember
locally, identify, transport, converse bilaterally, federate.

A crate is healthy when:

- It owns one product verb, or one tightly-coupled cluster of verbs. The
  printing-press stack (`inker` / `platen` / `verso-tile`) is a coupled cluster,
  not three independent verbs; that is fine.
- The verb is product-language, not prototype-language: "remember locally"
  rather than "blob store"; "host through iced" rather than "iced adapter."
- A reader who knows the verbs can predict the crate's contents without
  opening it.

A crate is suspicious when:

- Its name is a noun the prototype happened to use (`core`, `app`, `shell`,
  `host`) without a verb attached.
- Multiple crates plausibly own the same verb — the verb is split.
- It is "where things go that don't fit elsewhere."
- Its public surface re-exports another crate's public surface as a convenience.
  That bakes the other crate into this crate's vocabulary and quietly hardens
  into architecture.

Cost reduction (dependency, validation, change) is *downstream* of verb
clarity. A crate that owns a clear verb almost always has a small dependency
surface and a testable contract; a crate that doesn't, almost never does. Lead
with the verb; the costs follow.

Do not create a crate just because the prototype had a module. Do not merge a
crate just to reduce file count if that would force portable code to depend on
a host toolkit, engine backend, task runtime, renderer ID type, or persistence
implementation.

### 1.11 Verb-Test Audit (2026-05-06)

Applying §1.10 to the current Mere crate graph surfaces seven friction points.
None blocks the migration; they shape the next slice planning.

**1. `graphshell-core` is doing three jobs.** ~~Its 30 source files split roughly
half portable vocabulary…~~ **Partially resolved 2026-05-06.** Two-way split
landed instead of the three-way candidate from the original audit:

- New crate [`graphshell-shell-state`](../../../crates/graphshell/shell-state/)
  owns the (a) shell session state: `authorities`, `command_palette`,
  `command_surface_telemetry`, `frame_model`, `host_intent`, `omnibar`,
  `toolbar`, plus `routing` and the three `ux_*` modules.
- `graphshell-core` keeps the (b) portable vocabulary AND the (c) port-trait
  definitions (`viewer_host`, `host_event`, `async_host`, `signal_router`,
  `async_request`).

Why two-way instead of three-way: the audit's plan to fold the (c) port traits
into `graphshell-runtime` runs into a dep-direction wrinkle. `verso-tile`'s
apply algorithm (moved from runtime in friction point #2) uses
`ViewerSurfaceHost` as a trait bound. If that trait moved into
`graphshell-runtime`, `verso-tile` would have to depend on `graphshell-runtime`
— an unwanted back-edge in the press-stack peering. Keeping trait *definitions*
in `graphshell-core` (treating them as cross-consumer vocabulary, not host
machinery) preserves the dep direction. Concrete host *implementations* of
those traits still live in host-side crates (`graphshell-host`,
`graphshell-runtime::ports`).

`SurfaceId` was lifted from `ux_observability` (now in shell-state) into
`graphshell-core::accessibility` to keep the `accessibility` module's
descriptor table self-contained. `ux_observability` re-exports it for
back-compat.

Remaining open question for a future slice: whether `graphshell-core` still
benefits from a vocab/ports rename (it now hosts ~24 modules cleanly across
two job clusters — primitives and trait definitions — which is more
defensible than the prior three).

**2. `graphshell-runtime` and `verso-tile` both touch surface lifecycle.**
~~`runtime/surface_schedule.rs` and `runtime/webview_backpressure.rs`…~~
**Resolved 2026-05-06.** `apply_viewer_surface_schedule` (and its
`SurfaceScheduleApplyReport` / `SurfaceScheduleApplyError` types + tests)
moved from `graphshell-runtime/src/surface_schedule.rs` into
[`verso-tile/src/apply.rs`](../../../crates/verso-tile/src/apply.rs). Verso-tile
now owns the schedule shape AND the apply algorithm. `graphshell-runtime`
dropped the `verso-tile` dev-dep on `graphshell-host` since the test path no
longer rides through runtime.

`webview_backpressure.rs` did *not* move — on closer inspection it's
host-runtime retry/cooldown state for webview *creation*, not a verso-tile
schedule application. It uses `ViewerSurfaceId` from `graphshell-runtime::ports`
and stays where its host-runtime peers live. The audit conflated the two
files; this slice corrects that distinction.

**3. `graphshell-host` is a shared catalogue, not a host.** ~~Its current
contents are toolkit-vocab + a registry-shaped trait adapter…~~ **Resolved
2026-05-06.** Moved `HostToolkit`, `NEW_HOST_TOOLKIT_ORDER`, and
`preferred_host_toolkits` from `graphshell-host` into
[`crates/graphshell/core/src/host_toolkit.rs`](../../../crates/graphshell/core/src/host_toolkit.rs)
— the toolkit catalogue is cross-consumer vocabulary, not host-side machinery.
`graphshell-host` now only contains the genuinely host-side
`viewer_surface_host` adapter (`SurfaceFactoryHost`,
`ViewerSurfaceRegistryHost`); the crate's name and contents now match.
`graphshell-host-iced` dropped its `graphshell-host` dep and imports
`HostToolkit` from `graphshell-core::host_toolkit` directly.

**4. `graphshell` (the meta crate) co-locates two verbs.** It both *composes
the portable Graphshell crates for downstream consumption* (the `lib.rs`
re-exports) and *owns the application reducer* (`app_state/`). Fine while Mere
is the only consumer; needs to split when a second consumer wants the shell
vocabulary without the reducer. Watch for the trigger; do not split
preemptively.

**5. "Remember locally" is a first-class verb hiding as an internal module.**
~~It sits alongside identify / transport / converse / federate as a top-level
lane.~~ **Resolved 2026-05-06.** Extracted to a top-level crate at
[`crates/eidetic/`](../../../crates/eidetic/). The bundled rename + extraction
slice replaced the in-tree `graphshell::mnem` module with the new crate; types
went from `MnemRequest` / `MnemResponse` / `MnemStore` / `dispatch_mnem_request`
to `eidetic::Request` / `eidetic::Response` / `eidetic::Store` /
`eidetic::dispatch`, and the `WorkspaceEffect::RequestMnem` /
`PersistenceIntent::RequestMnemBlobLoad` etc. variants and the `mnem_responses`
report field were renamed to their `Eidetic` / `eidetic_responses` equivalents.
A `From<eidetic::Error> for WorkspaceServiceError` impl bridges the upstream
error type into graphshell's existing service-error vocabulary.

Naming decision history: the prototype name `mnem` is unavailable on
crates.io. `eidetic` was chosen (evokes eidetic memory; available on
crates.io and GitHub). Runners-up: `idyl` (resting/idle storage), `eido`
(shorter root). `eidetic` won on recognisability.

Publication: extraction landed and the workspace tests pass; `cargo publish`
is pending the workspace repository URL being set and Mark's crates.io login.

**6. The "route content to engine" verb is split.** ~~`inker::routing` defines
the route-decision vocabulary…~~ **Resolved 2026-05-06.** Dropped
`pub use inker::routing::{EngineRouteDecision, EngineRouteRequest, SurfaceContract,
SurfaceContractMode, SurfaceTargetId, WorkspaceRouteId}` from
[`crates/graphshell/src/app_state.rs`](../../../crates/graphshell/src/app_state.rs);
the file now does a private `use inker::routing::{EngineRouteDecision,
EngineRouteRequest}` for its own internal references. Submodules
`app_state/services.rs` and `app_state/intent_system.rs` updated to import
from `inker::routing` directly rather than reaching through `super::`.

**7. `graphshell::canvas` re-exports `graph_canvas`.** ~~Same pattern as #6…~~
**Resolved 2026-05-06.** Dropped `pub use graph_canvas as canvas;` from
[`crates/graphshell/src/lib.rs`](../../../crates/graphshell/src/lib.rs). No
external code consumed it. `graphshell` still depends on `graph_canvas`
internally (via `app_state/composition.rs`), so the dep stays — only the
public re-export is gone.

**Status 2026-05-06**: friction points #6, #7, #2, #1, #3, and #5 all landed
in one session. Friction point #1 landed as a two-way split (shell-state
extraction) instead of the original three-way candidate, after the
dep-direction analysis showed that lifting port traits into runtime would
break the verso-tile peering. Friction point #4 deferred per its original
guidance — needs a second consumer of the shell vocabulary before splitting
the meta-crate. The audit is now historical except for #4's gated trigger.

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

Acceptance: `crates/graphshell` is still light enough to compile as a library target without Genet, Wry, iced, GPUI, or platform WebView dependencies.

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
desktop host, renderer, Genet, Wry, iced, GPUI, concrete storage backend, or
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

### Phase 4 — Persistence and Eidetic Boundary

Private local memory/persistence flows through the [`eidetic`](../../../crates/eidetic/) crate:

- graph snapshots
- traversal/archive logs
- settings persistence
- local browsing memory
- cache/index persistence where private-user-scoped

The crate (extracted 2026-05-06) owns the typed `Request`/`Response`/`Store` vocabulary and the `dispatch` helper. The concept stays distinct from moot flora and protocol state.

Acceptance: `graphshell` can construct a workspace from a trait-backed persistence provider; concrete fjall/redb/rkyv storage is not hardwired into UI shell state.

### Phase 5 — Engine and Rendering Re-home

Move old engine routing and viewer decisions by responsibility:

- old `verso` routing policy -> `inker`
- old `middlenet-*` -> `nematic` family
- old tile/surface placement -> `verso-tile`
- host adapters -> `graphshell` host crates

Genet/Netrender integration stays aligned with the Genet cut plan and Netrender compositor roadmap.

Acceptance: Graphshell can ask Inker for an engine decision and receive a host-neutral route/surface contract without importing old Graphshell renderer internals.

### Phase 6 — Host Adapters

Only after portable/app layers compile, build fresh host crates in this order:

1. iced
2. GPUI
3. HTML/CSS
4. Xilem
5. Makepad
6. egui

Donor host modules are reference material only. Take lessons, not photocopies.
Do not pull in old mixed mutation assumptions, direct `GraphBrowserApp` state
edits, no-op lifecycle adapters, renderer ID maps in app state, stale prototype
UI contracts, or toolkit-specific shortcuts that bypass reducer intents/effects.

Acceptance: host crates depend on the narrow Graphshell crates they need
(`graphshell-core`, `graphshell-runtime`, `graphshell-host`) plus
`inker`/`verso-tile` only when they consume route or surface contracts; they do
not depend on old root crate internals or drag toolkit dependencies into
portable crates.

---

## 3. Current Position and Remaining Work

Phases 0-5 have substantively landed: the portable spine, reducers,
persistence/Eidetic boundary, `inker` route vocabulary, `platen` workbench
projection, and `verso-tile` surface lifecycle all compile in Mere with full
test coverage. `graphshell::app_state` has shrunk to reducer orchestration plus
the trait-backed service container; further outward moves of selectors,
helpers, and lifecycle bookkeeping should happen opportunistically when a
touched file gives them a free home, not as standalone slices.

What remains is **feature work**, not structural migration. Mere is the only
consumer of these crates, so framing items as "wait until a consumer needs it"
or "extend only when a new effect kind appears" gates nothing real — the gate
just freezes the codebase in its current shape. The work below is what the
browser needs to actually run; ship it because the product needs it, not
because a hypothetical second consumer pulls on it.

### 3.1 Critical Path — "Browser Actually Runs"

These three are load-bearing for any end-to-end demo. Without one of each,
Mere has no UI surface, no rendering, and no content.

1. **At least one real host adapter.** Host order pivoted 2026-05-08 to gpui first (Glass-HQ/gpui fork), with iced and HTML/CSS retained as fallbacks; vello/netrender/genet embed via a `PlatformSurface` trait using OS composition rather than wgpu texture sharing. [`graphshell-host-iced`](../../../crates/graphshell/host-iced/) remains as a port boundary; the gpui host is on the come up in parallel. Whichever lands first needs to present a real `SurfacePlacementPlan` from `verso-tile` in a window and route input back as intents.
2. **Engine layer in `inker`.** Landed 2026-05-08 — `inker::engine` defines the [`Engine`] trait, the [`EngineDocument`] / [`DocumentBlock`] / [`InlineSpan`] document model (with AccessKit role mapping for projection), [`EngineInput`] (already-fetched content; I/O is the host's job), and [`EngineRegistry`] for engine-ID dispatch. Default routing policy now points `gemini` / `spartan` schemes at the concrete `nematic.gemtext` engine; `gopher` / `finger` continue to use the `nematic.smolweb` umbrella ID until concrete engines exist.
3. **Protocol lanes in `nematic`.** Three concrete engines shipped 2026-05-08: [`MarkdownEngine`] (CommonMark via `pulldown-cmark`), [`GemtextEngine`] (Gemini's `text/gemini` line-oriented format), [`TextEngine`] (plain text with paragraph splitting). `nematic::engines()` returns a `Vec<Box<dyn Engine>>` of all three for one-call registration. Gopher menu, file viewer, and feed (RSS/Atom) lanes pending. End-to-end test in `nematic::tests` confirms: `EngineRoutePolicy::default()` routes `gemini://` → `nematic.gemtext` decision → `EngineRegistry::dispatch` → renders title from `# H1`. The path from address to document works.

### 3.2 Feature Areas — Donor Rebuilds

Areas classified in [`2026-05-06_graphbrowserapp_donor_inventory.md`](2026-05-06_graphbrowserapp_donor_inventory.md) need to be **rebuilt**, not just classified. The inventory's "classify before importing" rule is a quality gate on *how* a feature lands, not a time gate on *whether* to start. Order roughly by user-visible impact:

- **Runtime/webview lifecycle wiring.** Route `host_open_request`, surface present/retire, URL/title/history/scroll/crash events through the reducer rather than direct mutation. Required for any real navigation UX. Most planning shape exists in `app_state::workspace_routing` + `verso-tile::surface`; needs concrete iced wiring once §3.1 #1 lands.
- **History / undo / archive.** Mutation journal contract already exists; needs reducer-side undo state, navigation preview cursor, and Eidetic-backed archive queries.
- **Clip capture + clip nodes.** Modal/action UX state belongs in `app_state::app_ux`; capture payload type needs a portable shape; node creation goes through graph-runtime intents; storage through Eidetic.
- **Persistence health, snapshot timers, autosave.** Service-glue concerns; live next to `WorkspaceServices` rather than inside the reducer.
- **Sync / storage interop.** Rebuild around `mere-transport`, `murm`, `moothold`. The donor's `set_sync_command_tx` / `set_client_storage_manager` shape is largely obsolete residue; design fresh against the protocol crates rather than copying.

### 3.3 Phase 1 Loose Ends

Two Phase 1 items never moved and need a decision, not a defer:

- **`graphshell-comms`**: the donor crate's role overlaps `murm` (bilateral) and `moothold` (federation). Decide whether it survives as a UI-side wrapper, gets absorbed into the protocol crates, or is dropped from the plan entirely.
- **`graph-cartography`**: listed in the Phase 1 portable set but never migrated. Read the donor crate; if it owns a load-bearing graph-traversal/path concern not covered by `graph-tree` / `platen`, migrate; otherwise drop it from the plan.

### 3.4 Legitimate Defers

- **Friction point #4 — split `graphshell` meta-crate from the reducer.** This is the one consumer-pull gate that holds up: splitting requires committing to a public API shape, and there is currently no second consumer to validate against. Leave deferred until either (a) a real second consumer of the shell vocabulary appears, or (b) the reducer surface becomes painful enough inside the meta-crate to force the split on its own merits.

### 3.5 Pitfalls

- Do not split Navigator into multiple instances; maintain the single surface with configurable scope/form factor.
- Do not let host adapters mutate graph truth directly while implementing surface command outcomes.
- Do not import concrete desktop/webview/renderer dependencies into `graphshell::app_state`, `platen`, `inker`, or `verso-tile`.
- Do not treat `verso-tile` as old `verso`; engine routing belongs in `inker`, graph-aware composition belongs in `platen`.
- Do not copy donor `GraphBrowserApp` method bodies wholesale; classify against the inventory, take the contract or invariant, and rebuild against the new owner crate.

### 3.6 Cadence

Run `cargo test --workspace` from `repos/mere` after every narrow change. File-size ceiling: 600 LOC per source file in `repos/mere`; split before adding when a touched file is approaching the limit. Touched-only file decomposition — don't open file-size cleanup as its own slice unless a file is actively blocking comprehension.

---

## 4. Findings

### 2026-05-06 — Migration Review

- Mere already has real foundation code: `mere-identity`, `mere-transport`, `murm`, and `murmuring` are not placeholders.
- `cargo test --workspace` in `repos/mere` passes before migration work begins.
- Old `repos/graphshell` is a donor, not the proof root. Its root workspace is in active decomposition and still contains stale Servo/Genet path assumptions.
- The current app module cuts in old Graphshell are good migration seams and should be preserved.
- The most important architectural refinement is not to carry old `verso` forward as-is. It must be split across `inker`, `platen`, and `verso-tile`.

### 2026-05-06 — Verb-Test Audit (graphshell + graphshell-core)

A full verb-test pass over `graphshell-core/src/` (30 files), `graphshell-runtime/src/` (11 files), `graphshell/src/` (10 files), and the `graphshell-host` / `graphshell-host-iced` adapter crates produced the seven friction points captured in §1.11. Headlines:

- `graphshell-core` is the largest mismatch: it carries portable vocabulary, shell session state, and runtime/host ports under one name. Three verbs, one crate.
- `graphshell-runtime` and `verso-tile` both touch surface lifecycle; the apply algorithm should consolidate inside `verso-tile` with the runtime providing only the host-port trait.
- `graphshell-host` is currently a shared catalogue, not a host adapter — the name promises what it does not yet deliver.
- `graphshell::app_state` re-exporting `inker::routing::*` and `graphshell::canvas` re-exporting `graph_canvas` are the two convenience re-exports most likely to harden into architecture.
- "Remember locally" is a top-level product verb, not an internal helper; the target crate name is `eidetic` (the prototype name `mnem` is unavailable on crates.io); promote when real persistence shape arrives.

The audit is recorded as authoritative input for the next slice planning. It is not a backlog: items #6 and #7 (drop the convenience re-exports) are the only cheap moves; the rest should wait for a concrete consumer to make the split useful.

### 2026-05-06 — "Remember Locally" Crate Naming

The verb-test audit's friction point #5 ("`mnem` is a first-class verb hiding as an internal module") prompted a crate-name decision.

- **Chosen target**: `eidetic` (evokes eidetic memory; available on crates.io and GitHub).
- **Runners-up**: `idyl` (resting/idle storage; phonetically pleasant), `eido` (shorter root of the same Greek stem). Kept as backups if `eidetic` becomes unsuitable later.
- **Rejected**: `mnem` — the prototype name. Unavailable on crates.io.

Rationale: `eidetic` immediately reads as "remembered with high fidelity" without explanation. `idyl` is cute but loads weaker semantics. `eido` is too short / too generic. `mnem` is unavailable.

Implication: when friction point #5 is acted on (promote local-memory module to a top-level crate), the crate goes to `crates/eidetic/` and the in-tree types (`MnemRequest`, `MnemResponse`, `MnemStore`, `dispatch_mnem_request`, the `WorkspaceEffect::RequestMnem` variant, `mnem_responses` report fields) all rename in the same slice. Bundled rename + extraction is one slice; no name-only refactor today.

### 2026-05-06 — All four verb-test friction points resolved

Friction points #6, #7, #2, #1 all landed in one session, alongside the eidetic extraction (#5) that shipped earlier in the day.

- **#6 (drop `inker::routing` re-export)** — small, mechanical. The re-export was only consumed inside graphshell itself via `super::`; submodules `app_state/services.rs` and `app_state/intent_system.rs` now import from `inker::routing` directly.
- **#7 (drop `graph_canvas` re-export)** — no external consumers; one-line removal.
- **#2 (consolidate surface schedule)** — `apply_viewer_surface_schedule` and helpers moved from `graphshell-runtime` into a new `verso-tile/src/apply.rs`. Closer reading distinguished `surface_schedule.rs` (verso-tile's verb, moved) from `webview_backpressure.rs` (host-runtime retry/cooldown, stayed). The audit conflated the two; the corrected framing is now in §1.11.
- **#1 (split `graphshell-core`)** — landed as a *two-way* split: a new `graphshell-shell-state` crate owns the (a) session state (12 modules: `shell_state/*` flattened, `ux_*`, `routing`); `graphshell-core` keeps both (b) vocabulary AND (c) port-trait *definitions*. The audit's three-way split (with port traits folded into runtime) was rejected mid-flight because it would have forced `verso-tile` to depend on `graphshell-runtime` (since verso-tile's apply algorithm uses `ViewerSurfaceHost` as a trait bound). Treating trait *definitions* as cross-consumer vocabulary preserves the press-stack peering.
- One small refactor along the way: `SurfaceId` lifted from `ux_observability` (now in shell-state) into `graphshell-core::accessibility`, since the `accessibility` descriptor table in core needs it. `ux_observability` re-exports it for back-compat.

All four moves verified by `cargo check --workspace --all-targets` (zero warnings) + `cargo test --workspace` (no failures across the full suite).

### 2026-05-06 — Friction point #3 (`graphshell-host` rename-by-content)

After the four-friction-point session above, walked through the two remaining open friction points:

- **#3** landed as a content move rather than a rename: `HostToolkit` / `NEW_HOST_TOOLKIT_ORDER` / `preferred_host_toolkits` migrated from `graphshell-host` into `graphshell-core::host_toolkit`. The toolkit catalogue is cross-consumer vocabulary, not host-side adapter machinery — its presence in `graphshell-host` was the source of the name mismatch. With the vocab gone, `graphshell-host` is now genuinely host-side (just the `viewer_surface_host` registry adapter).
- **#4** deferred per the audit's own guidance. The `graphshell` crate co-locates "compose portable crates for downstream consumption" and "own the application reducer." Splitting requires a second consumer wanting one without the other; Mere is currently the only consumer. Watching for the trigger.

Verification: `cargo check --workspace --all-targets` clean; `cargo fmt --all` clean; `cargo test --workspace` no failures.

---

## 5. Progress

### 2026-05-06

- Plan created.
- Workspace license corrected to MPL-2.0.
- `graph-memory` and `graph-tree` migrated into `crates/graph/`.
- `graph-canvas` migrated into `crates/graph/`.
- Removed the donor-side optional Vello/Rapier adapter dependencies from migrated `graph-canvas`; Cargo resolves optional dependencies into the workspace lock even with `--no-default-features`, so renderer and rigid-body adapters need future sibling crates instead of feature flags inside the portable canvas authority.
- `graphshell-core` migrated into `crates/graphshell/core`.
- `register-diagnostics` migrated into `crates/registry/` to unblock runtime without preserving the old `registrar/*` donor path.
- `graphshell-runtime` migrated into `crates/graphshell/runtime`.
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
- Added `inker::routing::EngineRoutePolicy` and `EngineRouteRule` as the first concrete route-policy home outside Graphshell. The default policy sends full web schemes to `genet.web`, smolweb schemes to `nematic.smolweb`, local files to `nematic.file`, internal Mere/Graphshell routes to a headless internal engine, and unknown protocols to a headless external-protocol handoff instead of guessing a webview.
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
- Added `verso_tile::surface::SurfaceLifecycleState` and `SurfaceCommandSchedule` so a portable surface lifecycle can accept a `SurfacePlacementPlan`, emit present-command schedules, record deferred host outcomes, and produce retry schedules without importing host widgets or renderer handles. `graphshell::app_state` re-exports the scheduler vocabulary but does not own its state.
- Verification: `cargo test -p graphshell -p platen -p verso-tile` passed with 49 Graphshell tests, 14 Platen tests, and 13 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Added `WorkspaceServices::dispatch_surface_schedule`, which applies a `SurfaceCommandSchedule` through the existing `SurfaceHost` trait and records deferred outcomes back into `SurfaceLifecycleState`. This keeps retry bookkeeping in the portable surface lifecycle while leaving concrete host widgets outside `graphshell::app_state`.
- Verification: `cargo test -p graphshell -p platen -p verso-tile` passed with 50 Graphshell tests, 14 Platen tests, and 13 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Added `NodeKey` to `verso_tile::surface::SurfaceSlotPlacement` so surface placement schedules carry the graph identity required by host viewer allocation, not just pane/slot identity. `platen::workbench::project_surface_placements` now forwards the arrangement member node into the placement packet.
- Added `graphshell_runtime::surface_schedule::apply_viewer_surface_schedule`, a portable runtime adapter that applies Verso-Tile present schedules to `graphshell_core::viewer_host::ViewerSurfaceHost`, reports applied/already-satisfied/deferred outcomes, and leaves concrete egui/iced/wry surface resources outside portable app state.
- Verification: `cargo test -p graphshell-runtime -p graphshell -p platen -p verso-tile` passed with 51 Graphshell Runtime tests, 50 Graphshell tests, 14 Platen tests, and 13 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Added retire scheduling to `verso_tile::surface::SurfaceLifecycleState`: existing pane placements can now produce retire schedules, and applied/already-satisfied retire outcomes remove the placement from lifecycle state.
- Kept `graphshell_runtime::surface_schedule::apply_viewer_surface_schedule` as the single runtime viewer-surface adapter and added retire handling through `ViewerSurfaceHost::retire_surface`.
- Verification: `cargo test -p graphshell-runtime -p graphshell -p platen -p verso-tile` passed with 53 Graphshell Runtime tests, 50 Graphshell tests, 14 Platen tests, and 14 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Removed the transitional `apply_present_surface_schedule` wrapper and the present-specific `already_present` report counter. Runtime schedule application now has one public adapter (`apply_viewer_surface_schedule`) and one generic `already_satisfied` outcome count for both present and retire commands.
- Verification: `cargo test -p graphshell-runtime -p verso-tile` passed with 53 Graphshell Runtime tests and 14 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Added `graphshell-host` as the first narrow host-side crate. It migrates the useful donor `verso-host` viewer-surface registry adapter idea as `SurfaceFactoryHost` / `ViewerSurfaceRegistryHost`, without the old `verso` dependency, renderer IDs, Tokio spawner, or concrete GUI toolkit imports.
- Verification: `cargo test -p graphshell-host -p graphshell-runtime -p verso-tile` passed with 2 Graphshell Host tests, 53 Graphshell Runtime tests, and 14 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Added a `graphshell-host` integration test proving `SurfaceFactoryHost` applies real `verso_tile::surface::SurfaceLifecycleState` present/retire schedules through `graphshell_runtime::apply_viewer_surface_schedule`, while keeping runtime and surface crates as test-only dependencies of the host adapter crate.
- Verification: `cargo test -p graphshell-host -p graphshell-runtime -p verso-tile` passed with 2 Graphshell Host unit tests, 1 Graphshell Host integration test, 53 Graphshell Runtime tests, and 14 Verso-Tile tests; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Added `graphshell_host::HostToolkit` and `NEW_HOST_TOOLKIT_ORDER` to make fresh host work explicit: iced first, then GPUI, HTML/CSS, Makepad, and egui last. The order is data in the host seam crate, not a donor-import side effect.
- Verification: `cargo test -p graphshell-host` passed with 4 Graphshell Host unit tests and 1 Graphshell Host integration test; `cargo fmt` completed; `cargo test --workspace` passed across Mere.
- Added `graphshell-host-iced` as the first fresh host crate. It implements an iced-shaped `HostSurfacePort` boundary with explicit present/retire request queues and content callback registration, but does not import iced, the old `GraphBrowserApp`, renderer IDs, or direct graph mutation paths.
- Verification: `cargo test -p graphshell-host -p graphshell-host-iced` passed with 4 Graphshell Host unit tests, 1 Graphshell Host integration test, and 4 Graphshell Host Iced tests; after relocating the support crates, `cargo test --workspace` passed, including the Graphshell family crates and doctests; `cargo fmt` completed.
- Removed the stale `crates/shell/` directory. Graphshell support crates now live under the Graphshell family root: `crates/graphshell/core`, `crates/graphshell/runtime`, `crates/graphshell/host`, and `crates/graphshell/host-iced`.
- Added crate-structure efficiency rules: prototype modules are behavioral evidence, not final boundaries; new crates must reduce dependency, ownership, validation, or change cost, and portable crates must stay free of host toolkit, renderer, task-runtime, and persistence implementation dependencies.
- Reframed §1.10 from cost-gate "Crate Structure Efficiency" to product-verb "The Verb Test." The lens is now: each crate must own a product question; cost reduction follows verb clarity, not the other way around.
- Added §1.11 Verb-Test Audit recording the seven friction points found in `graphshell-core` (three jobs in one crate), `graphshell-runtime` ↔ `verso-tile` (split surface lifecycle), `graphshell-host` (catalogue not host), the `graphshell` meta crate (compose + reduce co-located), `mnem` (verb-hiding-as-module), and two convenience re-exports (`inker::routing` from `app_state`, `graph_canvas` from `graphshell::lib`). Recorded the cheapest moves (#6, #7) and the highest-leverage move that should wait for a real trigger (#1).
- Findings updated with a 2026-05-06 audit summary.
- **Resolved all four verb-test friction points (#6, #7, #2, #1) in one session.** Code changes: dropped two convenience re-exports (#6, #7); moved `apply_viewer_surface_schedule` from `graphshell-runtime` into a new `verso-tile/src/apply.rs` (#2); extracted shell session state into a new `graphshell-shell-state` crate (#1, two-way split rather than the three-way audit candidate, after dep-direction analysis). Lifted `SurfaceId` from `ux_observability` into `graphshell-core::accessibility` to keep the accessibility descriptor table self-contained. All verified: `cargo check --workspace --all-targets` zero warnings; `cargo test --workspace` no failures.
- **Also resolved friction point #3.** Moved the toolkit catalogue (`HostToolkit`, `NEW_HOST_TOOLKIT_ORDER`, `preferred_host_toolkits`) from `graphshell-host` into `graphshell-core::host_toolkit` — that catalogue is cross-consumer vocabulary, not host-side machinery. `graphshell-host` is now genuinely host-side (just the `viewer_surface_host` registry adapter); name and contents match. `graphshell-host-iced` dropped its `graphshell-host` dep and imports `HostToolkit` from `graphshell-core` directly. Friction point #4 explicitly deferred per its own audit guidance — needs a second consumer of the shell vocabulary before splitting `graphshell` into meta-crate + reducer. Verified clean.
- Resolved the "remember locally" crate name: target is `eidetic` (the prototype name `mnem` is unavailable on crates.io). Runners-up `idyl` and `eido` documented as backups. §1.9 ownership table, §1.11 friction point #5, Phase 4 title and body, and §3 next-slice bullet all updated to use `eidetic` as the forward-looking crate name. In-tree module + types stay under `Mnem*` names until the bundled rename + crate-extraction slice — no name-only refactor today.
- **Extracted `eidetic` as a first-class top-level crate** at `crates/eidetic/`. Defines `Request` / `Response` / `Store` / `Error` / `Result` / `dispatch` as the public surface, with serde-derived request/response types and an owned error vocabulary. Bundled rename: `MnemRequest`→`eidetic::Request`, `MnemResponse`→`eidetic::Response`, `MnemStore`→`eidetic::Store`, `dispatch_mnem_request`→`eidetic::dispatch`, `WorkspaceEffect::RequestMnem`→`RequestEidetic`, `PersistenceIntent::RequestMnemBlobLoad/Save`→`RequestEideticBlobLoad/Save`, `mnem_responses`→`eidetic_responses`, `WorkspaceServices::mnem`→`eidetic`, `FakeMnemStore`→`FakeEideticStore`. Added `From<eidetic::Error> for WorkspaceServiceError` so existing `?` propagation in graphshell works unchanged. Deleted `crates/graphshell/src/mnem.rs` and removed `pub mod mnem;` from `graphshell::lib`. Cleaned up doc-comment references in `mere-identity` and `mere-transport`. Friction point #5 from §1.11 is resolved.
- Verification: `cargo fmt --all` clean; `cargo check --workspace --all-targets` clean (zero warnings); `cargo test --workspace` passes (eidetic 3 tests + full workspace, no failures); `cargo publish --dry-run --allow-dirty -p eidetic` packages 5 files at 9.3 KiB. Crates.io publication pending workspace `repository` field being set and Mark's `cargo login`.

### 2026-05-08

- **Rewrote §3 to drop consumer-pull gating.** Mark's pushback: "this 'lands when a consumer needs it' gating of features is ridiculous. the only time a consumer will exist is if we do the features. we're the only feature consumer!" The original §3 framed remaining work as "primary next slices" with deferral language throughout ("only when a real engine implementation needs them," "if a real donor method needs it," "only when new pending-effect kinds appear"). That framing pretended discipline but hid "we haven't decided to do the feature yet." In a single-consumer codebase, the gate freezes the codebase rather than disciplining it.
- New §3 structure: §3.1 Critical Path — "Browser Actually Runs" (one real host adapter, one real engine, one real protocol lane); §3.2 Feature Areas — Donor Rebuilds (rewrite-don't-classify-and-defer of runtime/webview lifecycle, history/undo, clip capture, persistence health, sync/storage); §3.3 Phase 1 Loose Ends (decide on `graphshell-comms` and `graph-cartography`, don't defer); §3.4 Legitimate Defers (only friction point #4 — splitting the graphshell meta-crate from the reducer needs a real second consumer to validate API shape); §3.5 Pitfalls (architectural invariants, kept); §3.6 Cadence.
- Donor inventory's "classify before importing" rule preserved as a quality gate on *how* features land, not a time gate on *whether* to start them. The plan now distinguishes quality-gating (correct shape on entry) from feature-gating (do the work because the product needs it).
- No code changes in this entry — pure plan rewrite.
- **Filled out `inker` and `nematic` per the rewritten §3.1 critical-path items #2 and #3.** Started immediately on the same day the consumer-pull gating came out, demonstrating the new framing in action: the work is the product, not "wait for a consumer."
- **`inker::engine`** (new module): defines the missing engine layer beneath routing. `Engine` trait (`engine_id`, `render(&EngineInput) -> Result<EngineDocument, EngineError>`); portable serializable document model (`EngineDocument`, `DocumentBlock`, `InlineSpan` covering headings, paragraphs, code blocks, quotes, lists, images, rules, and the inline span tree); `EngineInput` (already-fetched content — network/disk I/O remains the host's job, keeping engines portable to wasm32); `EngineRegistry` for engine-ID dispatch; owned `EngineError` vocabulary. `EngineDocument::outgoing_links()` walks nested inline spans (links inside emphasis, inside strong, inside quotes, inside list items) for graphshell edge population.
- **`nematic::markdown`** (new module): first concrete `Engine` implementation. CommonMark via `pulldown-cmark` 0.13. `MarkdownEngine` (engine ID `nematic.markdown`) covers headings, paragraphs with emphasis/strong/links/inline code/soft-and-hard breaks, recursive block quotes, ordered/unordered lists with multi-block items, fenced and indented code blocks, horizontal rules. Image alt text preserved as plain text in v1 (lossy but simple); HTML, footnotes, tables, math, and metadata blocks dropped. Stack-based converter handling Start/End event balancing across nested block + inline contexts. Tests cover engine-ID stability, H1-as-title extraction, link/emphasis preservation, code-block language preservation, list ordering, nested quotes, rules, image alt-text retention, and end-to-end dispatch through `inker::EngineRegistry`.
- Verification: `cargo test -p inker` 8 tests pass (3 new engine tests + 5 routing tests); `cargo test -p nematic` 11 tests pass; `cargo test --workspace` all green; `cargo fmt --all` clean; `cargo check --workspace --all-targets` clean. File sizes well under the 600 LOC ceiling (engine.rs ~290 LOC, markdown.rs ~370 LOC after fmt expanded enum variants).
- §3.1 critical-path remaining: #1 fresh iced host adapter (the only piece left for end-to-end demo); #2 partial — markdown engine wired but smolweb/file/web engines still TODO; #3 partial — markdown lane shipped, gemini/gopher/file/feed lanes pending.
- **Host pivot: iced → gpui.** Per a new memory entry, the host plan moved from iced-first to a Glass-HQ/gpui fork as primary, with iced and HTML/CSS retained as fallbacks. Vello / netrender / genet embed via a `PlatformSurface` trait using OS composition (not wgpu texture sharing). §3.1 #1 updated to reflect the new direction; the existing `graphshell-host-iced` port-boundary scaffold stays as a fallback adapter rather than the lead host.
- **`nematic` filled out with two more engines plus a registration helper.** Per Mark: "we've got a gpui host on the come up, due to iced's foibles. wanna keep adding to nematic and inker in the meantime?" — kept building engines while host work happens in parallel.
- **`nematic::gemtext`** (new module, ~410 LOC): `GemtextEngine` (engine ID `nematic.gemtext`) parses Gemini's `text/gemini` line-oriented format. Headings (3 levels), link lines (`=> URL [label]`), list items (`* `, consecutive lines merge into one list), quote lines (`> `, consecutive lines merge), preformatted blocks (` ``` ` toggle, alt text captured as language hint), paragraph accumulation across non-prefixed lines with blank-line separation. Single-pass state machine with a `Pending` enum tracking the current accumulator. Tests cover engine ID, H1 → title, three heading levels, link with/without label, list/quote merging, preformatted block alt-text + prefix-swallowing inside fences, paragraph soft-break preservation, end-to-end registry dispatch.
- **`nematic::text`** (new module, ~129 LOC): `TextEngine` (engine ID `nematic.text`) splits on blank lines into paragraphs, preserves soft breaks within paragraphs. Tests cover engine ID, blank-line separation, soft-break preservation, no title extraction, empty body, content-type override.
- **`nematic::engines()`** helper: returns `Vec<Box<dyn inker::Engine>>` of all default nematic engines for one-call registration. End-to-end test in `nematic::tests` confirms `EngineRoutePolicy::default().route("gemini://capsule.test/")` produces a decision with `engine_id == "nematic.gemtext"`, and dispatching through `EngineRegistry` registered with `engines()` yields a parsed `EngineDocument` with the H1 as the title. The full path from address to document is proven for the smolweb lane.
- **`inker::routing` default policy split.** Was: `["gemini", "gopher", "finger", "spartan"]` → umbrella `nematic.smolweb`. Now: `["gemini", "spartan"]` → concrete `nematic.gemtext`; `["gopher", "finger"]` → `nematic.smolweb` umbrella (kept until concrete engines exist for those protocols). Added `ENGINE_NEMATIC_GEMTEXT`, `ENGINE_NEMATIC_MARKDOWN`, `ENGINE_NEMATIC_TEXT` constants alongside the umbrella for direct-engine routes. Renamed test `default_policy_routes_smolweb_to_nematic` → `default_policy_routes_gemini_to_gemtext` and added `default_policy_routes_gopher_to_smolweb_umbrella`.
- **`inker::engine` enrichment** (linter / Mark hand-edit during this slice): added `EngineDocument.lang: Option<String>` (BCP 47) for AccessKit `Role::Document` projection; documented every block / inline variant with its AccessKit role mapping; promoted `inline_text` to a public helper on the inker side; added `tracing::instrument` on `EngineRegistry::dispatch` with engine-ID and address fields. Loose-list / tight-list markdown tests added.
- File sizes: `inker/src/engine.rs` ~426 LOC, `nematic/src/markdown.rs` ~583 LOC (close to the 600 LOC ceiling — split before next material change), `nematic/src/gemtext.rs` ~410 LOC, `nematic/src/text.rs` ~129 LOC, `nematic/src/lib.rs` ~132 LOC.
- Verification: `cargo test -p inker` 9 tests pass; `cargo test -p nematic` 24 tests pass (markdown 13 + gemtext 9 + text 6 + lib 3, with overlap on dispatch tests; total includes 3 lib-level end-to-end tests); `cargo test --workspace` all green; `cargo fmt --all` clean; `cargo check --workspace --all-targets` clean.
- **Three more pieces landed 2026-05-08 in one session: content-type routing, gopher menu engine, RSS/Atom feed engine.** The plan's "build the features because the product needs them" framing cashed out: each piece is the natural next slice once the prior one made the previous decision concrete.
- **Content-type routing in `inker`.** Added `EngineRouteRequest.content_type: Option<String>` (with `#[serde(default)]`) and `EngineRouteRule.content_types: Vec<String>` (also serde-default for back-compat). New `EngineRouteRule::content_type()` constructor builds rules with no scheme matching, only MIME matching. `EngineRoutePolicy::route` prefers content-type rules when the request carries a content type, falling back to scheme rules and then the global fallback. Match is case-insensitive and ignores `; charset=…` parameter suffixes. Default policy gained content-type rules for `text/markdown` / `text/x-markdown` → `nematic.markdown`, `text/gemini` → `nematic.gemtext`, `text/plain` → `nematic.text`, `application/{rss,atom,feed}+xml` → `nematic.feed`. This unblocks markdown reachability via http/https without inventing a `markdown:` scheme, and lets a fetch response steer the second-pass route after the initial scheme route picked a fetcher. Five new inker tests cover the content-type path.
- **`nematic::gopher`** (new module, ~347 LOC): `GopherEngine` (engine ID `nematic.gopher`) parses RFC 1436 gopher menus. Each line: `<type><display>\t<selector>\t<host>\t<port>`. Type characters `i` (info) merge into paragraph runs across consecutive info lines; `0`/`1`/`7`/`9`/`g`/`I`/`s`/`T`/unknown become links with `gopher://host[:port]/<type><selector>` URLs synthesised per RFC 4266 (omitting `:70` since it's the default port); `h` items extract the actual URL from the `URL:` selector prefix; `3` server errors fold into info paragraphs with `[error]` tag; lines with empty host are skipped. The bare `.` line terminates parsing. 11 tests cover URL synthesis, non-default port, URL extraction, info merging, info+resource interleave, terminator behaviour, error handling, malformed-line skipping, link extraction, end-to-end registry dispatch.
- **`nematic::feed`** (new module, ~527 LOC): `FeedEngine` (engine ID `nematic.feed`) parses RSS 2.0 and Atom 1.0 with one event-driven walker via `quick-xml` 0.39. Both formats share the same logical shape — feed-level title plus a sequence of entries with title/link/summary — so a single state machine with a path stack handles both. RSS expresses links as element text (`<link>https://…</link>`); Atom uses self-closing `<link href="…"/>` with the URL in an attribute, captured on `Event::Empty`. Output: feed title → `EngineDocument.title`; each entry → level-2 heading + paragraph with link + (optional) paragraph with de-tagged summary. RSS `<language>` populates `EngineDocument.lang`. **quick-xml 0.39 wrinkle:** XML entity references (`&lt;`, `&gt;`, `&amp;`, numeric refs) are emitted as their own `Event::GeneralRef` separate from surrounding `Event::Text` chunks; without explicit handling, the entity content is lost AND surrounding whitespace gets eaten by `trim_text`. Fix: handle `GeneralRef` events explicitly (resolving via `quick_xml::escape::unescape`) and disable `trim_text` (commit-time `trim()` handles boundary whitespace correctly). Truncated XML (path stack non-empty at EOF) returns `EngineError::InvalidContent` instead of silently producing a partial document. 8 tests cover RSS, Atom, HTML stripping in `<description>` / `<content>`, empty feed, malformed XML, and end-to-end dispatch via the default policy with `application/rss+xml` content type.
- **`nematic::engines()` extended** to register all five engines (markdown, gemtext, gopher, feed, text). `application/feed+xml` content type routes via `EngineRoutePolicy::default()` directly to the feed engine — proven by `feed::tests::end_to_end_via_default_policy_with_content_type`.
- **Routing tweak:** `gopher` scheme now routes to concrete `nematic.gopher` engine; `finger` stays on the `nematic.smolweb` umbrella until a finger engine exists. Test renamed `default_policy_routes_gopher_to_smolweb_umbrella` → `default_policy_routes_gopher_to_gopher_engine`; new `default_policy_routes_finger_to_smolweb_umbrella`.
- File sizes: `inker/src/routing.rs` ~415 LOC; `nematic/src/feed.rs` ~527 LOC; `nematic/src/gopher.rs` ~347 LOC. All under the 600 LOC ceiling. `markdown.rs` is still at 583 LOC and needs splitting before any next material change.
- Verification: `cargo test -p inker` 15 tests pass (was 9; +5 content-type, +1 finger umbrella); `cargo test -p nematic` 53 tests pass (markdown 13 + gemtext 9 + gopher 11 + feed 8 + text 6 + lib 4 + overlap; was 33); `cargo test --workspace` 1099+ passing, no failures; `cargo fmt --all` clean; `cargo check --workspace --all-targets` clean.
- §3.1 critical-path remaining: only the host adapter (gpui-first per the 2026-05-08 pivot, with iced/HTML+CSS as fallbacks). The content path is now end-to-end provable for markdown, gemtext, gopher menu, feeds, and plain text — routing decides, registry dispatches, engine produces a portable `EngineDocument` with `outgoing_links()` exposing the navigation graph. The host just needs to render `DocumentBlock`s to pixels and route input back as intents.
- **Final two engines + availability filter (2026-05-08).** Mark: "more to add to nematic and inker? the goal's to get all 'em" — finished off the smolweb engine slots and added a host-side composability primitive.
- **`nematic::file`** (new module, ~234 LOC): `FileEngine` (engine ID `nematic.file`) owns one instance of each delegate engine and dispatches by file extension (`.md`/`.markdown`/`.mkd`/`.mdown` → markdown; `.gmi`/`.gemini` → gemtext; `.gophermap`/`.goph` → gopher; `.xml`/`.rss`/`.atom` → feed; everything else → text). Extension extraction handles `file://` schemes, query strings, fragments, trailing slashes; case-insensitive; treats hidden files (`.gitignore`) as having no extension. Hosts that load a file body without knowing its MIME hand the whole input to this engine; hosts that *do* know the MIME set `EngineInput::content_type` and let the inker policy's content-type rules win — explicit MIME beats extension sniff. 10 tests including end-to-end via the default `file://` route.
- **`nematic::finger`** (new module, ~115 LOC): `FingerEngine` (engine ID `nematic.finger`) wraps `TextEngine` with finger-specific content-type tagging (`text/x-finger`). Finger responses are plain text per RFC 1288 — no structure beyond the lines themselves — so a separate engine looks redundant against `nematic.text`, but having a distinct lane gives telemetry / logging / future finger-specific structure handling a stable engine ID to attach to. Replaces the `nematic.smolweb` umbrella for `finger://`. 4 tests.
- **Retired `ENGINE_NEMATIC_SMOLWEB`** entirely. The umbrella existed as a placeholder for protocols without concrete engines; with gopher and finger now both filled, no protocol is using it. Removed the constant from `inker::routing`. Pre-alpha breaking change with no external consumers — clean cut.
- **`EngineRegistry::contains(id) -> bool`** (new) — paired with **`EngineRoutePolicy::route_filtered(request, is_available)`** (new) to let hosts route only to engines they actually have registered. When a matched rule's engine isn't available, routing walks through to the next rule rather than producing a decision pointing at an unregistered engine. Use case: a wasm-only host can register a subset of engines (markdown + gemtext + text) and route requests through `policy.route_filtered(req, |id| registry.contains(id))` — `https://` requests fall through to the external-protocol fallback rather than dispatching to a non-existent Genet engine. The plain `route()` is now a thin wrapper that passes `|_| true`. 3 new inker tests.
- **All engine slots filled.** Inker's default policy now has only fully-implemented engine IDs in its scheme rules: every `nematic.*` ID is backed by a concrete engine in the `nematic` crate, registered by `nematic::engines()`. The remaining unfilled slots (`genet.web`, `graphshell.internal`, `host.external-protocol`) belong to other crates by design and are not nematic's responsibility.
- File sizes: `nematic/src/file.rs` ~234 LOC; `nematic/src/finger.rs` ~115 LOC. Both well under the 600 LOC ceiling.
- Verification: `cargo test -p inker` 18 tests pass (was 15; +3 filter); `cargo test -p nematic` 68 tests pass (was 53; +15: file 10 + finger 4 + lib 1); `cargo test --workspace` 1113+ passing, no failures; `cargo fmt --all` clean; `cargo check --workspace --all-targets` clean.
- **Semantic-document enrichment + knot note format + smolweb completionist (2026-05-08).** Mark, after exploring the donor's `middlenet-core::SemanticDocument`: "is there a semantic document lane like scroll? that will be important for notes/clips made in the graph; we want semantic information after all, helps fuel intelligence." Three slices ran in sequence: (1) enrich `EngineDocument` with provenance / trust / diagnostics + semantic block variants; (2) add a knot note format engine; (3) port the donor's smolweb stub formats (scroll, misfin, nex, guppy) as protocol-spec-faithful body engines.
- **§1 — `inker::document` extracted as its own module** (was inside `engine.rs`). Added `DocumentProvenance` (source_kind / canonical_uri / fetched_at / source_label), `DocumentTrustState` (Trusted / Tofu / Insecure / Broken / Unknown), `DocumentDiagnostic` (UnsupportedConstruct / DegradedRendering / ParseWarning / RawSourceFallback). New semantic block variants on `DocumentBlock`: `FeedHeader`, `FeedEntry`, `MetadataRow`, `Badge` — each maps to a real concept in some protocol's spec, so adopting them is *more* spec-faithful than treating RSS items as generic Heading+Paragraph. `EngineDocument::to_markdown()` and `::to_gemini()` round-trip renderers landed in a `document::render` submodule (split for the 600-LOC ceiling). All 7 existing nematic engines populate `provenance`; `trust` defaults to `Unknown` (host fills it from transport).
- **Feed engine adopted `FeedHeader` + `FeedEntry`** with entry dates from `<pubDate>` / `<published>` / `<updated>` and a `DegradedRendering` diagnostic when HTML is stripped from summaries. `uxtree` projects new variants to AccessKit roles: `Article` for FeedEntry, `Section` for FeedHeader, `Group` for MetadataRow, `Note` for Badge.
- **§2 — `nematic::knot`** (~429 LOC): file format defined by Mere itself, so it's allowed to be richer than the protocols. YAML-shaped frontmatter (key:value, [a,b,c] arrays, optional quote stripping; hand-rolled — no YAML dep) populates `provenance.canonical_uri` (from `source`), `provenance.fetched_at` (from `captured`), `provenance.source_label`, trust state (from `trust: trusted|tofu|insecure|broken`). `title` overrides body H1; `note_kind` and `tags` emit as MetadataRow blocks. Body parses through internal `MarkdownEngine`. Wired into FileEngine `.knot` dispatch + inker default policy `text/x-knot` content-type rule. Engine ID `nematic.knot`; default content-type `text/x-knot`. **This is the lane that fuels intelligence over notes / clips:** every knot carries explicit provenance, trust state, and kind, so downstream search / summarise / recall match on meaning, not just text.
- **§3 — donor smolweb stubs ported** as spec-faithful body engines. Each inherits provenance and trust handling from the document model, leaves envelope/transport concerns upstream, and defaults trust to `Unknown` until the host validates the envelope:
  - **`nematic::scroll`** (~179 LOC): scroll.mozz.us body engine. Body content-type → gemtext (default) or markdown. Emits `UnsupportedConstruct` diagnostic noting envelope signature verification is the transport's job, not ours.
  - **`nematic::misfin`** (~134 LOC): misfin.org gemini-style mail. Body is gemtext per spec; delegates to `GemtextEngine`. Tags content-type as `message/x-misfin`. Emits diagnostic about envelope (sender / recipient / timestamp / cert) not being parsed in this engine.
  - **`nematic::nex`** (~270 LOC): nex.nightfall.city. **Real format implementation** — heuristic detects directory listings (every non-empty line is a short whitespace-free entry, optionally ending with `/`) versus plain content. Directory entries emit as a `List` of `Link` spans with synthesised `nex://` URLs resolved against the request address; content responses delegate to `TextEngine`. Base-URL synthesis correctly handles trailing-slash, no-slash-after-scheme, and path-tail-stripping cases.
  - **`nematic::guppy`** (~97 LOC): guppy.mozz.us UDP-smolweb. Body is gemtext after host transport reassembly; delegates to `GemtextEngine` with guppy-tagged provenance.
- **Inker default policy**: scheme rules added for `scroll://`, `misfin://`, `nex://`, `guppy://` plus `text/x-knot` content-type rule.
- **File-size ceiling enforcement**: `inker/src/document.rs` (was 844 LOC after enrichment) split — types stay in `document.rs` (~431 LOC), round-trip renderers extracted to `document/render.rs` (~443 LOC) via re-opened `impl EngineDocument` and `impl DocumentBlock` blocks. `nematic/src/feed.rs` (was 641 LOC) split — implementation in `feed.rs` (~412 LOC), tests extracted to `feed/tests.rs` (~233 LOC) via mixed-form module declaration (`mod tests;` pointing at `feed/tests.rs`). All files now under the 600 LOC ceiling.
- File sizes after step 3: `nematic/src/scroll.rs` 179, `nematic/src/misfin.rs` 134, `nematic/src/nex.rs` 270, `nematic/src/guppy.rs` 97. All under 600.
- **Twelve concrete `nematic.*` engines now ship** (was seven): markdown, gemtext, gopher, feed, text, file, finger, knot, scroll, misfin, nex, guppy. Inker has a stable engine ID constant for each.
- Verification: `cargo test -p inker` 24 tests pass (was 18; +6 document module: provenance / trust / feed-link-walk / 4 round-trip render tests in `document/render.rs`); `cargo test -p nematic` 105 tests pass (was 68; +37: feed updates 4, knot 13, scroll 4, misfin 4, nex 7, guppy 3, lib helper updates); `cargo test --workspace` 1160 passing, no failures; `cargo fmt --all` clean; `cargo check --workspace --all-targets` clean.
- §3.1 critical-path remaining: the host adapter (gpui-first per the 2026-05-08 pivot). The content path is now end-to-end provable for **every** smolweb / file / note format: routing decides → registry dispatches → engine produces an `EngineDocument` carrying structured blocks, semantic blocks (FeedEntry / MetadataRow / etc.), provenance, trust state, and diagnostics → `to_markdown` / `to_gemini` round-trip back to native formats. Notes / clips have a real graph-native lane (`nematic.knot`); intelligence layers downstream can match on `provenance.source_kind`, `trust`, and the semantic block intent, not just text.
- **Polyglot knot landed (2026-05-08).** Mark articulated knots as the **superset language** for graph-native notes: each protocol's grammar (gemtext, gopher, RSS, etc.) can embed inside a knot via fenced code blocks, coexisting alongside markdown prose, with each block faithfully preserving its source protocol. Design captured in [`../../nematic_docs/implementation_strategy/2026-05-08_polyglot_knot_design.md`](../../nematic_docs/implementation_strategy/2026-05-08_polyglot_knot_design.md) (new `nematic_docs/` tree, per DOC_POLICY's planned layout).
- **Implementation:** knot body parsing now runs a polyglot post-process pass. After the markdown engine produces blocks, `nematic::knot::expand::expand_fenced_blocks` walks the block list (recursing into Quotes and List items) and replaces any `CodeBlock` whose language tag is `gemtext` / `gopher` / `nex` / `feed-entry` / `feed-header` / `metadata-row` / `badge` with the blocks the corresponding protocol's engine (or a small inline parser for the semantic-only fences) would produce. Unknown languages pass through as code blocks unchanged — `python`, `rust`, etc. don't get expanded.
- **Inline parsers** for the four semantic-only fences (`feed-entry`, `feed-header`, `metadata-row`, `badge`) live in `nematic::knot::expand`. They share a small `parse_kv_lines` helper that preserves key case (so `Login: alice` becomes `MetadataRow { label: "Login", value: "alice" }`) while schema-matching parsers (feed-entry / feed-header) lowercase their own comparison keys.
- **Round-trip via `EngineDocument::to_knot()`** in `inker::document::render`. Structural blocks render as standard CommonMark; the four semantic block variants render as fenced code blocks with their language tag (a `FeedEntry` becomes a `feed-entry` fence with `title:` / `url:` / `date:` / `summary:` lines, etc.). Recurses into Quote / List so nested semantic blocks also fence properly. Round-trip test confirms `parse → blocks → to_knot → parse → equivalent blocks`.
- **`nematic::knot::build_clip_knot(blocks, provenance, trust, note_kind) -> String`** assembles a complete knot file from raw blocks plus a source's `DocumentProvenance` and trust state. Frontmatter populated from provenance (source/captured/source_label) plus the supplied trust + optional note_kind; body uses `to_knot()` so semantic blocks fence correctly. The host's clip gesture wires up to this — once the gpui host lands, "select element on a tile + clip-to-knot" becomes a one-call helper.
- **File-size split first**: `nematic/src/knot.rs` (was 429) split before adding the expand pass — knot.rs (268 LOC, engine + frontmatter) + `knot/expand.rs` (268 LOC, fence expansion + inline parsers + build_clip_knot) + `knot/tests.rs` (403 LOC, all tests via mixed-form `mod tests;`). All under 600. `inker/src/document/render.rs` grew from 443 → 556 with `to_knot()`; still under but on watch.
- Verification: `cargo test -p inker` 24 tests pass (no change); `cargo test -p nematic` 117 tests pass (was 105, +12: 7 polyglot expansion + 5 to_knot round-trip + clip-knot builder); `cargo test --workspace` 1172 passing, no failures; `cargo fmt --all` + `cargo check --workspace --all-targets` clean.
- §3.1 critical-path remaining (still): host adapter. With knot polyglot landed, the content path is now richer: a clip gesture takes any selection of blocks from any tile, calls `build_clip_knot`, and saves a `.knot` file that round-trips into a document carrying every protocol's faithful representation alongside the user's own markdown prose. The graph-native note format is what fuels intelligence — `provenance` for "where", `trust` for "how authenticated", semantic block intent for "what kind of content" — and now mixes any combination of the twelve nematic engines into one document.

### 2026-05-09

- **Knot wikilinks + hashtags + inker richer routing.** Two parallel slices on top of the polyglot knot foundation. Mark also reframed the would-be HTML reader-mode lane: rather than building it in nematic, it belongs in **Genet as a three-head Hekate negotiator** (smolweb extract / middlenet / fullweb modes for the same HTML input) — captured in the [Blitz/Genet convergence memory](memory) as the strategic direction. **Don't build `nematic.html-reader`**; HTML in any depth is Genet's job.
- **Knot wikilinks** (`[[node-name]]`): the knot post-process pass now runs an inline-rewrite step after fence expansion. Wikilinks become `InlineSpan::Link { url: "mere://node/<slug>" }` where `<slug>` is the lowercased name with whitespace → `-`. Display text preserves the original surface form. The link routes through inker's existing `mere://` → `graphshell.internal` rule, so wikilinks are first-class graph navigation. Implementation needed pre-merging adjacent `Text` spans first because pulldown-cmark splits `[[my note]]` into separate Text events for `[`, `[`, `my note`, `]`, `]`.
- **Knot hashtags** (`#tag`): `#tag` tokens at word boundaries are *extracted* (not preserved inline) from paragraph text and emitted as sibling `DocumentBlock::Badge` blocks after the containing paragraph. Hashtags inside headings stay as text — only paragraphs extract. Boundary detection: `#` at start-of-string OR preceded by whitespace / `(`/`[`/`,`/`.`/`;`. Wikilinks inside an existing markdown link are not re-rewritten (the outer link's display text wins).
- **Inker richer routing**: the README's two planned expansions landed.
  - **Pinned engine** on the request: `EngineRouteRequest.pinned_engine: Option<String>`. When set and the engine is available per the active filter, wins over everything (content-type, per-host, scheme). The most explicit user signal — "this node always uses X."
  - **Per-host overrides** on the policy: `EngineRoutePolicy.per_host_overrides: HashMap<String, String>` mapping a case-insensitive host string to an engine ID. Wins over scheme rules but **loses to content-type rules** — server's claimed content type is more authoritative than the user's general domain pref. New `host_from_address(&str) -> Option<&str>` helper strips scheme / userinfo / port to get the bare host for matching.
  - Final priority: pin > content-type > per-host > scheme > fallback. Engine availability filter applies at every step.
- **Splits first.** `inker/src/routing.rs` (was 682 with the new tests) → `routing.rs` 406 LOC + `routing/tests.rs` 280 LOC via mixed-form `#[cfg(test)] mod tests;`. Same pattern used for `feed.rs`.
- **Construction-site updates** for the new `pinned_engine` field: 4 sites across `routing.rs` test helpers, `graphshell::app_state::services` test, and 3 nematic test sites (feed, file, lib end-to-end). Each adds `pinned_engine: None`. Per-host overrides default-empty so existing `EngineRoutePolicy::default()` rules don't change behaviour.
- Verification: `cargo test -p inker` 31 tests pass (was 24; +7 routing extensions: pin/per-host/host-extraction); `cargo test -p nematic` 126 tests pass (was 117; +9 wikilink + hashtag); `cargo test --workspace` 1206 passing (was 1172, +34 once Mark's `mere-kernel` / `mere-host-contract` / `mere-host` / `mere-domain/*` crates joined the count); `cargo fmt --all` + `cargo check --workspace --all-targets` clean.
- §3.1 critical-path remaining: still the host adapter. The content path now also resolves `mere://node/<name>` wikilinks through `graphshell.internal`, so a knot referencing `[[research-log]]` produces a navigable link the host can route to. Per-node engine pins + per-domain prefs are ready for the host's settings UI.
