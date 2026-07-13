# W3C/WHATWG standards as architecture: a mining review

**Status (2026-07-05):** research brief, broadened same-day from a
chrome-skewed first pass to cover the full browser platform. Mines the
standards corpus for architectural, design, and feature recommendations
across genet, xilem_serval, netrender, and mere/meerkat. Browser shipping
statuses were web-verified 2026-07-05 (sources at the end); spec-shape
claims are from the specs. Extends the method of
`genet/docs/2026-06-12_viewport_root_standards_scope.md` (the viewport/root
family: "when a rule turns up missing, look for its siblings") from one
family to the whole platform.

**The thesis:** the knockout strategy deletes W3C surface for later rebuild.
Every rebuild should land as the spec's mechanism (the standing rule: fix in
the engine as the standard's shape, never a host hack), pulled by a named
consumer, with WPT subsets as the done-condition. Each family below gets one
of three postures:

- **ADOPT**: use the standard's shape as internal architecture now, even
  before any content-facing API exists.
- **PULL**: stays knocked out until the named consumer needs it; land it
  spec-shaped when pulled.
- **SKIP**: deliberately dead, on the record, with the reason.

Part I is the platform spine (the crucial parts of a browser: loading,
security, parsing, script, storage, media, text). Part II is the chrome and
product families. The spine sections are mostly ADOPT because they are
cheap as internal shapes now and brutally expensive as retrofits.

---

# Part I — the platform spine

## S1. Loading pipeline: Fetch is the spine — ADOPT

**Specs:** WHATWG Fetch, Streams, URL, Encoding, MIME Sniffing.

Fetch is not "the HTTP client API"; it is the algebra the rest of the
platform is specified against. Requests carry a *destination* (document,
script, style, image, font...), a mode (cors, no-cors, navigate), a
priority, and an initiator; CSP, mixed content, SRI, service workers,
preload, and referrer policy are all specified as patches on the Fetch
algorithm. An engine whose loader is not Fetch-shaped re-derives each of
those as a special case.

**Recommendations:**
- Genet's resource loading becomes one internal `Request → Response`
  pipeline with destinations, modes, and priorities from day one, even
  while the only callers are the HTML parser and the image loader. Every
  later security feature (S2) then has its specified hook point.
- Bodies are Streams end to end (WHATWG Streams semantics: backpressure,
  tee for the preload scanner + parser split). Buffering policy stops
  being invented per call site.
- MIME sniffing per the spec, not by extension: the sniffing standard
  exists because content-type lies are an attack surface.
- URL is already rust-url (the WHATWG algorithm); Encoding rides `encoding_rs`
  (Gecko's own). Both are solved-by-crate; the recommendation is only to
  never bypass them.
- Nematic stays out of scope: smolweb protocols only; genet owns HTML
  lanes. But murm/moot sync transports should still speak Fetch shapes at
  the boundary where they hand documents to the engine.

## S2. Security model — ADOPT (the most expensive retrofit on this list)

**Specs:** HTML origin + agent clusters; Secure Contexts; CSP Level 3;
Referrer Policy; Subresource Integrity; COOP/COEP/CORP; Permissions +
Permissions Policy; Trusted Types (Baseline since Feb 2026); Sanitizer API
(Firefox 148 shipped first, Feb 2026; Safari positive).

Every engine that retrofitted origins, partitioning, or process isolation
paid a multi-year tax. The stack is pre-content today, which is exactly
when this family is cheap.

**Recommendations:**
- **Origin is a kernel-grade type**, not a string: scheme/host/port plus
  opaque origins, with agent clusters as the "may share memory" boundary
  and site (registrable domain) as the isolation grain. The
  cross-platform-parallelism brief already hit one consequence (COOP/COEP
  forking the PWA lane from the open-web lane); this is the general form.
- **CSP enforcement points come free if S1 landed**: CSP is specified as a
  Fetch patch plus parser/eval hooks. Put the hook points in even while
  the policy is "allow all".
- Secure-contexts gating as a bit on the environment, because half of all
  new APIs are spec-gated on it (WebGPU, storage, workers...).
- Permissions as one spine (the capability-gate catalogue is mere's richer
  superset; the W3C Permissions model should be a projection of it, not a
  second registry), with Permissions Policy as the per-frame delegation
  shape when iframes arrive.
- Trusted Types + Sanitizer: now Baseline/near-Baseline. When the scripted
  DOM lands, `setHTML` + sanitizer semantics should be the *first* sink
  API implemented, not the last; it is the modern default and it is
  drastically simpler to make `innerHTML` the legacy alias than the
  reverse.
- SRI and referrer policy: small, land with the loader.

## S3. HTML parsing — ADOPT (mostly solved-by-crate, two shapes to keep)

**Specs:** WHATWG HTML parsing (tree construction, error recovery, quirks,
fragment parsing), plus the speculative/preload-scanner pattern (not
spec-mandated, universal in practice).

html5ever carries the algorithm; quirks mode already landed (2026-06-11).
The shapes worth deliberate architecture:

- **Streaming parse off the network stream** (S1's tee): the document
  builds incrementally, layout can start before EOF. Large-document
  behavior on live cards depends on this more than on raw parser speed.
- **The preload scanner as a first-class concept**: a second, stateless
  scan of the byte stream that discovers subresource fetches early. It is
  the single biggest real-page loading win engines have, and it needs S1's
  priorities to schedule what it finds.
- `document.write` intercepting the parser: quirks-lane at most, per the
  SKIP list. Fragment parsing (`innerHTML`) arrives with the scripted DOM
  and is spec-defined; no invention needed.

## S4. Document and frame model — ADOPT

**Specs:** WHATWG HTML navigables (the post-2022 rewrite of browsing
contexts), traversable navigables, session history entries; Navigation API
(Baseline Jan 2026: Chrome, Edge, Firefox 147, Safari 26.2).

The spec's frame tree is a tree of *navigables*, each holding a series of
session history entries, with one *traversable* root owning joint history
traversal. That is a formally-debugged model of "a window of nested
documents with history", including the hard cases (iframe history,
cross-document traversal, bfcache eligibility).

**Recommendations:**
- Model meerkat's frametree and genet's future iframes on the navigable
  tree. A content card is a navigable; a gnode's session dimension is its
  entry series; the window's traversable is the graph session's history
  spine.
- Map graph traversal onto Navigation API semantics
  (`NavigationHistoryEntry`, intercept, traverseTo) so `window.navigation`
  later becomes a thin projection over machinery mere already runs.
- **Done-condition when pulled:** the `html/browsers/` WPT subtrees for
  navigation and session history on claimed slices.

## S5. Scripting and execution — ADOPT the shapes, PULL the engine surface

**Specs:** the HTML event loop (task sources, microtask checkpoint,
rendering opportunities); ES modules + import maps (Baseline); workers,
worklets, and the agent model; WebAssembly (core 3.0 era: GC Baseline since
late 2024; memory64 in all engines except Safari as of Feb 2026; JSPI in
Chrome 137+/Firefox 139+, Safari open); WebIDL.

**Recommendations:**
- **WebIDL is the binding architecture.** Every platform API is specified
  as IDL fragments; engines generate bindings rather than hand-writing
  them. The scripted-DOM lane (Nova web lane, Boa fallback) should be
  WebIDL-first from the start: a codegen path from IDL to engine calls.
  This is precisely the asset a build-up-from-Boa stack lacks and the
  thing Servo's script crate encodes; keep it through the carve-down even
  while `script` sits in workspace.exclude.
- The event loop's task-source vocabulary is the scheduling model for the
  runner (see C7): tasks, microtask checkpoint, then a rendering
  opportunity. Cheap discipline now, mandatory when pages share the loop
  with chrome.
- Workers before iframes on the script roadmap: the agent model (worker =
  own event loop, own realm, message-passing) matches armillary's actor
  shape, and most modern pages assume workers exist.
- **The wasm64 receipt:** memory64 is now everywhere but Safari. The vano
  fork's wasm64 + snapshot-clone identity is aligned with where the
  platform actually went; note it as a validated bet, and track JSPI
  (suspend/resume across the JS boundary) as the piece that would let
  synchronous-looking guest code drive async engine APIs.
- Import maps + module resolution are spec-complete and small; land them
  with the first script milestone rather than as a follow-up, since module
  loading exercises S1's destinations and priorities.

## S6. Service workers, Cache API, PWA family — PULL (consumer: PWA lane)

**Specs:** Service Workers, Cache API, Web App Manifest, Push/Notifications,
Background Sync (Chromium-only bits vary).

A service worker is specified as a Fetch interceptor running in a worker:
if S1 and S5 land as recommended, SW is a composition, not a subsystem.

**Recommendations:**
- Ordering: Cache API before Service Workers (it is a plain
  request/response store and useful to the engine's own HTTP cache
  discipline), SW only when the PWA lane demands offline.
- The browser/PWA split already governs scripting doctrine; extend it
  here: the *open-web* lane treats SW as a page capability to implement
  faithfully; the *product's own* offline story (mere as a PWA, moot
  sync) should not be built on SW machinery internally; it has eidetic
  and the sync tiers.
- Push: platform-dependent and product-visible; defer until a real
  consumer, and note iOS constraints (installed-PWA-only) make it a
  product decision, not an engine one.

## S7. Cookies and state partitioning — ADOPT (partition-by-default)

**Specs:** RFC 6265bis cookies; storage partitioning by top-level site;
CHIPS (`Partitioned` attribute); Storage Access API as the unpartition
gate; FedCM at the horizon.

The 2026 landscape: unpartitioned third-party state is effectively dead
(Safari blocks, Firefox partitions by default via Total Cookie Protection,
Chromium via CHIPS + Storage Access). New engines get to skip the entire
deprecation saga.

**Recommendations:**
- **Partition all site state by (top-level site, embedded origin) from the
  first line**: cookies, storage, cache, network state. Never implement
  the unpartitioned model even as an interim step.
- Storage Access API is then the *only* unpartitioning mechanism, gated
  through the permissions spine (S2), which is also where mere's
  user-facing privacy story gets its receipts (real state, not a shield
  icon performance).
- Cookie store itself is solved-by-crate territory at the HTTP layer, but
  the partition key must live in the store's schema, not be bolted onto
  lookups.

## S8. Media — PULL, with two ADOPT-shaped seams

**Specs:** image formats + async decoding; canvas 2D + OffscreenCanvas;
HTMLMediaElement + MSE; WebCodecs (near-Baseline: Safari 26 full, Firefox
desktop 130+); WebAudio; EME; WebRTC; WebGPU (W3C) / WebGL.

**Recommendations:**
- **WebCodecs is the codec seam shape.** Whatever decodes media internally
  (symphonia, rav1d, image-rs, platform decoders) should sit behind a
  WebCodecs-shaped interface: decoder/encoder objects, frame/chunk types,
  codec-string capability queries. Then exposing the API to content later
  is a projection, the burn/field lanes get frames in a uniform type, and
  Strophe's audio pressure-vessel can share the seam's vocabulary.
- Image pipeline first (already exists in part): async decode off the
  layout thread, `decoding`/`loading` attribute semantics,
  `contain-intrinsic-size` interplay with C5's rendering skip.
- `<video>` playback: pull when a live-card consumer demands it, in
  stages: basic playback → MSE. **EME/DRM is SKIP-for-now on the record**:
  it is a licensing program (Widevine/PlayReady) more than a spec; a
  knockout browser without DRM video is a coherent product stance until
  proven otherwise.
- WebRTC: defer; and keep the boundary clean: murm owns the product's p2p
  lane natively; WebRTC would only ever be a *content-facing* API, never
  the substrate murm rides on.
- **WebGPU exposure is the natural graphics API for genet** (the engine
  is already wgpu; wgpu upstream tracks the W3C spec). WebGL is the
  legacy tax: defer it, on the record, and let WebGPU-first be a stated
  identity of the browser. Canvas 2D lands earlier (it is chrome-useful
  and spec-stable), backed by netrender.

## S9. Fonts, text, and i18n — ADOPT (already half-adopted by crate choice)

**Specs:** CSS Fonts (the font matching algorithm), CSS Font Loading API,
WOFF2, Encoding, plus the Unicode algorithms the specs defer to: UAX9
(bidi), UAX14 (line breaking), UAX29 (segmentation).

**Recommendations:**
- The font stack (post font-intern/blob-cache work) should name the spec's
  concepts: the *font matching algorithm* per CSS Fonts §5 (family →
  style → weight → stretch cascade with per-character fallback), a font
  source abstraction that can host `@font-face` + the Font Loading API's
  promise surface when pulled.
- Text: parley is the layout engine (the middlenet decision holds);
  the recommendation is to keep the Unicode annexes as the named contract
  (UAX29 already explicitly cited in xilem_serval's word motion; do the
  same for bidi and line-break sources) so text behavior is auditable
  against the standard rather than against "what parley did".
- Encoding: `encoding_rs` at the network/parse boundary only; everything
  internal is UTF-8, per the Encoding standard's own model.

## S10. Forms — PULL (consumer: UA widgets lane, C4)

**Specs:** HTML form submission algorithm, constraint validation API,
`autocomplete` semantics, form-associated custom elements (via
`ElementInternals`, C4).

**Recommendations:** the form *controls* plan lives in C4 (UA widgets as
xilem_serval views). The engine-side family here is submission (a Fetch
composition: form data serialization → request with destination) and
constraint validation (a per-control validity state + a document-level
algorithm). Both are small once S1 and C4 exist. Autofill is a product
feature with a spec-shaped anchor (`autocomplete` tokens); mere's version
can be graph-native (identity/persona data as nodes) while honoring the
token vocabulary.

## S11. Real-time transport — PULL (small), one boundary note

**Specs:** WebSocket (stable), Server-Sent Events (trivial over S1),
WebTransport (Baseline Mar 2026: Safari 26.4 closed the gap), Fetch
response streaming.

**Recommendations:** SSE and fetch-streaming come nearly free with S1's
Streams. WebSocket is a contained, stable pull when a content consumer
appears. WebTransport is newly Baseline and is the interesting one: QUIC
streams + datagrams. Same boundary rule as WebRTC: content-facing API
only; murm's native transports are murm's business, though WebTransport's
model (multiplexed reliable + unreliable lanes) is worth reading as
*technique* for murm's own design (borrow-from-mature-libs rule).

## S12. Performance, timers, observability — ADOPT the throttling model

**Specs:** High Resolution Time, Performance Timeline (resource/navigation/
event timing, LoAF), timer clamping/throttling for non-active documents,
`scheduler.postTask` (Chromium shipped; other engines unverified).

**Recommendations:** the spec's *timer throttling tiers for non-fully-active
documents* are the other half of C7's card-freeze model: glance-LOD cards
get spec-legal throttled timers, not a bespoke pause. Resource timing hooks
should be designed into S1's pipeline (they are Fetch annotations), because
mere's own perf tracing (the tracing-reach plan) wants the same
measurement points; one instrumentation spine, two consumers. Reduce
timer/`performance.now()` resolution per spec guidance when cross-origin
isolation is absent (ties to S2's COOP/COEP fork).

## S13. Process and crash isolation — ADOPT the boundaries, defer the split

**Specs/practice:** site isolation (origin/site as process grain),
sandboxing, the crashed-frame contract (a dead iframe does not kill the
window).

**Recommendations:** the daemon-split brief already holds the process
question open; this family says: whatever the process count, draw the
*boundaries* at spec grains now (site for isolation, agent cluster for
shared memory), and give live cards the crashed-frame contract: a card's
engine failure renders a sad-tab card, never a dead window. That contract
is also what makes multi-engine multiplexing (scry/graft/weld behind
`SurfaceEngine`) honest: an engine is a replaceable, crashable unit behind
a surface.

---

# Part II — chrome and product families

(The product ideas that motivated several of these families, including the
three that deliberately go beyond the platform, are recorded in the
companion [xilem_serval directions brief](2026-07-05_xilem_serval_directions_brief.md).)

## C1. State-preserving reparenting — ADOPT (already converged)

DOM `moveBefore()` (Chrome 133, Firefox 144; Safari is the Baseline
blocker): an insertion that does not reset state (focus, animations,
iframe documents, popover state). `BoxTree::graft_subtree` is this
contract at the layout tier; align genet's DOM-level reparenting with
`moveBefore` semantics and steal its WPT tests as the splice-survival
regression suite. xilem_serval keyed views reparent via the move path,
never remove+insert. **Platform gap, on the record:** there is no standard
for *cross-document* state-preserving moves (`adoptNode` resets state);
the tear-out trichotomy deliberately goes beyond the platform there;
record the divergence in the tear-out brief.

## C2. Style system as the LOD and identity substrate — ADOPT

Container size + `style()` queries (size Baseline since 2023; style
queries: Chromium 111+, Safari 18+, Firefox landing 2026 under Interop
2026); cascade layers; `@scope`; custom properties; `color-scheme` /
`light-dark()`.

- **LOD is a container style query, not a private mechanism**: the canvas
  sets `--lod` on a gnode's container; sheets restyle glance/card/full via
  `@container style(--lod: glance)` (illustrative, not compile-ready).
- **Node identity rides the cascade**: the orrery NODE_SHEET becomes
  custom properties (`--node-hue` and kin) at each representation's root,
  so representations carry node identity by inheritance, not discipline.
- **Chrome styling is cascade-layered from day one**: `@layer ua, chrome,
  extension, user;` gives user theming a spec-defined priority story (the
  configurability principle, in CSS). `@scope` is the lighter isolation
  tool where a shadow root is too much.

## C3. Top layer and anchored overlays — PULL (consumer: meerkat chrome, near)

Top layer; Popover API (Baseline); CSS Anchor Positioning (Baseline 2026:
Chrome 125+, Firefox 147, Safari 26); Custom Highlight API (cross-browser
since Firefox 140, Jun 2025); `::target-text`.

- Implement the top layer as a first-class per-document rendering concept
  (sibling of the viewport object in the 2026-06-12 scope doc). Palette,
  omnibar dropdown, context menus, dialogs all become top-layer entries;
  xilem_serval's `overlay.rs` migrates onto it instead of growing a
  parallel system.
- Anchor positioning is the standard shape for chrome attached to a
  content node (hover cards, annotation handles). Overlay-roots decompose
  into top-layer element + `position-anchor` + UA shadow isolation.
- Ranged highlight painting behind `::selection`, `::highlight()`,
  `::target-text`, `::spelling-error` is one engine mechanism and is the
  spec-shaped exit from the in-the-wings audit's §5 pitfall (find-in-page
  and selection blocked by baked page textures).
- **Done-condition:** popover + anchor + custom-highlight WPT subsets;
  meerkat's palette and find-in-page ride them.

## C4. Components and UA widgets — PULL (consumer: form controls lane)

Shadow DOM incl. declarative + serializable roots (Baseline); custom
elements; `ElementInternals` + form-associated custom elements (Baseline);
customizable `<select>` (`appearance: base-select`: Chrome 135; Safari TP /
Firefox Nightly; not Baseline).

- Rebuild knocked-out form controls **as xilem_serval control views**
  serving as UA shadow content: one implementation for the omnibar and
  every page `<input>`. Build them `base-select`-shaped so the
  customization API is nearly free later.
- `ElementInternals` is the contract checklist (form layer +
  accessibility) even before any custom-elements API exists.
- Serializable shadow roots give snapshots a standard format: chrome cards
  and static page cards serialize to declarative-shadow-DOM HTML that
  round-trips through genet itself.

## C5. Rendering pipeline — ADOPT containment; PULL transitions

CSS containment; `content-visibility` + `contain-intrinsic-size`
(Baseline); View Transitions (same-document broad, Firefox recent;
cross-document Chromium 126+ / Safari 18.2+, Firefox flagged); the WebGPU
canvas-context model; CSS Grid L3 "grid lanes" masonry (Safari 26 first;
spec churning: `item-flow`, `item-tolerance`).

- **`content-visibility` is the standard name for card virtualization**:
  skippable rendering subtrees with placeholder sizing. Attack the
  live-card scene-churn whale (92-260 ms re-rasters) through this lever
  plus C7's freeze; netrender's per-surface tile caches are its
  raster-tier twin.
- **View transitions are the card-morph system**: summon and navigation
  morphs are snapshot-old/snapshot-new/animate-named-pairs, and genet
  already owns snapshots at the netrender boundary. One choreography
  system for canvas and future page transitions.
- **The canvas element model is the external-surface seam**: a `<canvas>`
  whose texture is supplied by an external wgpu producer (WebGPU
  canvas-context shape) is the standards-correct way to embed the graph
  canvas, scry/graft/weld surfaces, or burn output in a document; device
  and queue sharing follow the WebGPU model rather than a bespoke handle.
- Masonry/grid-lanes: watch; `item-flow` semantics still moving.

## C6. Input and editing — ADOPT EditContext's shape

Pointer Events + pointer capture (Baseline); EditContext (Chromium-only;
Firefox positive-with-concerns); Selection API; async Clipboard.

- Canvas drag, `gs::TileTab` drag, and marquee selection use pointer
  capture semantics internally: the debugged answer to "drag left the
  element", composing with future content pages competing for the pointer.
- **EditContext is the architecture even where the API is not Baseline**:
  editable region decouples buffer + IME composition from DOM mutation;
  the app owns the buffer, the platform owns composition. xilem_serval's
  `TextInput` already made that split; shape genet's IME integration as
  an internal EditContext so a rebuilt contenteditable later sits on the
  same object.
- Selection lands with C3's highlight painting (selection is a highlight
  range plus editing anchors).

## C7. Lifecycle and scheduling — ADOPT

The event-loop discipline lives in S5. The product-facing halves:

- **bfcache's "fully active" distinction is the live-card freeze model**:
  a glance-LOD card is a non-fully-active document (timers throttled per
  S12, media paused, snapshot shown), restored on focus like a bfcache
  hit. Real pages already must survive this lifecycle; freezing distant
  cards is spec-legal behavior.
- **The graph is a better speculation oracle than heuristics**: prerender
  neighbor nodes using the prerendering spec's internal shape (separate
  navigable, deferred activation, `document.prerendering`). Chrome
  guesses from hover; mere knows adjacency. Internal-first; no
  speculation-rules JSON needed.

## C8. Observers — PULL (thin projections)

MutationObserver consumes the same `DomMutation` stream xilem_serval
writes eagerly (one stream, three consumers: chrome replay/undo, sync, the
web API). ResizeObserver rides incremental layout's retained planes;
IntersectionObserver is the API face of the canvas LOD visibility
computation. Order by script-consumer demand; design the internal streams
now so each is a projection. Mutation *events* (DOM2) are SKIP: removed
from Chromium in 2024.

## C9. Storage buckets and multi-window — ADOPT shapes, PULL APIs

Storage Standard (buckets, quota, eviction) as the internal vocabulary for
the *site-data* tier (S7's partitioned store), so content-facing
IndexedDB/OPFS become projections later; eidetic keeps the engram tier.
Multi-window synced panels stay structural (one runner projecting into N
windows); where processes split, BroadcastChannel + structured clone is
the interchange shape, and the real-sync principle gets receipts from the
mutation log (which mutations reached which window), never a spinner.

## C10. Accessibility — ADOPT

WAI-ARIA + HTML-AAM + AccName: because chrome *is* DOM, one derivation
(DOM semantics → AAM/AccName → AccessKit) serves toolbar and page alike.
C4's controls declare semantics via `ElementInternals`. This is a place
the one-engine bet pays concretely: two-stack architectures do this work
twice.

---

## Method: WPT and WebDriver BiDi — ADOPT

- **WPT subsets are the done-condition currency** for every PULL above
  (the viewport scope doc's V3 harness, generalized). A knockout is
  "deferred, on the record"; a rebuild is "these WPT directories pass on
  the claimed slice".
- **WebDriver BiDi is the automation seam** for the meerkat verify harness
  (the capture harness is dead in partitioned mode; a BiDi-shaped
  screenshot/eval channel is the durable replacement and what WPT itself
  runs on).

## The SKIP list (deliberate, on the record)

- Mutation events, `document.write` in the hot parser path, sync XHR:
  quirks lane at most.
- AppCache, WebSQL, Portals, Shadow DOM v0, HTML Imports: dead upstream.
- Houdini custom paint/layout worklets: stalled across vendors; view
  transitions + custom highlights absorbed the use cases.
- Fenced frames / Topics / ads-coupled machinery: Chromium-only,
  off-mission.
- XSLT: keep knocked out; upstream deprecation momentum, near-zero pull.
- EME/DRM: skip-for-now as a product stance (licensing program, not a
  spec); revisit only against a real consumer.
- WebGL: deferred in favor of WebGPU-first identity (S8).
- MathML Core: defer; Baseline exists, so a future pull has a clean target.

## Priority order (consumer-pulled)

Spine first where it is retrofit-expensive, chrome where the consumer is
already waiting:

1. **S1 + S2 skeleton** (Fetch-shaped loader, origin type, partition key,
   hook points): pre-content is the only cheap moment.
2. **C2 style substrate** (LOD, identity, layers): unblocks canvas forms
   and theming; mostly Stylo plumbing.
3. **C3 top layer + anchor + highlight painting**: palette, menus,
   find-in-page, selection (the §5 pitfall exit).
4. **C1 moveBefore alignment**: names and tests splice survival; cheap.
5. **C5 content-visibility + C7 freeze/throttle** (with S12): the
   live-card churn whale, spec-shaped.
6. **C4 UA widgets as views** (+ S10 validation/submission): the form
   controls lane already queued for woodshed pressure.
7. **S4 navigable/history model**: before iframes or scripted navigation,
   so the frametree never needs a rewrite.
8. **S5 script lane** (WebIDL-first bindings, event loop, modules,
   workers; wasm64/JSPI tracking): the long pole; start with the binding
   codegen decision.
9. **S8 media seams** (WebCodecs-shaped codec API, image pipeline), S6
   Cache API, S11 transports, C8 observers: as consumers arrive.

## Verified-status sources (2026-07-05)

- moveBefore: [MDN](https://developer.mozilla.org/en-US/docs/Web/API/Element/moveBefore),
  [web-features explorer](https://web-platform-dx.github.io/web-features-explorer/features/move-before/)
  (Chrome 133, Firefox 144; Safari blocks Baseline).
- Anchor positioning: [MDN](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/position-anchor),
  [OddBird](https://www.oddbird.net/2025/10/13/anchor-position-area-update/)
  (Baseline 2026).
- Container style queries: [MDN](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Containment/Container_size_and_style_queries),
  [web.dev May 2026](https://web.dev/blog/web-platform-05-2026).
- Customizable select: [Chrome blog](https://developer.chrome.com/blog/a-customizable-select),
  [caniuse](https://caniuse.com/mdn-css_properties_appearance_base-select).
- EditContext: [MDN](https://developer.mozilla.org/en-US/docs/Web/API/EditContext_API),
  [caniuse](https://caniuse.com/mdn-api_editcontext) (Chromium-only).
- View transitions: [MDN](https://developer.mozilla.org/en-US/docs/Web/API/View_Transition_API),
  [CSS-Tricks](https://css-tricks.com/cross-document-view-transitions-part-1/).
- Navigation API: [web.dev](https://web.dev/blog/baseline-navigation-api),
  [InfoQ](https://www.infoq.com/news/2026/05/navigation-api-browser/)
  (Baseline Jan 2026).
- Custom Highlight API: [MDN](https://developer.mozilla.org/en-US/docs/Web/API/CSS_Custom_Highlight_API)
  (cross-browser since Firefox 140).
- Grid lanes / masonry: [CSS Grid L3](https://www.w3.org/TR/css-grid-3/),
  [Chrome masonry update](https://developer.chrome.com/blog/masonry-update),
  [CSS-Tricks](https://css-tricks.com/masonry-layout-is-now-grid-lanes/).
- WebCodecs: [caniuse](https://caniuse.com/webcodecs),
  [MDN](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API)
  (Safari 26 full; Firefox desktop 130+; near-Baseline).
- Wasm memory64/GC/JSPI: [webassembly.org features](https://webassembly.org/features/),
  [State of WebAssembly 2026](https://platform.uno/blog/the-state-of-webassembly-2025-2026/)
  (memory64: all but Safari; GC: Baseline late 2024; JSPI: Chrome 137+ /
  Firefox 139+).
- Trusted Types / Sanitizer: [MDN Trusted Types](https://developer.mozilla.org/en-US/docs/Web/API/Trusted_Types_API)
  (Baseline Feb 2026), [MDN Sanitizer](https://developer.mozilla.org/en-US/docs/Web/API/HTML_Sanitizer_API),
  [Firefox 148 ships Sanitizer](https://cyberpress.org/mozilla-releases-firefox-148/).
- Cookies/partitioning: [MDN Storage Access API](https://developer.mozilla.org/en-US/docs/Web/API/Storage_Access_API),
  [MDN State Partitioning](https://developer.mozilla.org/en-US/docs/Web/Privacy/Guides/State_Partitioning),
  [cookiestatus.com Safari](https://www.cookiestatus.com/safari/).
- WebTransport: [webrtc.ventures](https://webrtc.ventures/2026/04/webtransport-is-now-baseline-what-it-means-for-real-time-media/),
  [websocket.org](https://websocket.org/comparisons/webtransport/)
  (Baseline Mar 2026, Safari 26.4).
- `scheduler.postTask` status outside Chromium: left unverified, flagged
  inline (S12).
