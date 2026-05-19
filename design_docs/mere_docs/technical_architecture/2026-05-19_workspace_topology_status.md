# Workspace Topology Status — 2026-05-19

**Date**: 2026-05-19
**Status**: Snapshot after the B1–B7 supercrate naming pass + vestigial cleanup.
**Companion to**: [`../research/2026-05-15_browser_taxonomy_translation_brief.md`](../research/2026-05-15_browser_taxonomy_translation_brief.md) (taxonomy-translation framing), [`../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md`](../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md) (adoption sequence).

Earlier doc-level snapshots (e.g. the topology table in `DOC_README.md`) predate this pass and still cite a `crates/workbench/` umbrella that the rename dissolved. This file is the current source of truth for the workspace shape; the index has been updated to point here.

---

## 1. The supercrate topology

The workspace is organized into **semantic supercrate directories** under `crates/`. Each supercrate owns a single concern; sub-crates beneath it are real workspace members that can be built and tested in isolation. The directory path disambiguates, so crate leaf names dropped their `mere-*` / role prefixes in batches B1–B7.

```text
crates/
├── mere/                          # The product binary
│   ├── host/                       — winit + wgpu + vello host; orchestrates everything below
│   └── host-substrate/             — substrate ↔ runtime bridge (HostApp, scene sync, dispatch)
│
├── graphshell/                    # Graph + Shell — the chrome layer
│   ├── graph/                      — data + spatial substrate
│   │   ├── aether/                  — top-level paint primitives
│   │   ├── cartography/             — ProjectionRequest / ViewIntent / Projection
│   │   ├── graph-canvas/            — canvas scene IR (CanvasSceneInput, FrameRegion)
│   │   ├── graph-kernel/            — `kernel`: Graph + geometry + identity
│   │   ├── graph-layout/            — LayoutStrategy adapters (grid, force-directed, …)
│   │   ├── node-lineage/            — owner-scoped navigation lineage (url→url, node→node)
│   │   ├── orrery/                  — tiered-graph framework primitives
│   │   └── spatial-substrate/       — substrate scene + dispatch + camera
│   │
│   └── shell/                      — chrome view-models + runtime + registries
│       ├── domain/
│       │   ├── chrome/              — toolbar / omnibar / palette / authorities view-models
│       │   └── frame/               — FrameLayout + PaneContent
│       ├── host-ports/              — host-port trait vocabulary
│       ├── session-runtime/         — session graph store, manifest store, view-intent store,
│       │                                engine-profile store, switcher thumbnails, service runner
│       ├── state/                   — shell-state: re-exports chrome modules + ux probes
│       ├── system/
│       │   ├── control-plane/       — control-plane action bus
│       │   └── registry/
│       │       ├── register-diagnostics/
│       │       ├── register-renderer/        — renderer-registry trait + dispatch
│       │       └── register-renderer-types/  — wasm32-clean data-only types
│       └── ux-events/               — UX event vocabulary
│
├── inker/                         # Engines (per-node content production)
│   ├── document-canvas/            — document layout helper
│   ├── engines/
│   │   ├── nematic/                 — smolweb / Scroll / Gemini / markdown
│   │   └── scrying-engine/          — system-WebView producer (WebView2 / WKWebView / WPE)
│   └── inker/                       — root: SurfaceEngine / SurfaceProducer registry
│
├── forme/                         # Arrangement (per-graph-view workbench authority)
│   ├── forme/                       — FrameLayout / PaneBinding mutators + selectors
│   └── uxtree/                      — UxTree (AccessKit-native chrome tree)
│
├── platen/                        # Composition (graph→canvas + EngineDocument→RenderPacket)
│   ├── domain/
│   │   ├── apparatus/               — system inspector panel view-model
│   │   ├── gloss/                   — content-strip panel view-model
│   │   └── workbench/               — workbench panel view-model
│   └── platen/                      — canvas_scene + document_scene + workbench projections
│
├── verso/                         # Tile surfaces (the receptor)
│   ├── scrying-renderer/           — EmbeddedFrameRenderer for scrying producers
│   ├── tile-state/                 — per-tile state
│   └── verso-core/                 — tile surface vocabulary, SurfaceCommand, lifecycle
│
├── eidetic/                       # Memory layer (durable artifacts + fetchers)
│   ├── eidetic-core/               — content-addressed engrams, schema-typed manifests
│   ├── eidetic-fjall/              — fjall KV backend
│   ├── eidetic-https-fetcher/      — HTTPS fetcher
│   └── eidetic-iroh-fetcher/       — iroh fetcher
│
├── intel/                         # Intelligence (embeddings, semantic indexes)
│   └── embed/                      — embeddings + vector index + canvas search
│
├── murm/                          # Bilateral comms (1:1 / N:N over iroh streams)
│   ├── murm/                       — cabal abstraction
│   ├── murmuring/                  — cable engine + persistent store
│   └── transport/                  — iroh transport + memory transport
│
├── moot/                          # Federation (community/coalition)
│   ├── moothold/                   — t3 federation
│   └── mooting/                    — t2 themed-graph community
│
└── persona/                       # Identity (one human, many personas)
    └── identity/                    — keypair + provider + persona manifest
```

The Mere binary at `crates/mere/host/` is the only entry point; everything else is a library crate. Two crates sit outside the workspace `members` list and are reachable only as explicit path-deps:

- `crates/probes/` — sketch crates that adapt to upstream skew, never workspace-pinned.
- `crates/verso/masonry-renderer/` — the xilem-masonry adapter; its build matrix doesn't share the workspace pins yet.

## 2. Browser anatomy — functional groups

Mainstream browsers (Firefox, Chromium) ship roughly twelve functional groups. Mere covers some of them, replaces others with spatial-relational analogues, and skips a few. The taxonomy-translation brief from 2026-05-15 covers the *concepts*; this table maps them onto *current crate paths*.

| Browser functional group | Conventional shape | Mere equivalent | Crate(s) |
|---|---|---|---|
| **Browser chrome / UI** | Title bar, menus, address bar, sidebar, settings | Toolbar + omnibar + command palette + authorities view-models (rendered via xilem-masonry once the panel renderer lands) | `graphshell/shell/domain/chrome`, `forme/uxtree` |
| **Tab management** | Tab strip, tab switcher, session restore | **Replaced** by spatial graph canvas — nodes are tiles, edges are relations, tabs are not a primary concept; switcher = graph thumbnails | `graphshell/graph/{kernel,canvas,cartography,orrery}`, `forme/forme`, session-runtime's `switcher_thumbnail` |
| **Web engine** | DOM/CSS/layout/JS engine | **Multi-tenant** — engines coexist via inker registry: `scrying.web` (system WebView), `nematic.*` (smolweb), `serval.web` (full web, external) | `inker/engines/{scrying-engine,nematic}`, `crates/verso/masonry-renderer` (excluded), serval (external) |
| **Rendering / compositor** | Display list → compositor scene → GPU | Renderer-registry contract; three composition modes (InScenePaint / EmbeddedFrame / Overlay). vello + wgpu under everything. | `graphshell/shell/system/registry/register-renderer{,-types}`, `verso/{verso-core,scrying-renderer}`, `platen/platen` |
| **Process model** | Parent + content + GPU + network processes | **Single-process logical daemon** at v1; SessionServiceRunner reserves the seam for later split | `graphshell/shell/session-runtime::session_service_runner` |
| **Networking** | HTTP, fetch, cookies, cache, TLS, DNS | Two fetchers + a peer transport; cookies/cache fold into engine profile bytes | `eidetic/{eidetic-https-fetcher,eidetic-iroh-fetcher}`, `murm/transport` |
| **Storage / profile** | Profile dir, cookies, IndexedDB, history | **Eidetic** unifies KV + history + content-addressed engrams; engine profile bytes (cookies etc.) live behind `EngineProfileBinding` | `eidetic/*`, `graphshell/shell/session-runtime::engine_profile_store` |
| **Identity / sync / accounts** | Browser account, cloud sync | **Replaced** — persona is first-class, sync is murm/moot federation, no central account | `persona/identity`, `murm/*`, `moot/*` |
| **Extensions / mods** | WebExtensions API | Native/WASM mod surface via action bus + content-kind hooks; WebExtension compat deferred | (capability gates in `graphshell/shell/system/control-plane`; mod manifest not yet a crate) |
| **Security / sandbox** | Origin model, site isolation, sandbox | Capability gates (4-layer chain: action → session → persona → app); OS sandbox deferred | `graphshell/shell/system/control-plane` (gate chain), `graphshell/shell/session-runtime::engine_profile_store` (isolation scopes) |
| **DevTools / diagnostics** | Page devtools, browser console, telemetry | Apparatus panel + diagnostic events (`engine.route_chosen` / `permission.denied` / etc.) | `platen/domain/apparatus`, `graphshell/shell/system/registry/register-diagnostics`, `graphshell/shell/ux-events` |
| **Accessibility** | AT-SPI / IAccessible2 / NSAccessibility bridges | AccessKit via uxtree; renderer-boundary stitching for embedded content | `forme/uxtree`, `graphshell/graph/spatial-substrate::accessibility` |
| **History / bookmarks** | Places DB | **Two layers**: node-lineage (within-tile + node→node) and eidetic (durable engrams) | `graphshell/graph/node-lineage`, `eidetic/eidetic-core` |
| **Downloads** | Download manager | — (not yet a concern) | — |
| **Intelligence / search** | Awesomebar suggestions, history search | First-class intelligence layer — embeddings + vector index + semantic search | `intel/embed` |

## 3. What's load-bearing vs aspirational

**Real and consumed today (in the host's compile graph):**

- `kernel`, `spatial-substrate`, `graph-canvas`, `graph-layout`, `cartography` (the graph half)
- `frame`, `host-ports`, `session-runtime`, `shell-state`, `control-plane`, `register-renderer{,-types}`, `ux-events`, `chrome` (the shell half)
- `inker`, `nematic`, `scrying-engine` (engines)
- `forme`, `uxtree` (arrangement)
- `platen`, `apparatus`, `gloss`, `workbench` (composition)
- `verso-core`, `tile-state`, `scrying-renderer` (tile surfaces)
- `eidetic` + fjall + the two fetchers (memory)
- `embed` (intelligence)
- `identity` (persona)
- `murm`, `murmuring`, `transport` (messaging)
- `moothold`, `mooting` (federation)
- `host`, `host-substrate` (the binary)

**Crates that exist but are not yet wired into the host:**

- `orrery` (tier framework primitives; renderer chain consumes via `OrreryRenderer` in `mere/host`)
- `node-lineage` (lineage records exist in `kernel`; lineage-view UI not yet wired)
- `aether` (paint primitives — referenced indirectly through `graph-canvas`)

**Aspirational holes (concepts named in docs, no crate yet):**

- Mod manifest / extension surface (the capability-gate chain is in place; mods aren't)
- Process-split daemon (SessionServiceRunner reserves the seam; no remote runner)
- macOS IME hardening (audit identified this as the single subsystem justifying gpui — Mere now on xilem, deferred)
- Downloads / permissions UI / settings UI surfaces (chrome view-models are there, the UI isn't)

## 4. What changed in the 2026-05-19 pass

Eight commits landed today:

- `32e7207` reorganized the workspace into the supercrate topology (Phase 1; the directory moves).
- `b097db7` (B1) dropped `mere-*` from `graph/` leaf names: `mere-kernel` → `kernel`, `mere-spatial-prototype` → `spatial-substrate`, `mere-orrery` → `orrery`.
- `08b2293` (B2) dropped prefixes across `shell/`: `mere-host-runtime` → `session-runtime`, `mere-host-contract` → `host-ports`, `mere-renderer-registry{,-types}` → `register-renderer{,-types}`, `mere-ux-events` → `ux-events`, `mere-frame` → `frame`, `mere-graphshell` → `chrome`, `graphshell-shell-state` → `shell-state`, `graphshell-control-plane` → `control-plane`.
- `3a762a9` (B3) `scrying-tile-engine` → `scrying-engine`.
- `07132bf` (B4) platen + verso: `mere-{apparatus,gloss,workbench}` → bare names; `verso-tile` → `verso-core`; `verso-tile-state` → `tile-state`; `scrying-embedded-renderer` → `scrying-renderer`.
- `983a443` (B5) persona + intel + murm: `mere-identity` → `identity`; `intelligence-embeddings` → `embed`; `mere-transport` → `transport`.
- `54f7929` (B6) mere bin: `mere-host` → `host`; `mere-host-substrate` → `host-substrate`.
- `10d1381` (B7) deleted the vestigial top-level `graphshell` facade crate (`crates/graphshell/{Cargo.toml,src/}`) and refreshed the `chrome` crate's doc strings.

The follow-up commit (today) covered the type-level fallout: `MereHostApp` → `HostApp`, and a bulk-update of doc strings in source files (lib.rs `//!` headers, Cargo.toml descriptions, READMEs) that still mentioned the renamed crates by their old names. `design_docs/` was left alone — historical snapshots read correctly against the names that existed when they were written.

## 5. Open questions

1. **Mod surface as a crate?** Currently spec'd against the action bus + capability gates but no `mods/` supercrate. If the manifest/schema lands as a Tier-1 concern, it earns its own directory.
2. **Should `aether` graduate?** Its paint primitives are reachable only via `graph-canvas` re-exports. If nothing else consumes it directly, it could fold.
3. **`scrying-renderer` placement** — currently at `verso/scrying-renderer/` (it's a renderer for the verso surface layer), while `scrying-engine` is at `inker/engines/scrying-engine/` (it's the content-producer half). The split is correct in role but easy to confuse; the crate-name pass kept both names.
4. **`orrery` consumers** — the orrery renderer in `mere/host` is the only one. If the tier framework grows host-side dashboards, those move into `orrery` proper instead of `host/src/orrery_renderer.rs`.
5. **File-size ceiling check** — the 600-LOC rule lives in workspace memory; after the rename pass, no file in `mere/host/` is over the line but several substrate / kernel files are close. Worth a sweep when next touched.

## 6. Pointers

- Browser-taxonomy framing: [`../research/2026-05-15_browser_taxonomy_translation_brief.md`](../research/2026-05-15_browser_taxonomy_translation_brief.md)
- Renderer-registry contract: [`../research/2026-05-15_renderer_registry_contract_brief.md`](../research/2026-05-15_renderer_registry_contract_brief.md)
- Spatial chrome IR: [`../research/2026-05-15_spatial_chrome_ir_brief.md`](../research/2026-05-15_spatial_chrome_ir_brief.md)
- Adoption sequencing: [`../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md`](../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md)
- Per-supercrate READMEs: each `crates/<supercrate>/README.md` describes its sub-tree.
