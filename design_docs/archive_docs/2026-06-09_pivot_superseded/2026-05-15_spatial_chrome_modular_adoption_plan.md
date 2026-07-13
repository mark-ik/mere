# Spatial Chrome + Renderer Registry Modular Adoption Plan

**Date**: 2026-05-15 (initial); 2026-05-17 implementation status added
**Status**: Implementation strategy (sequencing plan; does not commit substrate-as-host adoption). Phases 2–3 largely shipped; phase 4 has structural skeleton.
**Scope**: Turns the spatial chrome IR, renderer registry contract, browser taxonomy translation, OS-plumbing audit, and existing multiplexer/session work into a staged plan. Goal: land the useful host-agnostic pieces first, defer substrate-as-host until proof gates clear, and keep Genet/NetRender/wgpu-* sibling crates aligned with Mere's renderer model.

**Primary inputs**:

- [`../research/2026-05-15_spatial_chrome_ir_brief.md`](../research/2026-05-15_spatial_chrome_ir_brief.md)
- [`../research/2026-05-15_browser_taxonomy_translation_brief.md`](../research/2026-05-15_browser_taxonomy_translation_brief.md)
- [`../research/2026-05-15_renderer_registry_contract_brief.md`](../research/2026-05-15_renderer_registry_contract_brief.md)
- [`../research/2026-05-15_os_plumbing_reuse_audit_brief.md`](../research/2026-05-15_os_plumbing_reuse_audit_brief.md)
- [`2026-05-11_graph_session_manifest_plan.md`](2026-05-11_graph_session_manifest_plan.md)
- [`2026-05-14_session_service_runner_plan.md`](2026-05-14_session_service_runner_plan.md)
- [`2026-05-14_engine_profile_boundary_plan.md`](2026-05-14_engine_profile_boundary_plan.md)
- [`2026-05-11_relation_taxonomy_and_edge_mutation_plan.md`](2026-05-11_relation_taxonomy_and_edge_mutation_plan.md)

---

## Thesis

> **Ship the renderer registry and taxonomy cleanup before any host pivot. Treat the spatial chrome IR as the desired shape of Mere's presentation/session layer, not as a mandate to replace gpui immediately. The adoption path is registry first, composition proof second, OS-plumbing proof third, substrate-host decision last.**

This plan deliberately separates five threads that are easy to conflate:

1. Browser taxonomy/documentation cleanup.
2. Renderer registry adoption under the current host.
3. NetRender/Vello composition proof.
4. Browser/PWA and p2p envelope constraints.
5. Substrate-as-host adoption decision.

## Implementation status (2026-05-17)

Snapshot of what's landed in code vs. the plan as filed two days prior.
Update this section in place when a phase advances; do not spawn a
status doc per `DOC_POLICY.md`.

### Phase 2 (renderer registry under current host) — **shipped**

- `mere-renderer-registry` + `mere-renderer-registry-types` extracted as workspace crates (not held in `mere-host-runtime` as the plan suggested — the wasm-clean split into a types crate justified the separate home; types crate carries the wasm-safe surface, full registry carries the vello/wgpu-bound traits).
- `NodeRenderer` / `InScenePaintRenderer` / `EmbeddedFrameRenderer` / `OverlayRenderer` / `RendererSelector` / `RendererId` / `NodeContentKind` / `CompositionMode` / `RendererCapabilities` all defined and re-exported through the full crate.
- `RendererRegistry::paint_node` / `deliver_in_scene_input` dispatch helpers added — close the "registry resolves but doesn't call" gap from the v0 contract.
- `as_in_scene_paint` / `as_embedded_frame` / `as_overlay` downcast accessors on `NodeRenderer` give the registry a dyn-trait dispatch seam.
- `DispatchError::WrongCompositionMode` surfaces renderer-impl bugs.
- **Selector chain (contract brief §5)**: steps 1 (per-node `renderer_pin` on `SceneNodeRef`) and 4 (`set_default_policy(kind, id)` per-content-kind) live on `RendererRegistry::select`; step 5 (first-candidate tie-break) is the `DefaultSelector` strategy. Steps 2–3 (profile-binding constraint, host capability filter) wait on host-side surfaces.
- **Diagnostic events (contract brief §8)**: `DiagnosticEvent::{RendererRegistered, RendererUnregistered, RouteDegraded}` emitted through a `DiagnosticSink` trait. `NoopSink` is the default; `RecordingSink` lets tests inspect emissions. Registered/unregistered fire at lifecycle points; `RouteDegraded { NoCandidates | WrongCompositionMode }` fires from the registry's dispatch helpers when the chain or downcast fails. Per-frame `route_chosen`, `hot_swapped`, and `surface_attach_failed` events remain unwired — they need host-side dedupe / consumer integration.

Capability-gate threading (per-node `engine.route_override`, profile escalation) and the 4-layer policy chain remain unwired — both flow through the host action bus once a real host is the integration point.

### Phase 3 (composition proof) — **mostly shipped**

Crate isolation chosen (`crates/mere-spatial-prototype/`, the plan's second suggested home).

Done-condition checklist:

- ✅ "Mixed scene without gpui-specific layout concepts" — substrate dispatches both InScene (`SolidRectRenderer`, `RecordingRenderer`) and EmbeddedFrame (`MasonryEmbeddedRenderer`, `ScryingEmbeddedRenderer` skeleton) nodes through a single `SubstrateHost::render_scene` path; gpui is nowhere in the dep tree.
- ✅ "Embedded-frame and in-scene paint surfaces share a coordinate/hit-test model" — both go through `SceneNodeRef` (placement + size) and the unified `SubstrateScene::hit_test`.
- ✅ "Driven by deterministic test data" — 36 unit + 2 GPU-integration tests in `mere-spatial-prototype`, 14 tests in `mere-masonry` (incl. 2 cross-crate `substrate_integration.rs`), 3 in `scrying-embedded-renderer`.
- ✅ "Windows local validation exists" — render-to-PNG integration test (`tests/render_to_png.rs`) produces a viewable PNG at `target/spatial_prototype_render.png` showing nodes + edge; windowed demo binary at `mere-masonry/examples/windowed_demo.rs` opens a real winit window with live `deliver_input_at` mouse-click routing.

Per-work-item:

| Item | Status |
| --- | --- |
| Root scene with pan/zoom camera | ✅ `SubstrateHost::camera()` / `set_camera()` / `pan(dx, dy)` / `zoom_at(pivot, factor)` / `scene_pos_from_host()` (`crates/mere-spatial-prototype/src/host.rs`). Camera composes onto each node's placement during dispatch; `deliver_input_at` pulls host_pos through `camera.inverse()` before hit-testing. |
| One cartography/graph node | Pending — needs cartography concept work (separate crate; not yet started). |
| One document tile rendered through engine → platen → NetRender | Pending — needs `inker::Engine` integration; tracked separately from the spatial prototype's renderer-registry surface. |
| One embedded-frame placeholder texture | ✅ `MasonryEmbeddedRenderer` (real masonry render via `imaging_vello`'s GPU path) + `ScryingEmbeddedRenderer` skeleton (next_frame wired against `WgpuTextureImporter`). |
| One relation edge with label and hit-test | ✅ for *geometry* (line stroke, `hit_test_edge` with `DEFAULT_EDGE_HIT_TOLERANCE`); label *rendering* pending parley wiring (`label: Option<String>` stored on `RelationEdge` for now). |
| Basic hit-test returning node/edge/content identity | ✅ `SceneHit::{Node, Edge}` returned from unified `SubstrateScene::hit_test`. |
| Thumbnail/capture path for switcher preview | ✅ `SubstrateScene::content_bounds()` + `fit_camera(bounds, target_size, ThumbnailFit)` (`crates/mere-spatial-prototype/src/thumbnail.rs`). Integration test `tests/render_to_thumbnail_png.rs` produces `target/spatial_prototype_thumbnail.png` at 128×128 from a 512×384 scene. |
| UxTree/AccessKit projection | ✅ for substrate-level nodes (`accessibility::project_scene` emits `TreeUpdate` with role-mapped nodes + AABB bounds). Renderer-contributed sub-trees (e.g. `MasonryTile::take_accesskit_update`) not yet merged — `EmbeddedFrameRenderer` trait needs to surface its pending tree updates first. |
| LOD-promotion system (IR brief §5) | ✅ `compute_lod_for_node(node, camera, thresholds)` + `LodThresholds` (`crates/mere-spatial-prototype/src/lod.rs`). Substrate rewrites `SceneNodeRef.lod` per frame in `paint_scene` / `render_scene` from apparent host-pixel size of `camera * placement`; stored LOD acts as a floor (never demotes below the producer's declared minimum). |

### Phase 4 (native texture interop consolidation) — **skeleton**

- `scrying-embedded-renderer` wires `scrying::WebSurfaceProducer` + `WgpuTextureImporter` into the registry's `EmbeddedFrameRenderer`. `next_frame` matches `WebSurfaceFrame::Native(_)` → imports to `wgpu::Texture`; non-native variants and errors fall back to last cached texture (frozen-frame semantics).
- The plan's call to "create or designate one low-level interop crate" — *designate* `scrying::native_frame` as that crate, since it already has `NativeFrame` / `ImportedTexture` / `TextureImporter` / `HostWgpuContext` / sync mechanisms and is the most production-tested. Renaming + extraction is a follow-up; the contract is already shaped.
- xilem / forest-rs/imaging local fork (`repos/imaging` branch `mere-wgpu-29-vello-0-9`) added `wgpu-29` + `vello-0-9` feature aliases — dissolves the prior wgpu 28 ↔ 29 skew that would have blocked the GPU path. Tracked back to upstream via the `mere-wgpu-29-vello-0-9` branch on both xilem and imaging local clones.

Capability gates (per-node route override / profile escalation) and diagnostics emission still unwired; they live on the host action bus once a real host is the integration point.

### Phases 5–8 — **untouched**

PWA/browser envelope, session/p2p sync boundary, OS-plumbing proof gate, substrate-as-host parity demo. The strong-host-model decision the rest of these phases depend on is the next big architectural milestone; no implementation work warranted until that lands.

### Cross-cutting status

- Version alignment: post-fork, xilem + mere both on wgpu 29 + vello 0.9 + parley 0.9. mere-masonry runs against `imaging_vello`'s GPU path (not the earlier CPU readback fallback). xilem fork branch tracks upstream; clean PR candidate.
- 54 tests across the four prototype crates, all green.
- The "no running prototype demonstrating substrate-as-host shape; risk of paper architecture" pitfall from the spatial chrome IR brief's preconditions is decisively dead — there's both a render-to-PNG integration test producing a viewable artifact and a windowed demo binary.

## 0. Current state

Already present or in flight:

- `inker` has document engines plus `SurfaceEngine` / `SurfaceProducer` shape for long-lived frame streams.
- `scrying-tile-engine` already adapts scrying into an embedded-frame-style tile producer.
- `EngineProfileBinding` has a pure path resolver and profile-binding plan.
- `GraphSessionManifest`, `ViewIntent`, session/service runner, action bus, capability gates, and diagnostics have planned seams.
- Relation taxonomy and cartography are already separated from tile rendering.
- NetRender already has scene/text/external-texture/hit-test work that can support a substrate proof.
- `wgpu-scry` and `wgpu-weld` both have native-frame/web-surface production ideas that should converge on one interop contract.

Still missing:

- No canonical `NodeRenderer` registry.
- Host dispatch still risks bespoke per-renderer branches.
- No Workbench-sized NetRender/Vello substrate proof.
- No accepted OS-plumbing extraction/validation plan beyond the audit posture.
- No explicit PWA/browser-host degraded envelope.
- No shared native texture interop crate across scrying/weld/Genet/NetRender.

## Cross-cutting prerequisite — single-write-path invariant on mere-kernel

Per [graphshell harvest brief](../research/2026-05-17_graphshell_harvest_brief.md) Tier 1 / T1-1: all durable graph mutations flow through one reducer entry on `mere-kernel`'s `Graph` (e.g. an `apply_intents()` boundary); direct mutations raise diagnostics. Enforced compile-time via `pub(crate)` on the graph mutators, runtime via an `INV-1`-style invariant check. The [typed action bus](2026-05-11_typed_action_bus_plan.md) is the natural carrier for the "sole intent path" guarantee — bus actions become the only legal mutation source.

This sits above the spatial-chrome lane but blocks none of it: phases 2–4 can proceed without the invariant landing, because the host doesn't reach into mere-kernel internals — it dispatches through the action bus already. The invariant is what makes that *enforced* rather than *conventional*. Track as a mere-kernel-side todo; revisit when bus-action coverage approaches 100% of the mutation surface (which it largely does today after the relation-taxonomy + edge-mutation work).

## Phase 1 - Taxonomy and doc reconciliation

**Goal:** Make the architecture legible in browser terms before code spreads.

**Work:**

1. Promote the browser taxonomy translation brief as the canonical mapping from browser subsystem vocabulary to Mere vocabulary.
2. Update the spatial chrome IR brief so it links to taxonomy, renderer registry, OS-plumbing audit, and this plan as separate children.
3. Keep old Graphshell terms translated into current Mere terms:
   - Graphshell local workbench -> Mere local workbench/app.
   - Verso/peer session layer -> Murm/murmuring.
   - Verse/community-scale layer -> Moothold/mooting.
4. Update `design_docs/DOC_README.md` with the new doc set and intended read order.

**Done when:**

- The design-doc index lists all four 2026-05-15 artifacts.
- No one document has to answer taxonomy, registry, OS plumbing, and implementation sequencing at once.

**Risks:**

- Over-indexing docs without changing code. This phase is intentionally small and should stop once the references are coherent.

## Phase 2 - Renderer registry v0 under current host

**Goal:** Land the useful abstraction without waiting for substrate-as-host.

**Work:**

1. Choose home:
   - preferred: `crates/mere-host-runtime/src/renderer_registry.rs` initially, because consumers are host/runtime-owned;
   - extract later to `mere-renderer-registry` only if sibling consumers appear.
2. Define:
   - `RendererId`,
   - `NodeContentKind`,
   - `CompositionMode`,
   - `RendererCapabilities`,
   - `NodeRenderer`,
   - `InScenePaintRenderer`,
   - `EmbeddedFrameRenderer`,
   - `OverlayRenderer`,
   - `RendererSelector`.
3. Add adapters:
   - document-tile adapter: `Engine` -> `EngineDocument` -> `platen` -> NetRender/Vello scene,
   - scrying adapter: `SurfaceEngine` / `SurfaceProducer` -> `EmbeddedFrameRenderer`,
   - cartography adapter: graph projection -> in-scene paint,
   - edge adapter: relation/edge labels -> in-scene paint,
   - placeholder overlay adapter for Wry/CEF.
4. Thread diagnostics:
   - `renderer.registered`,
   - `renderer.unregistered`,
   - `engine.route_chosen`,
   - `engine.route_degraded`,
   - `renderer.hot_swapped`,
   - `surface.attach_failed`.

   **Naming convention for long-running operations** (per [graphshell harvest brief](../research/2026-05-17_graphshell_harvest_brief.md) Tier 1 / T1-3): any operation that can hang, fail asynchronously, or have non-trivial latency emits a paired `<op>.started` / `<op>.succeeded` / `<op>.failed` triple with a timeout contract. Watchdog analyzers consume the stream to surface hangs vs. silent failures. The point events listed above stay point events; future async expansions (e.g. `renderer.boot_started` / `renderer.boot_succeeded` / `renderer.boot_failed` for renderers that boot asynchronously like Genet or scrying WebView2; `engine.warmup_*` for engines with cold-start) follow the triple convention. `ChannelRegistry` carries channel descriptors (schema, severity, retention) as declarative config separate from the live analyzers — schema is configuration, analyzers are pluggable.
5. Thread capability gates:
   - per-node route override -> `engine.route_override`,
   - profile escalation -> `engine.profile.escalate`.

**Done when:**

- Host tile dispatch resolves through the registry for at least document tiles and scrying tiles.
- Existing behavior is unchanged for users.
- The old bespoke dispatch path is either removed or clearly marked legacy.
- Unit tests cover selector resolution order and capability-gate denial behavior.

**Risks:**

- Pulling too much GPU detail into portable registry types. Keep GPU handles behind host-side adapters.
- Making `inker` own final composition. It should not; it owns engine output and profile/routing identity.

## Phase 3 - NetRender/Vello composition proof

**Goal:** Prove the spatial chrome substrate can compose real Mere-like surfaces without replacing the host.

**Work:**

Build a small proof harness, not a product host:

1. Root scene with pan/zoom camera.
2. One cartography/graph node.
3. One document tile rendered through document engine -> platen -> NetRender.
4. One embedded-frame placeholder texture using the same external-texture path expected by scrying/Genet.
5. One relation edge with label and hit-test.
6. Basic hit-test returning node/edge/content identity.
7. Thumbnail/capture path for switcher preview.
8. UxTree/AccessKit projection for the same scene.

**Suggested home:**

```text
crates/mere-host-runtime/src/spatial_proof/   (if host-runtime-owned)
or
crates/mere-spatial-prototype/                (if crate isolation is useful)
```

Do not name this final substrate yet. It is a proof.

**Done when:**

- The proof renders a mixed scene without gpui-specific layout concepts.
- Embedded-frame and in-scene paint surfaces share a coordinate/hit-test model.
- The proof can be driven by deterministic test data.
- Windows local validation exists; Linux X11 validation is attempted if practical.
- **Composition-pass ordering invariant on `SubstrateScene`** (per [graphshell harvest brief](../research/2026-05-17_graphshell_harvest_brief.md) Tier 1 / T1-2): the substrate's paint path enforces a strict three-tier layering — *chrome* (host UI surrounding the canvas) paints first, *content* (NodeRenderer dispatches per node, including both in-scene paint and embedded-frame composite) paints second, *overlay* (focus rings, lasso, drag preview, tooltips, edge labels above their endpoints) paints last. The contract is a type-level invariant on `SubstrateScene::paint_scene` (or equivalent), not a convention. Under a free-zoom camera, overlay Z is camera-relative; chrome Z is window-relative — without the strict ordering, overlays Z-fight with content as the camera moves. Currently `mere-spatial-prototype/SubstrateHost::render_scene` is flat (no enforced ordering); this done-condition closes the gap.

**Risks:**

- Accidentally building a parallel app host. Keep the harness narrow.
- Treating visual scene state as graph truth. It is presentation/session state.

## Phase 4 - Native texture interop consolidation

**Goal:** Stop sibling crates from drifting into incompatible native-frame contracts.

**Work:**

Create or designate one low-level interop crate with types equivalent to:

```text
NativeFrame
ImportedTexture
TextureImporter
HostWgpuContext
ExternalTexturePlacement
FrameSync / fence metadata
```

Candidate home:

```text
repos/wgpu-native-texture-interop/        (if kept independent)
repos/wgpu-scry/crates/wgpu-native-texture-interop/
repos/mere/crates/mere-native-texture/    (only if Mere-specific, not preferred)
```

Rules:

1. `wgpu-scry` depends on it for WebView/WebView2/WK/WebKitGTK captured frames.
2. `wgpu-weld` depends on it for CEF accelerated OSR frames.
3. Genet depends on it only at the host-output boundary, not inside core document/layout logic.
4. NetRender consumes imported textures through a stable external-texture API.
5. Mere's renderer registry sees only embedded-frame capability metadata and host texture handles, not platform-specific COM/ObjC/DMABuf details.

**Done when:**

- scrying and welding can describe native frames with the same enum/trait vocabulary.
- NetRender can sample imported textures through one path.
- Platform-specific sync/fence requirements are explicit in type names or metadata.

**Risks:**

- Making the crate too high-level. It should not know about Mere nodes, sessions, profiles, or panes.
- Ignoring macOS/Wayland validation gaps.

## Phase 5 - Browser/PWA envelope plan

**Goal:** Define what Mere-in-a-browser can honestly support.

**Work:**

1. Define `EnvelopeCapabilityProfile` as the single host capability/degradation contract (per [graphshell harvest brief](../research/2026-05-17_graphshell_harvest_brief.md) Tier 1 / T1-4). One product model across hosts; each envelope declares per-capability state: `Full`, `Degraded(reason)`, `Unavailable(reason)`. Initial envelopes:
   - `Envelope::DesktopNative` (full)
   - `Envelope::BrowserWasm` (degraded — see capability matrix below)
   - `Envelope::Headless` (degraded for visual capabilities, full for data/sync)
   - `Envelope::Mobile` (future; constrained for native-WebView differences and resource limits)

   Renderer selectors consult the profile via `RendererCapabilities::supported_in(envelope)`. Composes with the existing per-action [capability gate catalogue](../research/2026-05-14_capability_gate_catalogue_brief.md): **envelope is the *outer* layer** (what is even physically available on this host) — **gate is the *inner* layer** (which gated actions an actor may perform within what's available). Order: envelope filters first; if a capability is `Unavailable`, the gate is moot.

2. Bucket capabilities into the profile by category — supported / degraded / unavailable — for each envelope:
   - native: desktop wgpu backends, native file/profile dirs, native texture import, OS WebView capture.
   - web: WebGPU canvas, OPFS/IndexedDB persistence, browser-safe networking, WebRTC/relay p2p.
3. Require async WebGPU boot for browser targets.
4. Add a small NetRender smoke for browser canvas when the Pelt/NetRender web lane is active.
5. Define p2p transport constraints:
   - raw iroh native,
   - browser-safe relay/WebRTC path optional/future,
   - no claim of full p2p parity in PWA until proven.

**Done when:**

- PWA/browser-host documentation stops implying full native parity.
- Direct Lane/source/graph views are the first-class browser envelope.
- Genet/native WebView/native texture features are explicitly native-only.
- `EnvelopeCapabilityProfile` exists as a typed contract (somewhere in `mere-host-runtime` or a small `mere-envelope` crate) and is consulted by `RendererSelector`, replacing any ad-hoc per-feature "is this host supported" checks.

**Risks:**

- Marketing language outrunning capability.
- Letting browser constraints leak backwards into native architecture.

## Phase 6 - Session/p2p sync boundary

**Goal:** Decide which spatial/session state can replicate and which stays local.

**Work:**

Classify state:

| State kind | Examples | Sync posture |
| --- | --- | --- |
| Semantic graph truth | accepted nodes, typed relations, EntryRecord/content identity | Moothold/Murm durable sync |
| Durable session truth | manifest, panes, frame IDs, selected view intents, branch state | syncable by explicit session policy |
| Engine profile binding | persona/session/graph profile path, renderer compatibility metadata | sync metadata only; not profile bytes |
| Presentation state | camera, transient LOD, hover, active drag, latest frame texture | local by default |
| Live collaboration state | cursor, viewport follow, shared selection | future live-session protocol |

**Done when:**

- The spatial IR persistence question is answered in terms of these buckets.
- Graph/session manifests do not silently absorb ephemeral renderer state.
- P2P docs can refer to this table instead of inventing their own split.

**Risks:**

- Over-replicating presentation state and creating noisy/conflicting session restores.
- Under-replicating session intent and losing the point of collaborative graph work.

## Phase 7 - OS-plumbing proof gate

**Goal:** Decide whether substrate-as-host is worth reopening.

**Work:**

1. Write IME acceptance criteria.
2. Do source-grade gpui IME audit for macOS and Windows.
3. Decide whether `mere-os-plumbing` should land incrementally for cheap subsystems:
   - clipboard,
   - dialogs,
   - theme,
   - gestures,
   - intra-app drag.
4. Establish non-local validation strategy:
   - macOS,
   - Linux Wayland,
   - IME/candidate windows,
   - drag/drop,
   - accessibility smoke.

**Done when:**

- IME is no longer a vague "hard thing"; it has acceptance criteria and platform strategy.
- The non-local validation gap is owned.
- There is a credible answer to "why not stay on gpui?"

**Risks:**

- Underestimating IME polish.
- Treating AccessKit/winit coverage as proof that all OS integration is solved.

## Phase 8 - Substrate-as-host parity demo

**Goal:** Only after phases 2-7, build the Workbench-sized parity demo.

**Work:**

1. Parallel host path:

```text
mere-host/gpui                 (canonical)
mere-host/scene-graph-proof    (experimental)
```

2. Must demonstrate:
   - window open/close/resize,
   - pan/zoom spatial scene,
   - document tile,
   - graph view,
   - embedded web texture,
   - relation edge,
   - focus traversal,
   - clipboard text,
   - IME floor,
   - AccessKit tree,
   - diagnostics,
   - session restore.

3. Compare against gpui:
   - user-visible quality,
   - code ownership,
   - runtime performance,
   - OS coverage,
   - maintenance burden.

**Done when:**

- The demo is usable enough to make an adoption decision.
- The honest-broker comparison is written down.
- Either gpui remains canonical with the substrate as a pane/runtime layer, or the host pivot gets a real migration plan.

**Risks:**

- Building an impressive demo that still misses IME/a11y/session restore.
- Letting the prototype become the mainline host before it earns it.

## Genet changes required

Genet should become a renderer tenant, not a host.

Required direction:

1. Implement or adapt to `EmbeddedFrameRenderer` semantics for full-web pages.
2. Keep the profile ladder:
   - static/simple HTML path where appropriate,
   - interactive/scripting/fullweb path for real web pages,
   - explicit failure/degrade signals for routing.
3. Expose:
   - frame production,
   - input delivery,
   - navigation events,
   - capture/snapshot,
   - profile binding scope,
   - accessibility boundary.
4. Keep WebGL-over-wgpu output compatible with the shared native texture/external texture path.
5. Do not internalise scrying/Wry as compatibility paths. Those are peer renderers selected by Mere.

Pitfall: using Genet as a toolkit or replacing Mere's host/session model with Genet internals. Genet is the web content engine, not the workbench authority.

## NetRender changes required

NetRender should become the shared scene/composition substrate for in-scene content and external textures.

Required direction:

1. Stabilise a `SceneFragment`/layer-style API that Mere renderers can contribute to.
2. Keep external texture composition first-class.
3. Make hit-test/capture/replay deterministic enough for substrate proof tests.
4. Keep Parley text integration as a helper path, not a separate renderer authority.
5. Support async device/surface boot for browser/web targets.
6. Keep wasm/web build concerns separate from native desktop backend concerns.

Pitfall: making NetRender the app architecture. NetRender is rendering/composition infrastructure; Mere still owns session, graph truth, renderer selection, and actions.

## wgpu-scry changes required

`wgpu-scry` should be the system WebView/WebView2 capture/import renderer primitive.

Required direction:

1. Align `WebSurfaceFrame` / producer output with the shared native texture interop crate.
2. Keep `WebSurfaceProducer` policy-free: no Mere session semantics inside scrying.
3. Expose capability metadata:
   - native texture,
   - CPU fallback,
   - overlay-only fallback,
   - input/IME support,
   - snapshot support,
   - platform backend.
4. Keep Windows-first proof strong while documenting macOS/Wayland gaps.

Pitfall: turning scrying into "the web engine." It is an embedded-frame system-WebView renderer, one peer among several.

## wgpu-weld changes required

`wgpu-weld` should be optional CEF accelerated OSR support, not the default web path.

Required direction:

1. Align `NativeFrame` / importer contracts with the shared interop crate.
2. Register conceptually as `cef.web` or `chromium.web` if adopted.
3. Preserve CEF subprocess/security/profile costs in capability metadata.
4. Treat it as high-compat fallback or specific renderer choice, not as the core browser architecture.

Pitfall: CEF can solve compatibility fast while importing Chromium's weight. Keep the cost visible.

## Dependency shape

Target direction:

```text
mere-kernel
  -> relation taxonomy / cartography model / Eidetic IDs

inker / nematic
  -> engine routing, protocol-faithful document production

platen / netrender adapters
  -> document layout and scene production

renderer registry adapters
  -> content-kind dispatch and composition-mode selection

mere-host-runtime
  -> sessions, panes, action bus, workers, diagnostics, renderer lifecycle

host backends
  -> gpui canonical today
  -> scene-graph/substrate proof later

optional renderer providers
  -> Genet
  -> wgpu-scry
  -> wgpu-weld / CEF
  -> Wry overlay
```

Forbidden direction:

```text
mere-kernel -> host/backend/rendering crates
cartography -> gpui
inker -> gpui or final compositor
Genet -> Mere host/session authority
wgpu-scry -> Mere policy/session authority
wgpu-weld -> Mere policy/session authority
```

## Sidequests

Useful but not on the critical path:

1. WebExtension compatibility envelope.
2. DevTools/page inspector story for Genet and system WebView renderers.
3. `about:`-style diagnostics pages or Mere-native equivalent.
4. Download manager and permission UI aggregation.
5. Cross-renderer visual effects.
6. Overlay thumbnail capture.
7. Native notifications.
8. Substrate naming.
9. Source-grade gpui audits for non-IME subsystems if substrate proof gets serious.

## Pitfalls

1. Replacing gpui before renderer registry and OS proof gates land.
2. Letting the spatial IR become semantic graph truth.
3. Pretending overlay renderers can participate in deep hit-test, capture, clipping, and relation anchoring like in-scene renderers.
4. Treating static HTML as a universal fallback for full web content.
5. Chasing WebExtension parity before Mere's native mod/capability surface exists.
6. Claiming PWA parity before WebGPU, persistence, transport, and native-feature degradation are explicit.
7. Duplicating native-frame/importer contracts across scrying, weld, Genet, and NetRender.
8. Letting native `pollster`/desktop wgpu backend assumptions leak into browser/wasm targets.
9. Overfitting to Windows validation and forgetting macOS/Wayland are the substrate-host risk multipliers.

## Near-term execution order

1. Finish doc reconciliation (this plan, taxonomy brief, index updates).
2. Turn renderer registry contract into an adoption issue/plan with exact crate home.
3. Land registry v0 for document tiles + scrying under current host.
4. Start shared native texture interop consolidation across `wgpu-scry` and `wgpu-weld`.
5. Build NetRender mixed-scene proof.
6. Define browser/PWA envelope constraints.
7. Define session/p2p sync-state buckets.
8. Only then reopen substrate-as-host.

## Non-goals

- No immediate gpui removal.
- No immediate WebExtension implementation.
- No claim that PWA/browser-hosted Mere equals native Mere.
- No requirement that Genet, scrying, Wry, and CEF all ship before the registry is useful.
- No process-sandbox/multiprocess commitment for v1.

## Decisions

1. **Renderer registry adoption is the first implementation move.**
2. **Substrate-as-host remains gated by renderer maturity, OS-plumbing proof, and parity demo.**
3. **NetRender proof is a composition proof, not a host migration.**
4. **Genet is a renderer tenant, not the workbench authority.**
5. **wgpu-scry and wgpu-weld must converge on a shared native texture interop contract before both are treated as first-class Mere renderer providers.**
6. **PWA/browser-hosted Mere is explicitly degraded until proven otherwise.**
7. **P2P sync state must be bucketed before spatial persistence lands.**
