# Interaction Model Spine — one pipeline over the definitely-support formats

**Date**: 2026-06-18
**Status**: Canonical (technical_architecture). The single place a reader sees the whole
interaction loop end to end. It states the pipeline once and assigns each stage one owner; the
feature plans are non-overlapping layers it cross-refs. Written to stop the plan-cluster drift
(six docs circling the orrery-as-interactive-document with overlapping claims), verified against
the tree by a five-agent consolidation pass (2026-06-18).
**Code**: `crates/meerkat/` (fetch, content, render, input, window_view), `crates/orrery/`
(orrery + gyre + arrangements), `crates/inker/` (engines), `crates/graph/` (kernel + linked-data).

This doc is the **spine**; the [composition spine](2026-05-21_mere_composition_spine.md) stays
the *arrangement ontology* (forme / platen projection). Where this doc names an owner, that plan
owns the work; this doc owns only the contract between stages and the scope line.

## The model

Mere's interaction model is one pipeline, stated once over the formats we definitely support:

**fetch → render → represent → arrange → interact (+ clip + cooperate) → made-semantic**

A node is the object that represents an addressable thing in the graph. Content of any supported
format enters through one fetch actor, renders through one engine registry, is *represented* as a
pluggable node form, *arranged* by the orrery, *interacted* with (selected, dragged, clipped,
navigated, shared), and *broadcast* as kernel-authoritative semantics. The pipeline is coherent at
its data seams (one graph, one actor pool, URL identity end to end) and fragmented at its
presentation and input seams; the layers below own closing that gap.

### The definitely-support formats (the scope line)

In scope now (the model is stated over these):

- **smolweb / gemtext**: mature. The nematic engine (15 engines, `crates/inker/engines/nematic`),
  fetched + routed + rendered through the document-canvas lane.
- **djot**: wired. `DjotKnotEngine` registered in `nematic::engines()` (lib.rs:98), routed by
  `text/x-knot` (routing.rs:343); the default knot grammar (jotdown).
- **linked-data / semantic web**: mature. `crates/graph/linked-data` (JSON-LD + RDFa
  ingest/export), routed as the linked-data ingest harvest, default-off.
- **p2p / cooperative**: substrate live (murm Cable + p2panda transport + moothold tessera, in
  `comms_host.rs` / `sync.rs`); the named gaps are a shared-session and a moot-content card path.
- **local media (net-media)**: the one format that is plan-only today
  ([net_media_plan](../implementation_strategy/2026-05-26_net_media_plan.md), no crate); in scope
  as a *target*, needs the net-media crate built. AV1 / WebRTC decode internals can lag.

Deferred to the later **Genet / WPT track** (not this spine): full-web HTML via genet (the
flat-scene lane), scrying / compat WebView (the 549-LOC Windows-only self-rendering tile), the WPT
conformance grind, page-supplied CSS fidelity. Full-web rides the
[compatibility-view charter](../../verso_docs/technical_architecture/2026-06-10_compatibility_view_charter.md);
this spine treats genet / scrying only as the embedded-frame compositing case.

## The pipeline, stage by stage

### 1. Fetch (unified, no owner needed)

One off-thread fetch actor (`crates/meerkat/src/fetch.rs`), scheme-routed (http(s) → netfetcher
WHATWG Fetch; gemini / gopher / finger / spartan / nex / guppy / titan → errand), folding every
result into one `Fetched { content_type, body }`. Favicons + subresources ride the same actor.
Already unified; documented here as the entry contract, no plan needed.

### 2. Render (content-type → engine → scene)

`route_document_engine` over the inker `EngineRegistry` / `EngineRoutePolicy` + `nematic::engines()`
routes a fetched body into a lane:

- **Lane A** (document-canvas retained packet, host-queryable `link_at`): smolweb, djot,
  local-media, the rendered side of the supported formats.
- **Lane B** (genet flat scene): full-web HTML. *Later track.*
- **Lane C** (self-rendering external-texture): scrying / compat. *Later track.*

The definitely-support formats all live in Lane A plus the linked-data ingest harvest. The contract
is `content-type → engine → EngineDocument/packet → scene`. Owner: the inker engine registry
(substrate); no live plan rebuilds it.

### 3. Represent (the node's visual form)

Content-type → silhouette (`content_shape` → `NodeShape`, node_ops.rs) and activation → color are
the clean data mappings. Above them sits the **pluggable Representation** (tile / card /
textured-body / shape / scripted) and the **LOD "materialization is state" machine** (a glyph by
default, a DOM subtree on focus / expand, an underlay dot on cull). A node has up to three
representation code paths today (in-scene gnode, demoted underlay rect, host DOM card) with no
single contract; unifying them is the represent layer's job.
**Owner: [node-representation + arrangement plan](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md)**,
the Representation set and the LOD machine. The DOM-*materialization mechanism* (how a card or tile
becomes a real genet subtree under the camera) is owned by unified-document-host.

### 4. Arrange (the orrery's strongest seam)

The orrery owns a backend-free `LayoutView` read-model fed each frame from a swappable physics
source (`Physics::inline` or an off-thread armillary actor). The `crates/orrery/arrangements`
registry (phyllotaxis / radial / l-system / penrose / grid) overlays host-computed positions;
field-region couplings add localized force wells. The orrery and the tiled workbench are two
projections of one forme arrangement.
**Owners:** node-representation (the scene-wide arrangement choice + per-scene persistence);
[field-regions](../implementation_strategy/2026-06-13_scriptable_field_regions_plan.md) (the
localized / scripted arrangement scoped to a placed region). The arrangements registry is the
shared substrate; the [composition spine](2026-05-21_mere_composition_spine.md) keeps the forme /
platen projection ontology.

### 5. Interact (unified inside the orrery, fragmented above it)

Inside the orrery, interaction is already a formalized model: host-neutral semantic methods
(`pointer_down/up`, `cursor_moved`, `wheel`, `set_ctrl/shift`, `visit`), one selection set, one
camera, one set of `LayoutView` hit-tests covering nodes + edges + marquee + multi-select + edge-pick
+ field-drag. The orrery never sees a window.

At the **host level** this is the central fragmentation: a hand-written region-router (a ~550-line
`on_mouse_input` cascade, a parallel wheel cascade, a third keyboard precedence chain) routing by
testing the cursor against ~15 per-frame rect caches in `WindowView`, with no fall-through protocol
and `default_prevented()` never read. The model's job here is to **lift the orrery's contract up to
the host**: one input spine (top-down dispatch with fall-through), one focus authority, geometry
from the laid-out tree rather than re-derived rect caches.
**Owner: [tearout_composability_plan](../../archive_docs/2026-07-04_completed_plans/2026-06-19_tearout_composability_plan.md)** (continuing the archived window-composition plan),
the one input spine + one focus authority (its P2-companion), over the shell-root document that
unified-document-host Phase 1 consolidates.

- **Clip** = `tag_selected` / `assert_selected_relation` / `isolate_selection` plus the knot +
  eidetic capture path (needs a command; a named affordance riding the nematic knot work).
- **Cooperate** = murm Cable + p2panda + moothold tessera (live); the gap is a shared-session /
  moot-content card path.

### 6. Made-semantic (kernel-authoritative, one-way)

`linked_data::to_jsonld` export (`export.rs`) over the focused graph is the kernel-authoritative
broadcast (the dormant inverse of the wired ingest harvest). Once nodes are DOM (Phase 2 Path A) it
can be emitted as inline `<script type="application/ld+json">` per card. One-way: the view
broadcasts, it is never read back as a source (the
[two-natured](../research/2026-05-30_two_natured_kernel_brief.md) authority rule).

## Ownership map (the duplication resolved)

The drift was five seams each claimed by two or three plans. The cut:

| Seam | Owner | The others |
| --- | --- | --- |
| Orrery as an in-document element | unified-document-host Phase 2 | composition-spine keeps the arrangement ontology; modular-integration S1/S2 are shipped history |
| The Representation set + LOD machine | node-representation | unified-document-host references it, owns only DOM-materialization |
| Scene-wide arrangement | node-representation | field-regions owns localized/scripted; arrangements registry is the substrate |
| Localized / scripted arrangement | field-regions | node-representation owns scene-wide |
| The external-texture-input bridge | window-composition | distinct from the custom-layout `<orrery>` element (unified-document-host); two genet asks, not one |
| The two-hit-test (DOM vs gyre) boundary | unified-document-host Phase 2 | the gyre `hit_test` primitive exists; this is a boundary-doc + host-seam |

## Genet asks the model pulls (prioritized)

Revised 2026-06-19 against the
[unified-document-host](../implementation_strategy/2026-06-17_unified_document_host_plan.md) Phase 2
design pass: reading the engine showed the transform-aware hit-test already exists, so the first two
asks are not engine work, and Phase 2a then landed the host wiring (orrery cards select + focus
through the shell hit-test). The live engine asks are now image-decode and the
external-texture-input bridge.

1. **Custom-layout `<orrery>` element** with transform-positioned DOM children: deferred, not a
   gate. The gnodes already work as host-positioned `position:absolute` DOM in the shell
   document; a real `<orrery>` element only moves per-frame gnode placement from the host into the
   engine. Revisit only if host-driven transform-setting becomes a perf or correctness problem.
   (unified-document-host Phase 2, cond 1.)
2. **Transform-aware hit-test**: done in the engine. `IncrementalLayout::hit_test` →
   `GenetLaneView::walk_for_hit` (`genet-layout/genet_lane.rs:398-417`) inverse-maps the point
   through each node's `transform` and honours `pointer-events: none`. The host routes an
   orrery press through the shell hit-test first, a card hit dispatching in the document and a miss
   falling to `gyre` (`input.rs` `point_over_orrery_card`).
3. **External-texture element that bears input**: open. The content / scrying / textured-body
   bridge; `<external-texture>` is output-only today. Unblocks the content-pane input spine and the
   textured-body node form.
4. **Image-decode in the chrome / shell `IncrementalLayout` render path**: open. The session path
   passes an empty `ImagePlane` (`genet-layout/incremental.rs:738`, "Empty image planes, matching
   the scripted layout path"), so favicon `<img>` data-URIs paint nowhere on this lane, even though
   genet-layout decodes `data:` URIs inline when handed a populated plane.
5. **Key-dispatch unification through genet `dispatch_key`**: partly landed. Tab / Shift+Tab
   traversal and Enter / Space activation over the focusable set now ride `dispatch_key` (Phase 1
   cond 2); the residual is the omnibar's Enter, still hand-intercepted.

## Sequencing (tie the threads)

1. **This spine doc**: the consolidation artifact; cheap, no code, every later decision references
   it. (Carries the duplication boundary-lines into the four plans + the djot-comment fix + the
   modular-integration done-markers.)
2. **Finish unified-document-host Phase 1**: fold the separate `RosterPane` / `ListPane` runners
   into the one shell-container root, one focus ring, one a11y tree, collapse the Y-band input. The
   mechanism is proven (the 2026-06-17 spike); needs no engine work; it is the foundation the input
   spine and per-pane focus both need.
3. **In parallel (no engine dependency)**: node-representation P0 (restore the gnode's lost cues:
   shape-by-content-type, footprint, selection emphasis) and the moveable + resizable gnodes (a
   `size` on `OrreryGnode`; resize handles out of `content_rects`; size-by-degree opt-in).
4. **The window-composition focus / active-session decoupling → the one input spine** over the
   consolidated document.
5. **Then the cross-repo genet asks** for Phase 2 (the custom-layout `<orrery>` element +
   transform-aware hit-test); node-representation P1/P2 (pluggable forms, textured-body via the
   external-texture-input bridge) and the net-media crate ride here. The later WPT / full-web track
   stays parked behind the Genet charter until this definitely-support model is solid.

## Capability stack (where the product pieces ride)

The interaction model serves a bounded capability stack (Mark's framing, 2026-06-18): a
Lagrange-level browser plus spatial context, cooperative browsing, web clipping, self-hosting,
local-compute coordination, and accessibility / diagnostics / automation. This is the near-term
product, already a category of its own well before the full-web grind.

| Capability | State | Owner |
| --- | --- | --- |
| Lagrange-level smolweb browser | exists (nematic) | engine-picker work; render lane A |
| Spatial context (orrery) | exists | orrery + this spine's arrange/interact stages |
| Cooperative browsing | partial (substrate live) | comms / murm / moothold; gap = shared-session + moot-content card |
| Web clipping | pieces exist (knot + eidetic) | needs a command; nematic knot work |
| Self-hosting | tessera proven | moothold; gap = the moot community object |
| Local-compute coordination | partial (armillary + kernel wired) | actor constellation; gap = runner-mesh embed |
| Accessibility / diagnostics / automation | exists (accesskit + apparatus + uxtree + agent harness) | apparatus plan; gap = gloss-outline / A3 |
