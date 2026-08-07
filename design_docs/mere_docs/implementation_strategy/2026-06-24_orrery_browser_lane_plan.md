# Orrery Browser Lane Plan (capture-first)

**Status:** planning / design 2026-06-24. **Supersedes** the node-representation and
delivery framing of the
[browser_extension_companion_plan](2026-06-23_browser_extension_companion_plan.md):
its "orrery-in-a-tab live DOM cards" P1 is replaced here by **capture-first,
favicon-body nodes, a gloss sidebar, and the orrery as a discrete surface**. The
companion / smolweb / p2p / federation half of that plan stands as the forward
vision; this plan is the shippable **v1 with no native sync**.

**Product framing superseded 2026-07-27** by the
[Graphshell reference host plan](2026-07-27_graphshell_reference_host_plan.md).
The capture API and browser-delivery findings below remain evidence; Graphshell,
not the former orrery/Merecat framing, owns the extension and PWA product.

**One line.** A cross-browser extension that uses Mere's *own* orrery and data model
to turn your real browsing into a rich, owned, queryable graph. The browser stays
the browser; Mere adds the relational memory layer the browser lacks.

---

## 1. Thesis

- **The browser is another host for the orrery, the way pelt is.** The orrery is
  already `window-agnostic` (`frame(w,h) -> (Scene, needs_redraw)` + semantic input);
  meerkat hosts it on pelt/genet/netrender natively. Swap the host adapter and the
  *same component* renders to a browser surface. Not a port, the same orrery.
- **Capture is the product; the views are secondary.** Even a user who never opens
  the orrery gets something a browser cannot give them: a private, durable, queryable
  browsing memory (snapshots + extracted content + navigation provenance + the link
  neighborhood), in the Mere data model. The ask is "let me remember your browsing
  well," not "change how you browse."
- **A node is a physical thing, not a document.** A node is a DOM *object* in the
  host (an element with a position, a visual, hit-testing), which is *not* the same as
  a node being a DOM *document*. The second is the bad inference that produces cards /
  snapshots / live previews as if they were the node. They are not. A node is a
  content-type-coded **shape** in the gyre scene, carrying a favicon, optionally
  swapped for a sprite with an adjustable hull. The page it references is presented at
  a higher tier, never embedded as the node.
- **The map is not the territory.** The orrery is the map (snapshot nodes, relations,
  provenance). The browser tab is the territory (live pages). Clicking a node opens
  the real page in a normal tab. The canvas is never asked to *be* the browser.

## 2. Why all three browsers rule (the unlock)

The Memory64 / SharedArrayBuffer / COOP-COEP / nightly-build-std apparatus (genet's
`docs/2026-06-24_nova_memory64_browser_lane_plan.md`, and the
[substrate parallelism brief](../../2026-06-21_substrate_parallelism_composition_brief.md))
exists for one purpose: running a web engine (Nova page-JS + the parallel cascade)
*inside* the browser. WebKit cannot do that (no Memory64). **This lane does not run a
web engine.** The browser is the web engine; you let it browse. Mere runs only the
**orrery (graph viz) + capture + eidetic**, which is plain Rust→wasm with no JS engine
and no parallel cascade. So none of that apparatus applies:

| Need | Mechanism | Chromium | Gecko | WebKit |
| --- | --- | --- | --- | --- |
| compute | baseline `wasm32`, single-threaded | ✓ | ✓ | ✓ |
| store | OPFS (`eidetic-opfs`, Phase 7) | ✓ | ✓ | ✓ (16.4+) |
| underlay render | WebGPU, Canvas2D fallback | ✓ | ✓ | ✓ (18+) / fallback |
| capture + surfaces | WebExtension MV3 core | ✓ | ✓ | ✓ (Xcode wrap) |

The scary part of "cross-browser" was a different, harder product (genet-in-the-
browser) that this lane sidesteps. WebKit's only real friction is **distribution**
(Safari Web Extensions need an Xcode wrapper + App Store review) and **WebGPU
maturity** (Canvas2D is the insurance), not capability.

## 3. The form: an extension, three surfaces, one local store

Capture must be an extension (a PWA cannot observe other tabs / navigation), so the
extension is the spine and a standalone PWA viewer is a later option. One extension
origin = one OPFS store all surfaces share.

- **Background capture** (service worker + content script): on each `webNavigation`
  commit, the content script hands the page HTML + favicon to the background, which
  runs `genet-extract` (title / metadata / outline / links / text), takes a
  `captureVisibleTab` snapshot, and writes a node + a provenance edge into eidetic.
  Runs whether or not a view is ever opened.
- **The gloss sidebar** (`sidePanel` / sidebar): the djot outline of where you are,
  the page's link neighborhood, your recent corridor. Plain HTML (the gloss is DOM,
  not the spatial canvas, so it is light), the everyday surface.
- **The orrery page** (a discrete extension tab): the wasm orrery, a field of
  favicon-shape bodies over the WebGPU/Canvas2D underlay. Select a node → its snapshot
  preview appears off to the side; click → the real page opens in a normal tab.

## 4. The node model (explicit, so the doc stops drifting back)

A node's representation is a **ladder**, loaded at the tier the moment calls for:

1. **Default, always, cheap:** the content-type-coded shape + favicon, or a sprite
   with an adjustable hull (the [node-body/face model](2026-06-23_node_body_face_model_plan.md)
   / [node-representation arrangement](2026-06-18_node_representation_arrangement_plan.md)).
   A body in the gyre field. This is a DOM element, gyre-positioned over the underlay.
2. **On select:** the snapshot preview appears off to the side. The "what this page
   looked like" peek, without displacing the node or cluttering the field.
3. **On open/focus:** the full card / detail, or just opening the real page in a tab.

The favicon-body default is what makes the orrery *spatial memory*: you recognize a
site by favicon + position + cluster, and the snapshot is the confirm-the-recall peek.
A wall of snapshot cards would throw that away. The two render layers map cleanly: the
**on-screen layer** is the node elements (shapes / favicons / sprites), the **ground
layer** is the underlay (edges / fields) in the WebGPU Scene.

## 5. The assembly (what plugs in)

**Reused, wasm-compiled, unchanged in essence:**

- `orrery` (graph + gyre + camera + arrangements + cartography + node-body/face), the
  same component; the browser is its new host.
- `eidetic-core` + `eidetic-opfs`, the data model + the OPFS Store. **This lane is the
  browser-side consumer that activates [Phase 7](../../eidetic_docs/implementation_strategy/2026-06-09_eidetic_deferred_phases_plan.md)**
  (gates a/b/c measured; pack small blobs).
- `genet-extract`, the capture's content half (render-free; depends only on
  `layout-dom-api`, no engine rides in).
- `glossary` (`outline_djot` / `graph_metrics`, the
  [gloss outline lens](2026-06-23_gloss_outline_lens_plan.md)), feeds the sidebar.
- `netrender`, lowers the underlay Scene to WebGPU; the relational neighborhood is
  the [relational-browse](../../archive_docs/2026-08-06_completed_plans/2026-06-23_relational_browse_graphlet_plan.md) link graph.

**New glue (the "browser as pelt" adapter, the real work):**

- A wasm entrypoint driving `orrery.frame(...)`, lowering the Scene to WebGPU/Canvas2D
  for the underlay, placing gyre-positioned **DOM elements** for the nodes, routing DOM
  events back as semantic input.
- The capture pipeline (nav → DOM grab → `genet-extract` + `captureVisibleTab` +
  favicon + provenance → eidetic node + edge).
- The MV3 extension shell, packaged per browser.

**Dropped (the browser supplies it):** genet, inker, pelt, the meerkat host,
Nova/Boa, the companion (deferred with sync).

## 6. Phases (v1, no sync)

- **P0, the portable wasm core builds baseline.** `orrery` + `eidetic-opfs` +
  `genet-extract` + `glossary` compile for `wasm32-unknown-unknown` (the `getrandom`
  `wasm_js` flag is known). **Measure the bundle size early** (like the OPFS bench);
  it is the first real gate.
- **P1, capture.** The MV3 shell + the pipeline writing nodes/edges (+ snapshots +
  extracted content) into OPFS on navigation. Value lands here even with no view.
- **P2, the gloss sidebar.** The `glossary` outline of the current neighborhood +
  recent corridor, rendered as HTML in the sidebar.
- **P3, the orrery page.** The wasm orrery as a discrete tab: favicon-body nodes over
  the underlay, select → snapshot preview, click → real tab.
- **P4, cross-browser packaging.** Chromium + Firefox from one MV3-ish codebase;
  Safari follows once the core is proven (the Xcode wrap).

## 7. Honest gates / hard parts

- **Bundle size.** orrery + eidetic + genet-extract + glossary as one baseline wasm
  must fit the extension budget. Measure at P0.
- **Where the extraction wasm runs.** Content-script CSP makes running wasm there
  awkward; grab the DOM in the content script, run `genet-extract` in the background /
  an offscreen document.
- **WebGPU-in-an-extension-page** is solid on Chromium/Gecko, newer on Safari; the
  underlay is simple enough that the Canvas2D fallback is real insurance.
- **Safari distribution** (Xcode + App Store review) is friction, not a wall; it is
  why Chromium + Firefox ship first.
- **Node customization (sprite + adjustable hitbox) is immature.** The favicon-shape
  default is in hand; the sprite/hull editing rides on the node-body/face work
  regardless of host, so this lane inherits it rather than skipping it.

## 8. Later followups (tracked, out of v1)

- **Knot.** Notetaking in the browser, the nematic knot engine / `illume` djot
  lane, so you annotate nodes and write notes *in* the graph, not just capture pages.
- **Clipping.** Web clipping, clip a selection (not just the whole page) into the
  graph / corpus, a finer capture grain.
- **Filesystem shaping for meerkat to tap into (permissioned).** OPFS is origin-
  private, so native meerkat cannot read the extension's store directly. Shape the
  store so it is *also* exportable / mirrorable to a user-granted real directory (the
  File System Access API: `showDirectoryPicker`), giving meerkat a permissioned read of
  the same eidetic layout. This is the bridge to "sync to native" without the full
  companion, and it needs a documented on-disk eidetic layout + a permission model.
- **Auto-update (crucial for native Mere too).** The delivery update is store-managed
  for extensions and service-worker-managed for a PWA, but the load-bearing question is
  **data-format migration**: an update must not strand the OPFS corpus. The
  format-versioned engram contract (Phase 9's discipline) is the lever, version the
  store, migrate or re-derive on mismatch. Native Mere has the same problem with its
  own updater, so solve it once as an eidetic concern, not a per-host one.
- **Sync to native + the companion / smolweb / p2p / federation** half of the
  [browser_extension_companion_plan](2026-06-23_browser_extension_companion_plan.md)
  remains the forward arc beyond this v1.

## 9. Grounding

- [browser_extension_companion_plan](2026-06-23_browser_extension_companion_plan.md)
  (superseded node/delivery framing; the companion/federation forward vision stands).
- [substrate parallelism brief](../../2026-06-21_substrate_parallelism_composition_brief.md)
  and the genet Nova-Memory64 lane (why running a web engine is the hard path this
  sidesteps; the actor-per-origin substrate).
- [eidetic Phase 7](../../eidetic_docs/implementation_strategy/2026-06-09_eidetic_deferred_phases_plan.md)
  (the OPFS Store this lane activates; gates measured, pack small blobs).
- [gloss outline lens](2026-06-23_gloss_outline_lens_plan.md),
  [relational browse](../../archive_docs/2026-08-06_completed_plans/2026-06-23_relational_browse_graphlet_plan.md),
  [node-body/face](2026-06-23_node_body_face_model_plan.md),
  [document-script substrate](../../archive_docs/2026-07-03_completed_plans/2026-06-21_document_script_substrate_plan.md)
  (the reused pieces).
