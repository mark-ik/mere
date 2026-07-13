# Browser Taxonomy Translation — research brief

**Date**: 2026-05-15
**Status**: Research brief (taxonomy alignment; no implementation commitment)
**Scope**: Translates the [spatial chrome IR brief](2026-05-15_spatial_chrome_ir_brief.md), [renderer registry contract](2026-05-15_renderer_registry_contract_brief.md), and current Mere multiplexer/session work into ordinary browser subsystem language. Uses Firefox-like taxonomy as the comparison target, not as an implementation template.

**External reference points**:

- [Firefox Process Model](https://firefox-source-docs.mozilla.org/dom/ipc/process_model.html) — parent process, content processes, extension process, helper processes.
- [Firefox Rendering Overview](https://firefox-source-docs.mozilla.org/gfx/RenderingOverview.html) — DOM/layout/display-list/WebRender scene/compositor split, including spatial/picture tree concepts.
- [Firefox WebExtensions API Development](https://firefox-source-docs.mozilla.org/toolkit/components/extensions/webextensions/index.html) — extension API implementation surface.
- [Firefox WebExtensions background](https://firefox-source-docs.mozilla.org/toolkit/components/extensions/webextensions/background.html) — sandboxed extension code with privileged browser-side API implementations.

**Related Mere / Graphshell docs**:

- [`2026-05-15_spatial_chrome_ir_brief.md`](2026-05-15_spatial_chrome_ir_brief.md)
- [`2026-05-15_renderer_registry_contract_brief.md`](2026-05-15_renderer_registry_contract_brief.md)
- [`2026-05-15_os_plumbing_reuse_audit_brief.md`](2026-05-15_os_plumbing_reuse_audit_brief.md)
- [`2026-05-11_browser_multiplexer_framing.md`](2026-05-11_browser_multiplexer_framing.md)
- [`../implementation_strategy/2026-05-11_graph_session_manifest_plan.md`](../implementation_strategy/2026-05-11_graph_session_manifest_plan.md)
- [`../implementation_strategy/2026-05-14_session_service_runner_plan.md`](../implementation_strategy/2026-05-14_session_service_runner_plan.md)
- [`../implementation_strategy/2026-05-14_engine_profile_boundary_plan.md`](../implementation_strategy/2026-05-14_engine_profile_boundary_plan.md)
- [`2026-05-10_cartography_layer_brief.md`](2026-05-10_cartography_layer_brief.md)
- [`../implementation_strategy/2026-05-11_relation_taxonomy_and_edge_mutation_plan.md`](../implementation_strategy/2026-05-11_relation_taxonomy_and_edge_mutation_plan.md)
- Graphshell donor docs: `design_docs/graphshell_docs/research/2026-04-22_browser_subsystem_taxonomy_and_mapping.md`, `design_docs/graphshell_docs/implementation_strategy/2026-04-16_middlenet_lane_architecture_spec.md`, and the smolweb/Scroll alignment notes.

---

## Thesis

> **Mere is not a conventional browser frontend with a graph feature bolted on. It is a browser/workbench shell whose chrome/session model is spatial and relational. The browser taxonomy still applies, but several rows translate differently: tabs become addressable scene nodes, content processes become renderer tenants, WebExtensions become capability-gated native/WASM mods, and sync becomes Murm/Moothold graph/session replication.**

The value of the Firefox-like taxonomy is not imitation. It gives Mere a checklist for what a browser-shaped product eventually needs: process/isolation boundaries, rendering/composition, navigation/session restore, storage/profile state, permissions, extension/mod API, accessibility, diagnostics, update/distribution, and sync.

The refactor this implies:

1. Keep **browser subsystem taxonomy** separate from **spatial chrome IR**.
2. Keep **spatial chrome IR** separate from **semantic graph truth**.
3. Keep **renderer registry** separate from **engine implementation**.
4. Keep **native host adoption** separate from **renderer registry adoption**.

## 1. Taxonomy table

| Conventional browser subsystem | Firefox-like shape | Mere translation | Current posture |
| --- | --- | --- | --- |
| Browser parent / chrome | Parent process owns browser UI, privileged chrome, process management, helper orchestration | `graphshell/shell/session-runtime`, `graphshell/shell/system/control-plane`, capability gates, session manifest, window/runtime host | Partially planned; gpui host canonical today |
| Tabs / windows / browser frontend | Tabs and browser chrome around web documents | `GraphSessionManifest`, panes, `ViewIntent`, spatial scene nodes, multi-window projections | Session manifest and view-intent seams exist; spatial IR is research |
| Content process | Web content loaded in content processes, with origin/process isolation | Renderer tenants: `genet.web`, `scrying.web`, `wry.web`, Nematic document engines, panels | Renderer taxonomy clarified; registry not adopted |
| Layout/document engine | DOM/CSS/layout builds display lists/scenes | Genet for full web; Nematic/Platen/NetRender for protocol-faithful documents; Cartography for graph views | Multiple lanes exist; unified dispatch pending |
| Rendering / compositor | Display list -> WebRender scene/frame -> GPU/compositor | NetRender/Vello scene fragments plus external texture composition | NetRender exists; substrate proof pending |
| GPU/helper processes | GPU process, network/socket process, RDD/utility/helper processes | Future helper/session workers behind `SessionServiceRunner`; optional process split later | Single-process logical daemon for v1 |
| Navigation/history | URL navigation, Places, session restore, tab history | Graph/session manifests, frame/pane identity, view-intent sidecars, Eidetic durable memory | Manifest/view-intent work is the right spine |
| Storage/profile | Profile dir, cookies, cache, IndexedDB, permissions | `EngineProfileBinding` resolves persona/session/graph UDFs per renderer | Resolver exists; per-engine wiring incomplete |
| Permissions/security | Origin model, sandbox, site isolation, permission prompts | Capability gates plus engine-owned origin/security model; sandbox/multiprocess later | Gates are catalogued; OS/process sandbox deferred |
| Extensions | WebExtension process, schemas, content scripts, privileged browser APIs | Native/WASM mod/action API first; WebExtension compatibility only as a later envelope | Do not chase WebExtension parity as v1 |
| PWAs / service workers | Web app installability and background/service-worker runtime | Browser-host/PWA envelope for degraded Direct Lane; native worker/session model via `SessionServiceRunner` | Needs explicit envelope constraints |
| Accessibility | Browser/engine accessibility trees stitched through OS APIs | `uxtree` + AccessKit for chrome; renderer boundary stitching for content | Strong direction; host proof pending |
| DevTools/diagnostics | Page devtools, browser console, about pages, telemetry | Apparatus diagnostics, renderer route diagnostics, future page/engine inspector | Diagnostics vocabulary is forming |
| Sync/account | Browser-account sync of bookmarks/history/settings | Murm/Moothold p2p/federated graph/session sync, no central account assumption | Architectural direction; sync protocol separate |

## 2. What Firefox terms should and should not mean here

### 2.1 "Parent process"

Firefox's parent process is a privileged owner of browser chrome, child processes, and helper process orchestration. In Mere, the analogous owner is **not** a process at v1. It is the host/runtime authority around:

- session manifests,
- action bus dispatch,
- capability gates,
- renderer registry selection,
- window/surface lifecycle,
- diagnostics,
- background/session workers.

The [daemon split brief](2026-05-14_daemon_split_research_brief.md) already made the correct call: v1 can stay single-process while preserving a daemon-shaped seam.

### 2.2 "Content process"

Mere should avoid pretending every content surface is a web content process. A web page, a Scroll document, a graph view, a knot editor, and a panel are all **node content**, but they do not have the same engine semantics.

Renderer tenants are the right abstraction:

- `genet.web` — full web, JS/browser-API-heavy content.
- `scrying.web` — system WebView capture/import path.
- `wry.web` — overlay-only system WebView fallback.
- `nematic.*` — protocol-faithful smolweb/document engines.
- `cartography.graph` — graph projection renderer.
- `mere-domain.panel` — native Mere panels.
- `chrome.edge` — relation/edge renderer.

This is where the renderer registry brief should become authoritative.

### 2.3 "WebRender / compositor"

Firefox's rendering overview is useful because it separates document/layout work from GPU/compositor work and because WebRender itself splits picture/drawable structure from spatial transforms.

Mere's spatial chrome IR is **not** WebRender's spatial tree. The shared insight is narrower:

- transforms and placement deserve first-class representation,
- visible output should compile into a GPU-friendly scene/frame,
- hit-testing and async movement benefit from spatial metadata,
- display/composition data should not be confused with source document truth.

The Mere version is a workbench/chrome scene graph above heterogeneous renderers, not a CSS page-internal spatial tree.

### 2.4 "Extension process"

Firefox treats WebExtensions as sandboxed extension code with privileged browser-side API implementations. Mere should borrow that **shape** without inheriting the API target too early.

Mere's primary extension model should be:

- a manifest/schema for mods,
- capability-gated actions,
- declared renderer/content-kind hooks,
- optional background/session workers,
- diagnostics,
- install/enable/disable lifecycle,
- no implicit access to semantic graph truth or engine profile bytes.

WebExtension compatibility can be a later bridge for browser-ecosystem affordances. It should not be the primary mod substrate because Mere's privileged surface is graph/session/spatial, not tab-and-DOM only.

## 3. Impact on product goals

### 3.1 Browser embeddable goal

The spatial IR improves the embeddable story if the registry lands first.

The key shift: "embed a browser" becomes "register a `WebPage` renderer with one of three composition modes." Genet, scrying, Wry, and optional CEF/wgpu-weld are then choices behind the same content-kind boundary, not separate host strategies.

Do not make substrate-as-host a dependency of this goal. The embeddable browser path can advance under gpui as long as the renderer contract is host-agnostic.

### 3.2 Extension/mod goal

The taxonomy argues against a WebExtension-first plan.

Mere's first extension surface should be a native/WASM mod surface over:

- action bus commands,
- content-kind renderer registration,
- graph/session queries with capability gates,
- panel registration,
- worker registration,
- diagnostics/events.

WebExtension compatibility is useful later for ecosystem access, but it is a compatibility envelope, not the source of truth.

### 3.3 PWA/browser-hosted goal

Browser-hosted Mere is a constrained envelope, not the full native product.

Likely supported in browser/PWA mode:

- Direct Lane document rendering,
- source/graph views,
- browser-safe persistence,
- browser WebGPU where available,
- WebRTC/relay-mediated p2p if proven.

Likely unavailable or degraded:

- native Genet,
- native texture import,
- OS WebView capture,
- raw iroh assumptions,
- native process/helper orchestration,
- unrestricted filesystem/profile directories.

This is not a failure. It means the PWA is a collaboration/reading/editing envelope over the same durable graph/session concepts, not a full native shell replacement.

### 3.4 P2P goal

The spatial IR helps p2p because it gives visible state addressable structure. It is also dangerous because it tempts the system to replicate presentation state as graph truth.

Split the state:

- **Durable graph truth**: semantic nodes, relations, content identities, contribution records, accepted manifests.
- **Durable session truth**: selected frames/panes, view intents, branch state, engine profile bindings when appropriate.
- **Ephemeral presentation**: camera position, transient LOD decisions, hover/focus, active drag, latest embedded-frame texture, animation state.

Murm/Moothold should sync the first two categories selectively. The third category is local unless explicitly promoted into a shared live-session protocol.

### 3.5 Smolweb goal

The spatial IR strengthens the smolweb lane if it preserves protocol-faithful documents.

Routing rule:

- Gemini/Gopher/Scroll/Markdown/feeds/plaintext -> Nematic Direct Lane.
- Static/simple HTML -> Nematic HTML Lane when that lane exists.
- JS/browser-API-heavy web apps -> Genet.
- Emergency/system fallback -> scrying or Wry/CEF, depending on composition needs.

Do not flatten Scroll or other smolweb formats into HTML just to reuse a browser-shaped renderer. Scroll remains source-faithful document truth first, visual projection second.

## 4. Refactor required in the current framing

The spatial chrome IR brief should remain the shape/framing parent, but the durable doc set needs these separations:

1. **Taxonomy** lives here: "how Mere maps to browser subsystems."
2. **Spatial IR** lives in the parent brief: "what shape chrome/session presentation wants."
3. **Renderer registry** lives in its contract brief: "how content kinds pick renderers."
4. **OS plumbing** lives in its audit: "what substrate-as-host would cost."
5. **Adoption sequencing** lives in the implementation plan: "what lands first, with done conditions."

Without that split, the spatial brief has to answer too many questions and will drift into a mega-doc.

## 5. Dependency implications

Core dependency direction should stay:

```text
mere-kernel / eidetic / relation taxonomy
    -> cartography / nematic / inker
    -> platen / uxtree
    -> renderer-registry adapters
    -> graphshell shell session/control-plane crates
    -> host backends (gpui today; substrate prototype later)
```

Rules:

1. `mere-kernel` must not depend on gpui, wgpu, Genet, scrying, Wry, CEF, NetRender, or host crates.
2. `inker` owns engine identity and engine/profile routing, not final composition.
3. `platen`/NetRender are document/layout/render adapters, not graph truth.
4. Renderer-registry types should live at the host/runtime boundary, or in a tiny host-facing crate with portable IDs/types plus feature-gated GPU handles.
5. scrying/wgpu-weld/Genet are optional renderer providers, never core dependencies.
6. Native texture interop should be factored once and shared by scrying, wgpu-weld, Genet, and NetRender.
7. Browser/PWA builds get their own async WebGPU/OPFS/WebRTC envelope; do not let native `pollster`/desktop backend assumptions leak into it.

## 6. Decisions

1. **Firefox-like taxonomy is a checklist, not a target architecture.**
2. **Mere's browser chrome authority is session/spatial/action authority, not DOM-chrome authority.**
3. **Renderer tenants are the content-process analogue.**
4. **NetRender/Vello is the compositor analogue only after renderer registry boundaries are clean.**
5. **Native/WASM mods are the primary extension model; WebExtensions are compatibility later.**
6. **PWA/browser-hosted Mere is a constrained envelope and must be documented as such.**
7. **P2P sync must distinguish semantic graph truth, durable session truth, and ephemeral presentation state.**

## 7. Open questions

1. Should the renderer registry stay under `graphshell/shell/system/registry` or split into a tiny host-facing crate with portable IDs/types? Lean: shell/system boundary, not `mere-kernel`.
2. What is the first acceptable WebExtension compatibility story: none, read-only adapter, or a privileged compatibility renderer?
3. How much of `GraphSessionManifest` should be syncable by default versus local-only?
4. Does CEF/wgpu-weld become a first-class `chromium.web` renderer or remain an experiment until scrying/Genet gaps force it?
5. What diagnostics UI replaces `about:`-style browser internals for Mere?

## 8. Implied follow-up

The companion implementation plan is [`../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md`](../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md).
