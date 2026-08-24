# Workspace Topology Status — 2026-05-19

**Date**: 2026-05-19
**Status**: Snapshot after the B1–B7 supercrate naming pass + vestigial cleanup. **Latest:** §7 (2026-06-07) records the `graphshell/` supercrate dissolution into the `graph` / `orrery` / `shell` / `system` clusters; §8 (2026-06-07) records the `canvas-ir` + `graph-layout` review → `orrery/arrangements` + `gyre::barnes_hut`. §§1–5 predate the genet-as-host flip and are stale (see §7's staleness flag).
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
| **Web engine** | DOM/CSS/layout/JS engine | **Multi-tenant** — engines coexist via inker registry: `scrying.web` (system WebView), `nematic.*` (smolweb), `genet.web` (full web, external) | `inker/engines/{scrying-engine,nematic}`, `crates/verso/masonry-renderer` (excluded), genet (external) |
| **Rendering / compositor** | Display list → compositor scene → GPU | Renderer-registry contract; three composition modes (InScenePaint / EmbeddedFrame / Overlay). vello + wgpu under everything. | `graphshell/shell/system/registry/register-renderer{,-types}`, `verso/{verso-core,scrying-renderer}`, `platen/platen` |
| **Process model** | Parent + content + GPU + network processes | **Single-process logical daemon** at v1; SessionServiceRunner reserves the seam for later split | `graphshell/shell/session-runtime::session_service_runner` |
| **Networking** | HTTP, fetch, cookies, cache, TLS, DNS | Two fetchers + a peer transport; cookies/cache fold into engine profile bytes | `eidetic/{eidetic-https-fetcher,eidetic-iroh-fetcher}`, `murm/transport` |
| **Storage / profile** | Profile dir, cookies, IndexedDB, history | **Eidetic** unifies KV + history + content-addressed engrams; engine profile bytes (cookies etc.) live behind `EngineProfileBinding` | `eidetic/*`, `graphshell/shell/session-runtime::engine_profile_store` |
| **Identity / sync / accounts** | Browser account, cloud sync | **Replaced** — persona is first-class, sync is murm/moot federation, no central account | `persona/identity`, `murm/*`, `moot/*` |
| **Extensions / mods** | WebExtensions API | Native/WASM mod surface via action bus + content-kind hooks; WebExtension compat deferred | (capability gates in `graphshell/shell/system/control-plane`; mod manifest not yet a crate) |
| **Security / sandbox** | Origin model, site isolation, sandbox | Capability gates (4-layer chain: action → session → persona → app); OS sandbox deferred | `graphshell/shell/system/control-plane` (gate chain), `graphshell/shell/session-runtime::engine_profile_store` (isolation scopes) |
| **DevTools / diagnostics** | Page devtools, browser console, telemetry | Apparatus panel + diagnostic events (`engine.route_chosen` / `permission.denied` / etc.) | `domain/apparatus`, `graphshell/shell/system/registry/register-diagnostics`, `graphshell/shell/ux-events` |
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
- Current cluster layout: §7 below (the former `graphshell/` README's role); the
  root `Cargo.toml` is the authoritative member list.

## 7. Update — 2026-06-07: `graphshell/` supercrate dissolved

The `graphshell/` supercrate directory (§1) is gone. Its README is deleted. The
graph + shell crates it held now live in four functional clusters directly under
`crates/`, designed with Mark to drop the `graphshell` umbrella and the residual
`mere-*` leaf prefixes, and to put presentation-of-the-graph crates next to the
graph framework they serve:

```text
crates/
├── graph/        # The graph's underlying structure (data)
│   ├── graph-kernel/      — `kernel`: Graph + geometry + identity + shared nav memory
│   ├── node-lineage/      — owner-scoped navigation lineage (the shared GraphMemory)
│   └── linked-data/       — RDF/linked-data ingest + export
│
├── orrery/       # Presentation + manipulation of the graph (the field-canvas)
│   ├── orrery/            — `orrery` (was orrery-host): the genet-on-winit content-root
│   │                         host; `frame(w,h) -> (Scene, redraw)` + the always-offload
│   │                         physics backend. meerkat hosts the same `Orrery` lib.
│   ├── gyre/              — rapier-backed body/field simulation + rapier-free LayoutView
│   │                         + barnes_hut (harvested O(n log n) repulsion primitive)
│   ├── aether/            — field algebra (the force source gyre integrates)
│   ├── cartography/       — ProjectionRequest / ViewIntent / Projection + LayoutStrategy
│   ├── arrangements/      — deterministic layouts (penrose / l-system / phyllotaxis /
│   │                         kanban-timeline / radial-grid / semantic) + LayoutStrategy adapters
│   └── mere-orrery/       — the orrery a11y/uxtree projection (domain layer)
│
├── shell/        # Chrome view-models (the bands around the content-roots)
│   ├── chrome/           — toolbar / omnibar / palette / authorities view-models
│   ├── frame/            — FrameLayout + PaneContent
│   └── comms/            — the comms pane view-model
│
└── system/       # Runtime + registries (host-neutral plumbing)
    ├── session-runtime/  — session graph / manifest / view-intent stores
    ├── shell-state/      — re-exports chrome modules + ux probes
    ├── ux-events/        — UX event vocabulary
    └── registry/         — the register-* capability registries (see root Cargo.toml
                             for the authoritative member list)
```

The split principle (Mark): **graph/** is the underlying structure; **orrery/**
is its presentation and manipulation — so `gyre` and `aether` belong with
`orrery`, not with `graph`, the way inker/forme/platen/verso are workbench-
associated. **system/** is the host-neutral runtime + registries, promoted out of
the old `shell/system` nesting. **shell/** keeps only the chrome view-models.

Because every consumer takes its workspace-internal deps via `dep.workspace = true`,
the moves were mostly a root-`Cargo.toml` path rewrite (members + the
`[workspace.dependencies]` paths) plus a handful of per-crate relative-path-dep
fixes where a crate crossed clusters (aether→kernel, meerkat→orrery). Verified
green: full `cargo build` + tests at the same counts as before the move (kernel
241, node-lineage 24, gyre 7, orrery 20, platen 45, workbench 4, meerkat 44+26).

### Staleness flag (deferred)

§§1–5 above predate **both** the genet-as-host flip **and** this dissolution.
They still describe `crates/mere/host` + `host-substrate` as the binary — the host
is now **`meerkat`** (genet-as-host; see the [genet host flip plan](../implementation_strategy/2026-06-01_genet_host_flip_plan.md)),
and several crates named there (`spatial-substrate`, `host-ports`, `control-plane`,
`register-renderer`, `verso/scrying-renderer`, `crates/mere/host-substrate`) have
since moved, merged, or been cut. The cluster tree in this §7 is the current
structural truth for the former `graphshell/` crates; a full §1–5 refresh against
the post-flip workspace is a separate pass, not done here.

## 8. Update — 2026-06-07: `canvas-ir` + `graph-layout` reviewed, recaptured, deleted

A liveness review found `canvas-ir` and `graph-layout` were both **dormant** —
compiled into the workspace but never reached by the running app. The live
graph-positioning path is `gyre` (rapier physics) → `cartography::Projection` →
`platen::{scene_paint, orrery}` → netrender; `canvas-ir`'s scene IR and
`graph-layout`'s `LayoutStrategy` adapters were bypassed (graph-layout was a
`platen` dependency used only in `#[cfg(test)]`).

Outcome:

- **`crates/orrery/arrangements`** (new): the keep-worthy deterministic layouts
  recaptured from `graph-layout` — penrose (P2/P3 aperiodic tilings), l-system
  (Hilbert/Koch/Dragon), static (grid / phyllotaxis / radial), axial (kanban /
  timeline), semantic embedding + edge-weight, the `curves` helpers, and the
  layout registry. Implements `cartography::LayoutStrategy` (adapters), carries
  its own light `scene`/`camera` snapshot types (the canvas-ir dependency is
  gone). The configurable ArrangementRelation design space, parked next to
  `gyre`/`aether`/`cartography` in the orrery cluster.
- **`gyre::barnes_hut`** (harvested): the O(n log n) Barnes-Hut quadtree +
  `repulsion_forces` primitive, lifted from `graph-layout` into the live physics
  crate, plus a `BarnesHutRepulsion` `gyre::Force` — a *global* charge-repulsion,
  the O(n log n) counterpart to `NodeExclusion`'s spatial-index charge. Per Mark
  (2026-06-07), both roles stay available and host-toggleable: compose it
  *alongside* `NodeExclusion` (local separation + global spread) or *instead of*
  it at scale, and expose `strength` / `theta` / `min_distance` as config. The
  Force is implemented + unit-tested (separates bodies over a tick); what remains
  is the host-side toggle/scale-switch + config surfacing and live calibration of
  `strength` against `NodeExclusion` (its falloff is FR-style `1/d`, not `1/d²`).
- **Dropped:** `force_directed` (gyre supersedes), the `extras` force-modifier
  passes (overlap gyre + aether fields), `physics_config`, and `canvas-ir`
  entirely (a scene IR parallel to the live netrender path). `platen`'s dead
  `canvas_scene` module went with `canvas-ir`; `cartography_scene` (the
  strategy-dispatch seam) stays, its test repointed to `arrangements`.

Verified green: arrangements 99, gyre 28 (+4 Barnes-Hut), platen 40, meerkat
44+26, full `cargo check --workspace` clean. Net: ~6k LOC of gyre-redundant /
dead layout code removed; the distinctive arrangements preserved + recaptured.

## 9. Update — 2026-06-10: `verso` supercrate retired

The 2026-06-10 stack audit confirmed verso's chartered realization role had
been decomposed and absorbed (constellation actors own lifecycle, platen-view
owns between-tiles geometry, node-lineage owns within-tile history, eidetic
owns cached content), leaving `verso-core` a 2k-LOC intent reservation with
one external export and `tile-state` with zero consumers beyond a
session-runtime re-export nothing downstream used. Outcome:

- **`crates/verso/` deleted** (`verso-core` + `tile-state`; git-revivable).
- **`SurfaceTargetId` inlined into `inker::routing`** (its sole external
  export; every use site already imported it via inker paths, so no caller
  changed).
- **Dead deps dropped**: platen + workbench (`verso-core`), session-runtime
  (`tile-state` + its unconsumed re-export). Stale layer claims removed from
  the platen README/lib docs, forme, uxtree, eidetic-core, and inker docs.
- **The name survives with a designated charter**: verso = the engine-flip /
  compatibility-view seam, minted at the first genet→scrying flip — see
  `verso_docs/technical_architecture/2026-06-10_compatibility_view_charter.md` (`genet/design_docs/verso_docs/technical_architecture/2026-06-10_compatibility_view_charter.md`).
  The `verso-tile` crates.io reservation is unaffected.

Verified green: lib tests for inker, nematic (157), scrying-engine (11),
session-runtime (67); `cargo check` clean for the five touched crates and
meerkat (full cross-repo graph).
