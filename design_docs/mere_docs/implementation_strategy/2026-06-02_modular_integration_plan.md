# Mere Modular Integration Plan

**Date**: 2026-06-02
**Status**: Draft (for review). The unifying sequence + architecture spine for
integrating all of Mere onto the single serval-as-host shell (`meerkat`). It does not replace the
canonical docs it weaves: the [composition spine](../technical_architecture/2026-05-21_mere_composition_spine.md)
(the model), the [serval-as-host flip plan](2026-06-01_serval_host_flip_plan.md)
(the host migration), and the [adoption roadmap](2026-05-27_adoption_roadmap.md)
(the R0–R5 wiring order). It sequences those three in-flight tracks into one build,
fixes the architecture's root question, inventories the (large) already-built
leverage surface, and schedules the cleanup.
**Grounded in**: a whole-corpus read this session (7 tracks over ~50 design docs +
crate verification against the 2026-06-02 tree). Where a doc disagrees with the
code, the code wins and the doc is flagged for reconciliation (§7).

---

## 1. The architecture: a graph-rooted projection model

One principle governs the whole shell, and it is the same one rejected in the
servoshell, graphshell, and meerkat eras whenever it tried to re-invert:

> **The graph is the sole root. Everything visible is a contingent projection of
> it. No projection may become the application root.**

- **The graph (session) is the root truth** — nodes, edges, fields. The durable
  thing a window attaches to (the [multiplexer model](../research/2026-05-11_browser_multiplexer_framing.md):
  window = attach client, graph = session).
- **The orrery is the graph's own spatial presentation.** It is the legitimate
  root *surface* because it renders the graph itself, not a projection of a subset.
- **A tile is a projection of a node** — the node's media, which resides in or is
  linked from it. It carries its binding (`graph_id`) so it knows what it projects.
- **The workbench, gloss, apparatus, system views, and tiles are all contingent.**
  The workbench is a cross-graph tiled-media-analysis *mode*; gloss is a
  content-commentary strip; apparatus is a system-inspector strip. Each hangs off
  the graph. Promoting any of them to root would invert the node→projection
  relationship — the inversion we keep rejecting. **No workbench (or gloss, or
  apparatus) as root.**
- **Tear-out is a leaf falling off the tree, not a new tree.** A detached tile
  keeps its `graph_id` binding; it does not become a new root graph. The leaf
  metadata exists for exactly this.
- **Panes are window-specific and mixable**, each bound to a graph via `graph_id`,
  so one window can hold panes across multiple graphs — but mixing happens at the
  window/pane layer, never by promoting a tiled view to root.

### The layered stack

Everything below the host is host-neutral. The host is the only serval-coupled
layer. The [composition spine](../technical_architecture/2026-05-21_mere_composition_spine.md)
(graph → forme → platen → verso → inker → host) is fixed; the flip changed only
the bottom realization substrate.

| Layer | Crates | Role |
| --- | --- | --- |
| Truth / data | `kernel` (graph + Field/Coupling), `eidetic`, `persona/identity`, `murm`/`moot`, `intel/embed`, `import`, `node-lineage` | graph + durable memory + identity + comms |
| Graph substrate | `aether` (fields), `gyre` (physics), `cartography` (projection), `graph-layout`, `platen::scene_paint`/`orrery` | the graph realized as spatial geometry + paint |
| Engines | `inker` (controller), `nematic` (smolweb), `serval` (fullweb), `scrying-engine` (system WebView), `document-canvas` | node media → render output |
| Composition | `forme` (arrangement authority), `platen` (projection compiler), `verso` (surface lifecycle), `frame` (tiled-mode pane tree) | arranging projections of the graph |
| Shell domain | `chrome`, `shell-state`, `session-runtime`, `ux-events`, `register-viewer` | host-neutral view-models + routing |
| **Host (serval-coupled)** | `meerkat` (+ `serval-winit-host`), retiring: `mere-app` | window + present + input; renders the projections |

---

## 2. The host model (meerkat)

meerkat is the one app shell. The render path is settled and the seam is **not** a
renderer-registry trait (that was killed in the [2026-05-21 rescaffold](../technical_architecture/2026-05-21_app_architecture_rescaffold.md);
the README's `host-ports/` + `register-renderer/` directories are gone from disk,
only an empty `register-renderer-types` stub survives). The seam is:

- **Two-root composition** (live in meerkat): a chrome-root (xilem-serval view tree
  over the reused `chrome` view-models, diffed into a `ScriptedDom`) composited
  over a **content-root**, via `netrender::compose_external_texture`. Separate-roots
  discipline from commit one.
- **The content-root is the orrery** — the active graph rendered spatially,
  full-bleed, chrome floating over it. (Today it is synthesized HTML; that is the
  central gap, §4.)
- **`content_for(graph / node / pane)` resolves to one of three composition modes**
  (the surviving, re-derived-against-serval form of the old NodeRenderer modes), as
  a **convention**, not a trait registry:
  - **in-scene** — a serval `ScriptedDom` content-root / `platen` PaintList (the
    orrery underlay, document tiles via `document-canvas`, fullweb via serval);
  - **embedded-frame** — a `netrender` `ExternalTexturePlacement` (scrying/web
    tiles, the path meerkat already exercises);
  - **overlay** — an OS WebView (wry), reserved.
- **`frame::FrameLayout` is the tiled-workbench mode container, not the window
  root.** Binary splits are BSP-complete and correct *for the tiled mode*; n-way
  grouping is tabs/stacks at the tile-group layer (`platen::PlanSlot::Tabs`,
  `forme::Arrangement::stacked`), not splits. The orrery is not a `FrameLayout`
  leaf; it is the content-root's spatial surface.
- **One bin; dev isolation by launch flag + headless lib tests** — `meerkat
  --graph <…> --mode orrery|workbench`, `--engine <…>`, etc. No per-domain app-host
  bin (the standalone `orrery-host` bin folds in and retires). Each lane stays a
  host-neutral lib with headless tests; the bin is the only window.

---

## 3. Already built — the leverage surface

The single biggest finding of the corpus read: **most of the parts exist, are
green, and are host-neutral. The work is wiring, not building.** Status verified
against the tree.

**Host / present (serval-as-host, live):**
- `meerkat` — chrome-as-DOM shell on screen: toolbar/omnibar/command-palette/linear
  history via xilem-serval over reused `chrome`; two-root composition. Content-root
  is now the `Orrery` (S1, `8786484`); fetch + a live engine behind it is the S2 gap.
- `serval-winit-host` — shared wgpu+netrender present stack (boot, rasterize,
  acquire, input mapping). Used by both serval bins. (Two copies exist; mere's is live.)
- `orrery-host` — the full interactive orrery (platen underlay + live gyre + abs-pos
  DOM node children under one camera + pan/zoom/inertia/drag/pick/marquee/edge-pick +
  pre-materialized pool), now factored into a reusable window-agnostic `Orrery` lib
  (`frame -> (Scene, needs_redraw)` + semantic input) that meerkat hosts as its
  content-root (S1.1 `64ebe44`); the bin is a thin shell over it.

**Render substrate (frozen, build on it):** `netrender` (Scene, `composite_paint_layers`,
external-texture, box-shadow masks), `paint_list_render` (`PaintCmd`→Scene;
`DrawStroke`/`DrawPath`→`SceneShape` landed), `serval-layout` (`IncrementalLayout` +
persistent Stylist + incremental inline-transform restyle), `pelt-live`
(`scene_from_scripted_dom`/`hit_test_node` — meerkat reuses), `xilem-serval`. Render
gaps are localized warn-skips (nine-patch borders, inset shadows, path clips, stroke
cap/join/dash, advanced blend) — none block the common pane.

**Graph substrate:** `kernel` (Field/Coupling now first-class primitives; open
Semantic predicate), `aether` (field algebra + Rhai + Burn, landed), `gyre`
(rapier physics + `edge_hit_test`/`rect_select`/`cull_aabb`/`pin`), `cartography`
(projection contract), `platen` (`orrery_paint_list`/`_from_positions`/`_demoted` +
`scene_paint` + `tree_projection` + morphorm layout).

**Engines / content:** `inker` (routing + EngineDocument + Engine/SurfaceEngine
traits), `nematic` (16 smolweb engines), `document-canvas` (`InkerPaintList`, real
text via parley), `scrying-engine` (host-neutral). The pipeline is proven
end-to-end in libs: bytes → `inker` route → `EngineDocument` →
`platen::build_document_scene` → `document_canvas` → `InkerPaintList` →
`paint_list_render` → `netrender::Scene`.

**Composition:** `forme` (arrangement authority; graphlet/lens/pressure parked),
`frame` (`FrameLayout` + `PaneContent`), `verso-core`/`tile-state` (latent, reshaped
to `TileId`).

**Data (built, unwired):** `eidetic` (4-layer blob→manifest→engram→models stack +
fjall + https/iroh fetchers; only consumer is intel/embed), `session-runtime`
(manifest / view-intent / thumbnail / service-runner, all v0a primitives, not
host-wired), `intel/embed` (Tier-2 embeddings, persists through eidetic),
`linked-data` (JSON-LD bridge), `import` (ingest, no consumers).

**P2P / social (bilateral lane far along, federation barely started):** `transport`
(`P2pandaTransport` live), `murmuring` (real p2panda operations), `murm`
(`SyncedCabal`, gossip + LogSync, real SyncStatus/resync), `persona/identity`
(BLAKE3 vault + passphrase backend; OS-keychain backend unbuilt), `misfin`/`webfinger`
(built), `moothold` (stub + tessera Phase 1), `mooting` (stub).

**Net:** `netfetcher` (sibling repo; WHATWG-Fetch engine, increments 1–5, 52 tests,
**zero consumers**), `net-media` (plan-only, no crate).

---

## 4. The integration gaps (the actual work)

1. **meerkat content-root** — ~~synthesized HTML, not the orrery~~ **the orrery now
   (S1)**. Remaining: no fetch and no live engine behind a node yet. The content
   pipeline exists; meerkat does not yet consume it. This is the S2 keystone gap.
2. **Two serval bins (meerkat + orrery-host) should be one shell** — *functionally
   folded* (S1.2): both run the same `Orrery` over the shared `serval-winit-host`.
   Remaining: the orrery-host bin's physical retirement, deferred to the S7 cutover.
3. **Node-media tiles** — *shipped* (S2.2): the focused node's media renders as a
   floating card from real fetched content — document lane (nematic) + HTML lane
   (serval). Remaining: web-via-scrying (S6), binary media, multiple / at-node tiles
   and the tiled workbench (S4).
4. **netfetcher** — *consumed* (S2.2b): meerkat fetches off the UI thread (tokio
   worker + `EventLoopProxy` wake) and routes the bytes to engines. Remaining: a
   durable cookie jar / cache (host-backed `FetchContext`), binary content, and a
   real `FetchContext` policy instead of `permissive()`.
5. **Persistence** — *graph + full view-intent wired* (S3.1 / S3.2a / S3.2b):
   `session_graph_store` persists the graph and `view_intent_store` the camera +
   focused node; meerkat restores all three on launch (a reload re-opens where you
   were, card and all). Remaining: per-persona/session/manifest threading and the
   eidetic content store for media + history (S3.2c).
6. **No tiled-workbench mode / peripheral panes** — `FrameLayout` exists but meerkat
   has a single content pane; gloss/apparatus are latent a11y projections.
7. **murm/moot unsurfaced** — `SyncedCabal` works but no comms surface exists.
8. **graph-canvas not deleted** — physics/fields/projection/paint are re-homed, but
   `platen::canvas_scene` + `graph-layout` still type-couple to it; IR types must be
   re-homed before deletion.
9. **register-\* latent + dual routing** — `register-viewer` (mime→viewer) duplicates
   `inker::routing` (engine-id); reconcile.
10. **Pervasive doc staleness** — README lists cut crates; many docs predate the
    June tree.

---

## 5. The build: lanes + near-term sequence

Each **lane** is a host-neutral lib (or libs) with headless tests, developable
independently and exercised in isolation via a meerkat launch flag. The lanes:
**Host/Chrome**, **Orrery/Spatial**, **Engines/Content**, **Composition/Workbench**,
**Persistence/Session**, **Field-system**, **P2P/Social**, **Intel**. The near-term
critical path threads the flip plan (P1–P5) and the adoption roadmap (R0–R5):

- **S1 — Orrery as meerkat's content-root (flip P1 → in the real shell).** Fold
  orrery-host's loop into meerkat as the content-root spatial surface (the graph,
  rendered), chrome composited over it. *Leverage*: orrery-host is done;
  `serval-winit-host` is shared. *Done*: meerkat opens to an interactive orrery of a
  graph, pan/zoom/drag/pick working, chrome on top. **Done `2026-06-02`** (S1.1
  `64ebe44` + S1.2 `8786484`); the orrery-host bin's physical retirement folds into
  the S7 cutover (it stays a lib test-harness over the shared `Orrery` until then).
- **S2 — Node media as tiles + fetch (flip P4 in-scene part + netfetcher).** Wire the
  engine pipeline so a node's content renders as a tile (in-scene): omnibar URL →
  `netfetcher::fetch` → `inker` route → engine → `document-canvas`/serval →
  `PaintList` → composited tile, bound to the node (`graph_id`). *Leverage*: the
  whole pipeline + netfetcher exist. *Done*: navigating populates the graph and shows
  a node's media as a tile. This is the graph-rooted browse loop.
  - *S2.1* (`df50602`): `Orrery::visit(url)` grows the session graph on navigation
    (URL identity dedups; new nodes link from the current selection as a browse
    trail); meerkat seeds the root and visits on every navigation.
  - *S2.2a* (`415bd88`): the focused node's media renders as a **floating card** over
    the orrery (Mark's placement pick), via the proven synchronous document pipeline
    (`EngineDocument → layout_document → scene_from_packet → composite`). Content is
    synthesized (welcome page / address placeholder) — only the byte source is a stub.
  - *S2.2b-i* (`1d9d9c0`): real `netfetcher::fetch` off the UI thread — a tokio
    worker + an `mpsc` channel + an `EventLoopProxy<()>` wake (delivery model 2),
    drained in `user_event`; per-URL content cache (Loading / Ready / Failed) with
    real loading + error card states. Renders fetched bytes as plain text.
  - *S2.2b-ii* (`7c03a93`): content-type routing — a nematic `EngineRegistry`
    (markdown / gemtext / plain / feeds) through the document lane, and `text/html`
    through the **serval lane** (`set_inner_html` → `scene_from_scripted_dom`,
    reusing the host renderer). This consumes netfetcher first (gap #4) and is the
    async-host architecture S3/S5 reuse. **S2 done `2026-06-02`.**
- **S3 — Persistence host seam (R-data).** meerkat constructs a per-identity `eidetic`
  Store + a `ManifestStore`, `load_from_disk` on startup, `mark_dirty`→debounced
  flush. Build `session_graph_store` (the eidetic↔`kernel::graph` glue; serde
  `graph.json` as the live store). *Done*: a graph survives restart; view-intent
  persists per pane.
  - *S3.1* (`900253e`): `session-runtime::session_graph_store` saves / loads the
    graph through its serde `GraphSnapshot` as `graph.json` (native-only); meerkat
    restores `<data_dir>/mere/graph.json` on launch (`Orrery::with_graph`, positions
    preserved, no re-settle) and saves after each navigation. Load failure falls
    back to a fresh session. **The graph survives restart.**
  - *S3.2a* (`8de3946`): the **camera persists** via `view_intent_store`. The orrery
    exposes `CameraView` (pan + zoom); meerkat treats `<data_dir>/mere` as the
    session dir (`graph.json` + `views/` siblings), maps the camera to/from the
    `CameraSnapshot` affine, restores it on launch (suppressing the recenter), and
    saves graph + camera after each navigation and on close. **View-intent (camera)
    persists.**
  - *S3.2b* (`fe5fdd3`): the **focused node persists**. `ViewIntent` gained a `focus`
    field (the node's URL); `Orrery::select_by_url` re-selects an existing node
    without adding one; meerkat saves the focused URL with the camera and re-selects
    it on launch. **View-intent persistence is complete: graph + camera + focus all
    survive restart, so a reload re-opens the card you had open.**
  - *S3.2c* (remaining): persona / session / manifest threading (the `ManifestStore`
    and per-id `sessions/<id>` layout already exist) for multi-session / multi-window;
    the eidetic content store for fetched media + history (debounced `mark_dirty`
    flush), so pages survive restart without re-fetching.
- **S4 — Tiled-workbench mode + peripheral panes (flip P2 + verso, R2).** `FrameLayout`
  becomes the cross-graph tiled-analysis mode over node-tiles; retarget
  `platen::layout` Morphorm→taffy under serval; light up gloss/apparatus projections;
  verso surface lifecycle gains its first real consumer here (consumer-first). *Done*:
  a workbench mode arranges node-tiles from one or more graphs; tear-out keeps
  `graph_id`.
- **S5 — Comms surface + cheap p2p win (R5 start).** Surface `murm`'s `SyncedCabal` as
  a comms view; wire `eidetic-iroh-fetcher` + transport blobs as cheesecloth-pinning
  v0 (replicate engrams by hash). Build the host OS-keychain `IdentityStorage`.
  *Done*: a working bilateral comms surface; engrams replicate by hash.
- **S6 — External content re-home (flip P4 embedded-frame).** Re-land scrying web
  tiles on meerkat via `ExternalTexturePlacement` (discard the dead Masonry fork
  edits; the scrying-engine crate is host-neutral). *Done*: a web/scrying tile
  composites through serval.
- **S7 — Cutover + cleanup (flip P5 + R4).** Retire `crates/mere/app` and the
  xilem/masonry fork deps once meerkat reaches parity; then §7 cleanup.
- **Later (own milestones):** federation proper (moot machinery), intel workers via
  the SessionServiceRunner, profiles/advanced tear-out (branch/fork), the field-system
  intelligence payoff (Burn embedding → vector field). These ride their own lanes and
  do not gate the host integration.

---

## 6. Cross-cutting decisions to resolve

- **Dual routing.** `inker::routing` (engine-id, live) stays canonical; harvest
  `register-viewer`'s capability/conformance declarations into it. Resolve when
  meerkat first routes >1 content engine.
- **Two graph-persistence formats.** Pick serde `graph.json` (inspectable,
  linked-data-compatible) as the live session store; reserve the kernel's `rkyv`
  `persistence.rs` path for a later compaction tier. No parallel store.
- **Three fetch lanes stay distinct.** `netfetcher` (WHATWG HTTP), `eidetic`-fetchers
  (content-addressed blob), `transport` (P2P) serve different policies; defer any
  convergence until duplication bites.
- **Unify the two Camera types** (mere-app center-based, platen offset-based) into one
  host-neutral Camera (flip D5).
- **Keep the two capability layers separate.** In-process gates
  (`kernel::permissions`, most-restrictive-wins) vs the federation capability stack
  (Meadowcap cluster-path caps + Biscuit policy + p2panda group-key). Do not conflate.
- **`EngramId` → `eidetic::ManifestId`** at the consolidation bridge; do not let the
  placeholder string become a parallel identity.
- **STM TTL vs eidetic no-GC.** Short-term sidecars (session-runtime) own sweep;
  eidetic engrams are durable-by-default.
- **Pick one `serval-winit-host`** (mere's is live; confirm serval's copy is
  reference, not a drifting fork).
- **Narrow `kernel::host_toolkit.rs`** — it still enumerates Iced/Gpui/Egui/Makepad
  host adapters, a multi-adapter assumption that predates the serval-only flip.

---

## 7. Cleanup + doc reconciliation

Schedule these so they stop polluting the topology and misleading readers:

- **Delete graph-canvas** after re-homing its shared IR types (`CanvasSceneInput`,
  `CanvasViewport`, `ProjectionMode`, `Color`) out of the monolith (into
  `cartography` or a thin canvas-ir crate) and dropping `platen::canvas_scene` +
  `graph-layout`'s type-coupling. Behavioral modules already have zero live
  consumers (orrery-host + mere-app bypass it).
- **Relocate `kernel` + `cartography` + `graph-layout` out of `crates/graphshell/`**
  (R4, pure move) — kernel-under-shell is upside down.
- **Sync `crates/graphshell/README.md`** to drop the cut `host-ports/` /
  `register-renderer/` / `control-plane/` / `spatial-substrate/` rows.
- **Mark superseded (not delete)**: `host_architecture_roadmap`, the
  `renderer_registry_contract`/`spatial_chrome_ir`/`xilem_embedding` briefs,
  `typed_action_bus_plan`, the between-tiles seam §4, the verso adoption + scrying
  integration host-wiring (the crates survive; the Masonry/Xilem wiring is dead),
  and the p2p docs' stale code-state (the 06-01 p2panda spike + 06-02 LogSync plans
  are canonical current-state).
- **Fix stale headers**: `platen/lib.rs` ("document-canvas planned" — it exists),
  `murmuring/lib.rs` ("Cable wire coming"), the eidetic hash-agility open-questions
  (multihash Hash shipped), and re-root "mere-host-runtime" doc paths onto
  `session-runtime` + meerkat.
- Per DOC_POLICY: update `DOC_README.md` for any doc add/move (done this session for
  this plan), and carry the R0 invariant contracts (a11y capability, 5-scope
  permissions, temporal-integrity, three-pass Chrome→Content→Overlay compositor,
  undo-classification) as guardrails cited per-slice.

---

## 8. Explicitly deferred

geist / LLM serving / Distillery / LoRA-as-engram; `net-media` (WebRTC + decode);
federation proper (moot graph-view CRDT, mooting adapters, bridges, tier machinery);
group-key E2EE (p2panda-spaces vs Keyhive — both pre-v1, watch, do not adopt);
Biscuit policy over tessera facts; Veilid per-moot transport; advanced tear-out
(branch/fork) + nonstandard browsing profiles; SPARQL/Oxigraph content-subgraph
projection. None gate the host integration; each rides its own lane when a real
consumer appears.

---

## Progress

- **2026-06-02** — Plan created from a whole-corpus read (7-track read-only fan-out
  over ~50 docs + crate verification). Settled the architecture root question with
  Mark: the graph is the sole root, the orrery is its spatial presentation, and the
  workbench/gloss/apparatus/tiles are contingent projections (no inversion). Recorded
  the leverage surface (most parts built + host-neutral), the integration gaps, the
  lane model + near-term sequence (S1 orrery-as-content-root → S2 node-media+fetch →
  S3 persistence → S4 workbench → S5 comms → S6 external → S7 cutover+cleanup), the
  cross-cutting decisions, and the cleanup/doc-reconciliation list. No code this pass;
  next is S1 (fold orrery-host into meerkat as the content-root).
- **2026-06-02 — S1 (substance) done: orrery is meerkat's content-root.**
  - *S1.1* (`64ebe44`): extracted orrery-host's standalone `App` into a reusable,
    window-agnostic `Orrery` lib (`orrery-host` gained a `[lib]` target) —
    `frame(w, h) -> (Scene, needs_redraw)` (composites the underlay + abs-pos node
    pool + marquee; no present) plus semantic input methods (`pointer_down`/`up`,
    `cursor_moved`, `wheel`, `set_ctrl`, `reseed`) returning a redraw flag, and a
    `PointerButton` enum. Construction + paint/DOM helpers split into `build.rs`
    (every file < the 600-LOC ceiling: lib 481, build 226, main 190). The bin is now
    a thin winit shell over the lib. 6/6 lib tests pass.
  - *S1.2* (`8786484`): meerkat depends on the orrery-host lib; the content band
    below the chrome is now an `Orrery` instead of synthesized HTML. `render()`
    composites the orrery's scene at the content band and chains a redraw while it
    animates; content-band input (cursor / wheel / press / release / modifiers)
    routes to the orrery in band-local coordinates; the chrome band keeps its
    hit-test. Removed the synthesized content seam (`content.rs` + the `content`
    module). 31/31 meerkat lib tests pass; meerkat builds clean.
  - *Deferred*: the orrery-host bin's **physical** retirement (gap #2's last step)
    folds into the S7 cutover, alongside `mere-app`, rather than deleting a working
    isolated harness mid-stream. It shares the `Orrery` lib + `serval-winit-host`
    with meerkat (no divergent engine), so it is a lib test-harness bin, not a
    competing app host. Done-condition for S1's interactive-orrery-in-the-shell is
    met; only the cleanup tail moves.
- **2026-06-02 — S2 (synchronous browse loop) done; real fetch (S2.2b) is next.**
  - *S2.1* (`df50602`): `Orrery::visit(url)` grows the session graph as you navigate
    — URL identity selects an existing node, else a new node is added and linked from
    the current selection (the browse trail as graph structure), re-syncing physics +
    the node pool and re-settling. `Orrery::new` now starts empty; the sample ring
    moved to `with_sample_graph` (bin + tests). meerkat seeds the root from the
    initial location and `visit`s after each navigation. 8 orrery lib tests.
  - *S2.2a* (`415bd88`): the focused node's media renders as a **floating card** over
    the orrery (Mark's placement pick; composite order orrery → card → chrome). Proves
    the document pipeline in the shell: `node_document(url)` (synthesized content,
    synchronous) → `layout_document` → `scene_from_packet` → composited card, cached
    by `(url, size)`. `Orrery::focused_url()` feeds it. meerkat gained inker +
    document-canvas (netrender feature); 4 card tests. Only the byte source is a stub.
- **2026-06-02 — S2.2b done: real fetch + content-type routing. S2 complete.**
  - *S2.2b-i* (`1d9d9c0`): off-UI-thread fetch. `meerkat::fetch` runs netfetcher on a
    tokio worker; outcomes return over an `mpsc` channel and wake the loop via an
    `EventLoopProxy<()>` (delivery model 2), drained in `user_event`. Per-URL content
    cache (Loading / Ready / Failed) keyed by URL identity; the card shows real
    loading + error states. Deps: netfetcher (sibling), url, tokio. 6 bin tests.
  - *S2.2b-ii* (`7c03a93`): content-type routing. A nematic `EngineRegistry`
    (markdown / gemtext / plain / feeds) feeds the document lane; `text/html` rides
    the **serval lane** (`set_inner_html` → `scene_from_scripted_dom`), confirming
    Mark's point that serval is free here (already the host). 10 bin tests
    (markdown-through-nematic + HTML-through-serval). Deps: nematic.
  - The full graph-rooted browse loop now runs: navigate → graph grows a linked node
    → fetch off-thread → route by type → the focused node's media renders as a card,
    chrome on top. The async-host seam (worker + channel + proxy wake) is the
    foundation S3 (persistence flush) and S5 (murm sync) push results through.
  - *Process note*: `Cargo.lock` left unstaged across S2.2b — a sibling crate's
    concurrent fastbloom/zstd change is interleaved in the shared lock; the next lock
    commit carries meerkat's netfetcher / tokio / url / nematic entries with it.
  - *Deferred from S2*: durable `FetchContext` (cookie jar / cache / real CSP) instead
    of `permissive()`; binary media; page-supplied CSS in the serval HTML lane.
- **2026-06-02 — S3.1 done: the session graph survives restart.**
  - `session-runtime::session_graph_store` (`900253e`, native-only): `save` / `load`
    the graph through its serde `GraphSnapshot` (URL-stable) as pretty `graph.json`,
    plus a `sessions/<id>` path helper for when manifests thread the session id.
  - `orrery-host`: `Orrery::with_graph` restores a graph keeping each node's saved
    position (no spiral re-seed, no auto-settle), and `graph()` exposes it to persist.
  - `meerkat`: loads `<data_dir>/mere/graph.json` on launch (else seeds fresh from the
    initial location) and saves after each navigation; load failure falls back to a
    fresh session. Deps add session-runtime + dirs. 3 store + 10 orrery tests pass.
  - *Next (S3.2)*: persona / session / manifest threading (the `ManifestStore` +
    `sessions/<id>` layout exist), camera + focused-node restore via
    `view_intent_store::CameraSnapshot`, and the eidetic content store for media.
  - *Also batched this session* (`91d94bd` + `f93a257`): committed the concurrent
    working tree on request — the p2panda/p2p + social lane (murm / transport /
    persona / moothold-tessera / probes / docs) and the register-renderer-types
    scaffold. Not build-verified here (sibling-agent WIP); two new p2p/tessera docs
    are committed without `DOC_README` index lines (flagged for their authors).
- **2026-06-03 — S3.2a done: the camera survives restart.**
  - `orrery-host`: `CameraView` (plain pan + zoom) with `camera()` / `set_camera()`
    (guards a non-finite / zero zoom, clamps to range).
  - `meerkat` (`8de3946`): `<data_dir>/mere` is the session dir (`graph.json` +
    `views/` siblings, per the view-intent spec); the camera maps to/from the
    `CameraSnapshot` affine and restores on launch (suppressing the first-frame
    recenter); graph + camera save after each navigation and on window close. Reuses
    `view_intent_store` (atomic writes); no new deps. 11 orrery + 11 meerkat tests.
  - The serval HTML lane also gained page-supplied inline `<style>` layering
    (`inline_stylesheets_from_source`), landed alongside by a sibling change.
  - *Next (S3.2b)*: `ViewIntent.focus` to re-open the focused node's card on reload;
    persona / session / manifest threading; the eidetic content store for media.
- **2026-06-03 — S3.2b done: the focused node survives restart. View-intent complete.**
  - `session-runtime` (`fe5fdd3`): `ViewIntent` gained a `focus` field (the focused
    node's URL, by URL identity); `is_empty` + a round-trip test updated. Two distinct
    `ViewIntent` types exist — this persistence one (extended) vs `cartography`'s
    projection-request one (`focus: Option<NodeKey>`); the persistence one is URL-keyed.
  - `orrery-host`: `Orrery::select_by_url` re-selects an existing node by URL without
    adding one. `meerkat`: saves the focused URL with the camera, re-selects it on
    launch after the graph + camera restore. 10 view-intent + 12 orrery tests pass.
  - **View-intent persistence is complete**: graph (S3.1) + camera (S3.2a) + focus
    (S3.2b) all survive restart. *Remaining S3.2c*: persona / manifest threading for
    multi-session / multi-window, and the eidetic content store (no re-fetch on reload).
