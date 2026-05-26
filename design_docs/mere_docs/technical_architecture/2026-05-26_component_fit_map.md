# Mere Component Fit-Map

**Date**: 2026-05-26
**Status**: Orientation inventory. Where every workspace component sits, its
current state, and the overlaps/gaps to reconcile. Companion to the
[composition spine](2026-05-21_mere_composition_spine.md) (the conceptual flow)
and the [salvage maps](../research/2026-05-22_retired_host_stack_salvage_map.md)
(the cut/decomposing parts). This doc is the *literal placement* + state.

---

## The live path (what the running app actually touches)

After the re-scaffold, `mere-app` exercises a **thin slice** of the workspace:

```
kernel (graph truth, +store)
  → forme (arrangement, +store)        ← persisted
  → platen::project_tree (tree projection only)
  → mere-app/panes.rs (Xilem views)    ← NOT verso
  → inker (EngineRoutePolicy / registry)
  → nematic (markdown engine)          ← the one wired engine
+ mere-app/{camera,graph_canvas}.rs    ← hand-rolled orrery (NOT graph-canvas crate)
+ kernel::store + forme::store         ← persistence (NOT session-runtime)
```

Everything else is either **planned future tier**, **orphaned-but-keep**, or a
**crate the host reimplemented thinly and bypassed**. That last category is the
important finding.

## The central finding: hand-rolled host vs. existing machinery

The re-scaffold deliberately built a lean idiomatic-Xilem host. In doing so it
**reimplemented, in miniature, several things that already exist as substantial
crates**, and bypassed them:

| Live path uses (hand-rolled) | Existing crate it bypasses | Crate LOC |
|---|---|---|
| `mere-app/graph_canvas.rs` + `camera.rs` (orrery widget: scene, camera, hit-test) | **graph-canvas** (framework-agnostic scene/camera/projection/hit-test IR) | 9,608 |
| seeded ring layout in `main.rs` | **graph-layout** (force-directed/analytic/streaming) + **cartography** (LayoutStrategy) | 9,643 + 1,016 |
| `engine_tile::document_views` (blocks → masonry text) | **document-canvas** (parley layout → netrender via paint_list) | 2,397 |
| `panes.rs` workbench/apparatus/toolbar views | **workbench** / **apparatus** / **chrome** domain crates | 217 + 101 + 2,446 |
| direct masonry rendering of EngineDocument | **verso-core** / **tile-state** (tile/surface realization) | 1,031 + 985 |

This isn't a bug, it's the prototype doctrine (rewrite over migrate). But it
means **the workspace now has two layers that do the same jobs**, and each
overlap needs a deliberate call: **adopt** (grow the host into the crate),
**retire** (the hand-rolled version wins, cut the crate), or **keep-latent**
(revive when the slice lands). My read on each is in the table below; none are
urgent, but they should be decided rather than drift.

## Full inventory

State legend: **live** (on the running path) · **partial** (used in part) ·
**latent** (kept, not wired) · **stub** (skeleton) · **planned** (future tier) ·
**external** (sibling repo).

### Graph truth + spatial layout
| Crate | LOC | Role | State | Note |
|---|---|---|---|---|
| `kernel` | 12,776 | identity / authority / mutation kernel; graph truth + `store` | **live** | relocate **out** of the `graphshell/` supercrate (kernel-under-chrome is upside down) |
| `node-lineage` | 1,540 | owner-scoped navigation lineage (visit branching) | latent | the per-tile history model; not yet wired |
| `cartography` | 1,016 | projection layer, `LayoutStrategy` contract | latent | **adopt** for the orrery's real layout (replaces the seeded ring) |
| `graph-layout` | 9,643 | layout algorithms (analytic/streaming) | latent | **adopt** via cartography when the ring goes |
| `graph-canvas` | 9,608 | framework-agnostic graph canvas IR (scene/camera/hit-test/render-packet) | latent | overlaps `mere-app/graph_canvas.rs`; **decide adopt-vs-retire** — the hand-rolled widget covers v1, this crate has the richer IR |
| `aether` | 394 | rapier physics (forces/fields on nodes) | latent | sidequest: streaming/force-directed motion |

### Arrangement + projection + realization
| Crate | LOC | Role | State | Note |
|---|---|---|---|---|
| `forme` | 6,455 | arrangement authority + `store` | **live** | canonical; most of the crate (graphlet/lens/pressure) is parked, v1 uses `arrangement` + `forme_document` |
| `uxtree` | 537 | a11y / automation tree | latent | projection target for the domain crates; a11y not yet wired in the host |
| `platen` | 1,515 | projection compiler; `project_tree` + canvas/cartography/document scene wrappers | **partial** | `project_tree` is live; `canvas_scene`/`cartography_scene`/`document_scene` (the graph-canvas/cartography/document-canvas wrappers) are **not** used by the host |
| `verso-core` | 1,031 | tile/surface realization | latent | **the gap** — the host renders engine docs directly into masonry, bypassing verso. Decide where verso fits or whether masonry-direct is the answer |
| `tile-state` | 985 | tile lifecycle (within-tile history, cache) | latent | verso-adjacent; not wired |

### Engine choice + content
| Crate | LOC | Role | State | Note |
|---|---|---|---|---|
| `inker` | 3,186 | engine controller / routing / `EngineDocument` | **live** | the routing seam works end to end |
| `nematic` | 4,961 | smolweb engines (markdown/gemtext/gopher/feed/…) | **live** (markdown) | the other lanes (gemtext/feed/…) light up once real fetch lands; rung-1 customer of netfetcher |
| `document-canvas` | 2,397 | parley document layout → netrender (paint_list) | latent | overlaps `engine_tile::document_views`; **adopt** for real document layout once masonry-text stops sufficing |
| `scrying-engine` | 735 | `inker::SurfaceEngine` over the system WebView | **present, unwired** | the **next slice** — close the browse loop |

### Host + domain
| Crate | LOC | Role | State | Note |
|---|---|---|---|---|
| `mere-app` | 1,753 | the Xilem host (6 files) | **live** | the running app |
| `frame` | 880 | FrameLayout domain (savable resizable panes) | **partial** | host uses `GraphId`; `FrameLayout` not yet (the real multi-leaf FrameTree slice) |
| `orrery` | 144 | kernel graph → uxtree (a11y projection) | latent | name collision with the host's orrery *widget*; this is the a11y projection, not the renderer |
| `chrome` | 2,446 | toolbar/omnibar/menu view-models | latent | host hand-rolls its toolbar; **adopt** the view-models or **retire** if the hand-rolled chrome wins |
| `workbench` / `gloss` / `apparatus` | 217 / 133 / 101 | domain → uxtree projections | stub/latent | host hand-rolls workbench + apparatus content; these are the a11y/domain projections |
| `shell-state` | 176 | shell session-state aggregator | latent | keep-latent (salvage map) |
| `ux-events` | 1,998 | UX event taxonomy + telemetry | latent | keep-latent; instrument the host eventually |
| `register-diagnostics` | 2,934 | diagnostics registry (253 channels) | latent | keep-latent; valuable instrumentation |
| `session-runtime` | 2,378 | session manifests / view-intent / engine-profile / thumbnails | latent | host uses its own `session_dir` + `forme::store` + `kernel::store`; **rewire** the manifest/view-intent stores when multi-session lands |

### Future tiers (planned by design, not wired)
| Crate | LOC | Role | State |
|---|---|---|---|
| `eidetic` (+ `-fjall`/`-https-fetcher`/`-iroh-fetcher`) | ~4,800 | content-addressed local memory lane (blob store) | planned |
| `embed` | 4,951 | embedding provider + vector index (intelligence tier) | planned |
| `murm` / `murmuring` / `transport` | ~5,600 | bilateral P2P comms (iroh QUIC) | planned |
| `moothold` / `mooting` | 61 | federation supercrate | stub (skeleton) |
| `identity` | 1,686 | personas, Ed25519, keychain | planned (persona tier) |

### Externals (sibling repos)
| Repo | Role | State |
|---|---|---|
| `serval` | full-web engine (servo stack) | external, WIP — the **delegate** lane |
| `netrender` | Scene rasterizer + `paint_list_{api,render}` | external — document-canvas lowers to it |
| `netfetcher` | portable WHATWG Fetch | external, planned — the network organ; rung-1 customer is nematic |
| vendored: `xilem` / `imaging` / `blitz` / `glass-gpui` | UI / paint / HTML-lib / gpui-fork | external, minimally maintained |

---

## What this surfaces (the reconciliation questions)

1. **Adopt-vs-retire the bypassed machinery.** The host reimplemented thin
   versions of graph-canvas, graph-layout/cartography, document-canvas, verso,
   and the chrome/domain crates. Most should be **adopt-when-the-slice-lands**
   (the crates hold real capability the host will want: force-directed layout,
   parley+netrender documents, tile lifecycle, a11y projections). A few may be
   genuinely **superseded** by the lean host (chrome's view-models if the
   hand-rolled toolbar suffices). The hand-rolled pieces (`camera.rs`,
   `engine_tile::document_views`, the seeded ring) are explicitly scaffolding to
   be replaced.
2. **Verso is the clearest gap.** The realization layer isn't on the live path.
   Either the host grows to render *through* verso/tile-state (tile lifecycle,
   surface management), or "masonry-direct" is accepted as the answer and verso
   is rescoped. Decide before the scrying.web tile lands, since that tile is
   exactly a verso-shaped surface.
3. **graph-canvas crate vs the host widget.** 9.6k LOC of scene/camera/hit-test
   IR sits unused beside a 441-LOC hand-rolled widget. When the orrery needs
   cartography layout + LOD + render-packets at scale, that's the moment to
   adopt the crate (or extract the parts worth keeping); until then the widget
   wins on simplicity.
4. **kernel + cartography relocate** out from under `crates/graphshell/`.
5. **The orphaned-keep cluster** (chrome, shell-state, ux-events,
   register-diagnostics, session-runtime, the domain crates, aether) is real
   value with no current consumer — track it here so it's revived or retired
   deliberately, not left to rot.

## Recommendation

Treat the hand-rolled host layer as the **temporary inner ring** and the bypassed
crates as the **outer ring to grow into**, slice by slice, with a named call per
overlap:

- **scrying.web tile** → forces the verso question (#2).
- **real cartography layout** → adopts cartography + graph-layout, and forces
  the graph-canvas-crate question (#3).
- **real document layout** → adopts document-canvas (+ netrender).
- **multi-session / multi-window** → rewires session-runtime.
- **real chrome / a11y** → adopts chrome + the uxtree domain projections, or
  formally retires them.

The future tiers (eidetic, embed, murm/moot, identity) are correctly dormant;
they wire in at their own milestones and aren't overlaps, just not-yet.
