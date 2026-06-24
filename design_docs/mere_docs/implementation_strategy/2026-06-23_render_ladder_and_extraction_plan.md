# Mere render ladder + web-extraction lane

**Date**: 2026-06-23
**Status**: Architecture aligned; no meerkat code yet. Grounds the page-JS lane as a
*rung*, not a static-path replacement, and adds the orthogonal analysis axis.
**Origin**: the page-JS scoping conversation. Page JS is mostly built in serval + pelt
(see "Findings"); before wiring it into meerkat we fix the framing so it slots into the
principled profile ladder and the "analyze the web, don't just render it" goal.
**Relates to**: serval's `docs/2026-05-12_serval_profile_ladder_plan.md` (the canonical
rung taxonomy), the
[engine picker plan](../../inker_docs/implementation_strategy/2026-06-15_engine_picker_and_pluggability_plan.md)
(the rung selector), the
[relational-browse graphlet plan](2026-06-23_relational_browse_graphlet_plan.md) and the
[eidetic browsing derivation plan](../../eidetic_docs/implementation_strategy/2026-06-12_eidetic_browsing_derivation_plan.md)
(the extraction front-end + sink), and the
[native session store plan](2026-06-23_native_session_store_plan.md) (the cookie/storage
seams the scripted rung consumes).

---

## Two axes, not one

The web is consumed along **two independent axes**. Conflating them (e.g. "just run
the JS") loses both the principled-profile discipline and the analysis capability.

1. **Render ladder** (vertical — *how much of the web stack a page is given*):
   `static → interactive → scripted → fullweb`, plus `scrying.web` (the system WebView
   fallback). A higher rung is **additive**; a lower rung is a first-class composition,
   not a degraded one.
2. **Analysis / extraction** (orthogonal — *parse and pull information, no render*): a
   crawler/scraper lane that turns fetched pages into structured data (links, text,
   metadata) for the eidetic browsing corpus and for distilling into harnessed
   models/agents (the flora / federated-LoRA lane). It rides *any* rung's DOM.

The render ladder answers "show me this page"; the extraction axis answers "tell me
what's in (and across) these pages." We want both.

---

## Findings (verified 2026-06-23)

### The render ladder is canonical, and the scripted rung is mostly built

serval's profile-ladder plan defines four principled compositions proven by their
**dependency graph**, not a runtime flag (a static page must not pull `script` / `mozjs`
into its build). The justification is **attack-surface + bundle-size + DOM-as-library**,
not wasm-safety. Capability gate: JS engine + script DOM bindings appear only at
`scripted` and `fullweb`.

| Rung | Adds | JS in dep graph |
| --- | --- | --- |
| `serval-static-html` | parse → style/layout → paint | no |
| `serval-interactive-html` | forms / focus / input / a11y | no |
| `serval-scripted` | JS engine + DOM bindings + event routing | yes |
| `serval-fullweb` | navigation / workers / storage / media / WebGL / devtools | yes |

The **scripted rung already exists and is tested**: pelt's `ScriptedDocument`
(`ports/pelt-desktop/scripted.rs`, ~40 tests across Boa + Nova) runs the classic-script
timing model (inline / `defer` / `async` / `type=module` with cross-module `import`,
SRI, charset), drives `setTimeout`/`setInterval` + microtasks + frame-cadence GC, runs
the script → layout → render loop (`frame()`), serves `getComputedStyle` off the last
cascade, and has real `addEventListener` / `dispatchEvent` with capture→target→bubble
propagation. The one gap for interactivity is the **host-input → DOM-event bridge** (a
real click/keypress → hit-test → synthesized `MouseEvent`/`KeyboardEvent` →
`dispatchEvent`), which pelt leaves as a "V4 follow-up".

### meerkat is static-only today; the rung selector already exists

meerkat renders fetched HTML through serval's **static** layout (`StaticDocument` → the
`serval_render` glue) — no `Runtime` / `BoaEngine` / `NovaEngine` (the `ScriptInstance`
in meerkat is mere's `mere:script` WIT, a different runtime). The **rung selector is
already in place**: the inker engine picker (`engine_pins` / `EngineRoutePolicy` /
`is_surface_engine`, the same mechanism the verso flip rides) pins a node to an engine.
"Render rungs of the internet" = expose the serval profiles as engine choices and pin a
node to the rung it needs. Nematic stays the protocol-faithful lane (Gemini / Gopher /
Markdown / feeds), untouched.

### The extraction primitives mostly exist

- `serval-static-dom` (`StaticDocument`) — the no-JS parse tree; the base for
  render-free extraction.
- The relational-browse plan's **rect-free anchor enumerator** over `LayoutDom` — the
  link extractor for a crawl frontier (the *one* new primitive that plan calls for;
  today's `<a href>` harvest is layout-coupled via `LinkHit` + rect).
- The **JSON-LD ingest** (kernel / linked-data) — structured-data extraction into the
  graph (remote `@context`s resolved offline).
- The verso donor's `ScriptedDom::outer_html` / `form_values` — DOM / form text
  extraction.
- `eidetic_browsing_derivation` — the dataset sink (it parks a serval-side
  text-extraction seam as a named trigger); the relational-browse graphlet is the
  front-end that feeds it.

pelt's `headless.rs` is a GPU-free *render* to a scene snapshot (reftests), which is
close to but not the same as extraction — extraction wants the DOM / text, not the
paint list.

---

## Architecture

### A. The render ladder in meerkat = engine-picker rungs

Expose the serval profiles as engine ids the picker can pin (alongside the existing
`scrying.web`):

- `serval.static` (default) — the current path; fast, deterministic, **no JS in its
  dep graph** (the ladder's witness discipline holds in meerkat too).
- `serval.interactive` — forms / focus / a11y, still no JS (may fold into static
  initially).
- `serval.scripted` — pelt's `ScriptedDocument`; the page-JS rung.
- `serval.fullweb` — later; the broad browser surface.
- `scrying.web` — the system-WebView fallback (already wired; the verso flip target).

Default to the lowest rung that serves the page; escalate by pin (user choice) or,
later, an origin policy (a per-site rung default — scripted for known web apps,
static-by-default for safety + speed). The picker is the single selection point.

### B. The scripted rung integration (page JS)

Port pelt's `ScriptedDocument` into meerkat's content actor as the `serval.scripted`
profile:

- The content actor builds a `ScriptedDocument<E>` (parse → run scripts → `frame()`)
  for a node pinned to `serval.scripted`, instead of the static `StaticDocument` render.
- Wire the seams meerkat already has: the `ResourceFetcher` (external scripts /
  subresources) → meerkat's fetch actor; `CookieProvider` / `StorageProvider` → the
  session jar + an eidetic-backed storage store (this is where the native-session-store
  6a/6b seams light up).
- Drive `pump()` + `frame()` per redraw; `has_pending_work()` keeps timer-driven pages
  animating.
- Threading: the `Runtime` is `!Send`, so it lives on the content-actor thread (one per
  document); the providers go over the `Send + Sync` session jar, storage durability via
  the actor → host channel (the cookie-persist pattern).

### C. The input → event bridge

The event machinery exists; wire host input to it: on a click / key over a
`serval.scripted` tile, hit-test the laid-out DOM, synthesize the DOM event, and
`dispatchEvent` at the target (capture→target→bubble already works). This is what makes
the scripted rung *interactive* (buttons, forms, SPAs). Serval/host-side, bounded.

### D. The extraction lane (analyze without rendering)

A profile that **parses and extracts, with no cascade/layout/paint**, feeding eidetic +
distillation. Orthogonal to the render ladder, and able to ride any rung's DOM:

- **static-parse extract** — `StaticDocument` → extractors (anchors, headings, main
  text, JSON-LD / microdata / OpenGraph, forms). The cheap, fast, automatable crawl
  path; no render.
- **headless-scripted-DOM extract** — run the `serval.scripted` rung to mutate the DOM,
  then extract the *post-JS* DOM, still no paint. This is how SPAs / JS-rendered content
  get scraped.

Output flows to the eidetic browsing corpus (the relational-browse graphlet is the
interactive front-end; `eidetic_browsing_derivation` is the dataset derivation), which
grounds personal intelligence and the flora distillation lane. Crawl concerns
(frontier: depth / fan-out cap, per-host politeness, robots) belong here, per the
relational-browse V2 scope.

### Invariants

- **Static stays JS-free in its dep graph** — the ladder's witness discipline, in
  meerkat as in serval. A higher rung is additive, never a mutation of a lower one.
- **Render ladder ⊥ extraction axis** — extraction is not "a lower render rung"; it is a
  different output (data, not pixels) that can draw from any rung's DOM.
- **The engine picker is the single rung selector** — no parallel rung-selection path.
- **Nematic is untouched** — the protocol-faithful lane stays separate from the HTML
  rungs.
- **Extraction feeds eidetic + distillation**, never a side silo.

---

## Phases

1. **Rung taxonomy** — *(1a done 2026-06-23)* the ladder is now a first-class concept in
   inker's engine vocabulary: `ServalRung` (`Static` / `Interactive` / `Scripted` /
   `FullWeb`, `Ord` by capability) + the rung engine ids (`serval.web` stays the static
   id for pin compatibility; `serval.scripted` / `serval.interactive` / `serval.fullweb`
   are the higher rungs) + `serval_rung` / `is_serval_rung` classifiers. Higher rungs are
   registry-gated, so a pin to an unimplemented rung falls back to static (tested). *(1b
   remaining)* surface the available rungs in the meerkat picker — folds into phase 2,
   since a rung only becomes pickable once registered.
2. **Scripted rung** — port `ScriptedDocument` into the content actor (B): fetcher +
   cookie/storage providers + the `pump`/`frame` loop + per-document runtime lifecycle.
   Lights up `document.cookie` / `localStorage`. The big integration chunk.
3. **Input → event bridge** (C) — interactive scripted rung. *(done 2026-06-24.)* A
   click now flows pointer → hit-test → dispatch → listeners → re-render, end to end:
   - **serval `Runtime::dispatch_event(raw_id, type) -> bool`** (`cf44ec4`): dispatches a
     synthetic DOM event at a host-supplied node with full capture→target→bubble
     propagation, returning `false` when a listener called `preventDefault`. Wiring: a
     `__reflectNode` native (raw `NodeId` → canonical pinned reflector) plus a
     `__dispatchSynthetic` global defined *inside* the DOM bootstrap (where the
     IIFE-local `wrapNode` is in scope) bridges reflector → `wrapNode` → `dispatchEvent`.
     The earlier "snag" was a stale build + a missing `wrapNode` call, not an eval-scope
     limit — `set_function` is `register_global_callable`, so natives are global. Tested
     on Boa + Nova.
   - **pelt `ScriptedDocument::click_at`** (`b157af1`): hit-tests the live DOM, dispatches
     the click (listeners may mutate the tree), then applies the in-page anchor-nav
     default action only when not `preventDefault`-ed. The hit-test session drops before
     dispatch (which re-enters the host mutably); the default-action layout rebuilds after.
   - **meerkat**: `ContentCommand::ScriptedClick` + `Constellation::click_scripted` /
     `is_scripted` (`f58ab47`), and input.rs routes a left click on a `serval.scripted`
     card/tile to the page, consuming it like a link click (`f277afc`). All gated behind
     the `scripted` feature; the default JS-free build is unchanged.

   Region-level refinement (which parts of a scripted tile are interactive vs. a
   drag-handle for the orrery) and keyboard-event dispatch (`keydown`/`keyup` via a
   `KeyboardEvent`) are the natural follow-ons; the dispatch entry is event-type-generic,
   so `key` is a thin addition once a `KeyboardEvent` shape lands in the bootstrap.
4. **Extraction profile** (D) — static-parse extractors → eidetic first; then
   headless-scripted-DOM for SPAs. Wire to relational-browse + eidetic derivation.
   *(slices 1–3 done 2026-06-24: the `serval-extract` crate.)* The render-free
   extraction **primitive** now exists in serval — `serval-extract`, depending only on
   `layout-dom-api` (the dep graph is the witness: no cascade/layout/paint can creep
   in). It walks any `LayoutDom` (a no-JS `StaticDocument` or a script-mutated DOM
   alike) into a `PageExtract`: the **rect-free anchor enumerator** (the plan's named
   primitive — `href` + text + `rel`, unresolved, the crawl frontier's source), the
   `<title>`, declared **metadata** (`description` / canonical / OpenGraph), the
   `<h1>`–`<h6>` **outline**, and the full **visible text** (script/style/head
   excluded). 12 tests. **Still open**: (a) the home/wiring decision — how the extract
   surfaces to meerkat + eidetic (a `serval.extract` engine? a side-output on every
   serval render? the existing `Contribution` path that already carries JSON-LD
   harvest?); (b) headless-scripted-DOM extract (run the scripted rung, extract the
   *post-JS* DOM — same `extract()` over a `ScriptedDom`); (c) a main-content/readability
   pass over the visible text; (d) the crawl frontier (depth/fan-out cap, politeness,
   robots) per relational-browse V2.
5. **Refinements** — script-added stylesheets, retain-until-dirty layout, origin rung
   policy, and the web-API long tail (Canvas2D / WebSocket / fetch-driven re-render;
   serval already has a WebGL factory seam) as pages demand — standards by standards.

---

## Phase 2 design (grounded in the content actor, 2026-06-23)

Reconnoitred the slot-in; the build is now mechanical, in slices:

- **Reuse, don't reimplement.** `pelt-desktop` re-exports `ScriptedDocument` /
  `ScriptedEngine` / `LoadedDocument` / `LocalFetcher`, and `pelt-core` exports
  `ResourceFetcher`; meerkat already depends on both. The scripted rung holds a
  `pelt_desktop::ScriptedDocument<E>` (the whole tested script-loading + timers + GC +
  `frame()` loop), not a hand-rolled one.
- **Where it lives.** meerkat's content actor (`constellation::Activation`, off-thread on
  the pool) is an armillary actor that holds `!Send` state on its own thread (like the
  fetch actor's tokio runtime), so a `ScriptedDocument` (which owns a `!Send` `Runtime`)
  lives there fine. The actor's `render()` already special-cases two paths (the
  `mere:script` mutable `ScriptedDom`; the static `StaticDocument` via
  `scene_from_content_band`); the scripted rung is a third: hold an
  `Option<ScriptedDocument>`, build it on `Show` when the routed engine is
  `serval.scripted`, and emit `frame()`'s `Scene` (and `pump()` per drive for timers).
- **Three lanes, not one registry.** The actor's `EngineRegistry` holds the *nematic*
  document engines; `serval.web` (static HTML, the `StaticDocument` → `scene_from_content_band`
  path) and `scrying.web` (the surface/ScryingHost path) are **special-cased lanes**, not
  registry engines. So `serval.scripted` is a *fourth lane*, special-cased like the static
  one. The one host↔actor touch-point: the host owns the pin (`engine_pins`), so it must
  signal the actor to take the scripted lane — either thread the routed engine id into the
  `Show` command, or a dedicated rung field. (The actor's own policy is only for
  content-type re-routing; the pin is host-side.)
- **Availability + picker (1b).** Making `serval.scripted` `is_available` (so the pin
  resolves past phase 1a's fallback, and the picker surfaces it) is a host-side routing
  concession, not a registry `register()` — the scripted lane is special-cased, not an
  `EngineDocument` impl.

### Slices

- **2a — inline scripts render.** *(built 2026-06-23, `0213ee7`)* On `Show`, a
  `serval.scripted` node builds `ScriptedDocument::parse(body)` (inline `<script>`s
  run); `render()` frames the live document. A fourth content-actor lane, additive (the
  static / WIT-script / nematic lanes are untouched). Behind the meerkat `scripted`
  cargo feature so the base build links no JS engine (the witness discipline; the
  `boa_engine` patch mirrors serval's and only satisfies resolution — `script-engine-boa`
  is optional, so boa stays out of the default compile graph). The host threads the
  routed engine through `constellation.drive` → `Show`; `serval.scripted` is present +
  pickable (context menu / settings / apparatus), gated on the feature. Both
  `cargo check -p meerkat` and `--features scripted` green; the scripted variant **links**
  (no image-size-limit hit). **Verified** by two feature-gated tests driving the real
  off-thread content actor: a page whose only text is injected by an inline `<script>`
  renders glyph runs through the `serval.scripted` lane (JS ran + the mutated DOM
  rendered), while the same page on the static lane paints none (the control proves the
  glyphs came from the JS). *(No external scripts, no providers yet — 2b / 2c.)*
- **2026-06-23 (phase 2b)**: external `<script src>` works (`208b014` + serval `e20a5ce`).
  A `ScriptFetcher` (pelt `ResourceFetcher` over the actor's blocking `ContentNetFetcher`)
  feeds external scripts through the routing fetch (session jar + SSRF floors); serval's
  new `ScriptedDocument::from_body` runs the host's already-fetched body and fetches only
  the scripts (no document re-fetch). Deterministic test (mock fetcher → external-script
  text renders). Next: 2c (cookie/storage providers light up `document.cookie` /
  `localStorage`), then phase 3 (input→event bridge for interactivity).
- **2026-06-23 (phase 2c)**: `document.cookie` over the session jar (`435985d` + serval
  `a8e7cae`). The scripted rung installs a `JarCookieProvider` (origin-scoped, over the
  process session jar) on the document before scripts run, so a page's JS shares the
  exact cookies HTTP uses — the cookie convergence the native-session-store plan named.
  HttpOnly hidden from script; a JS write persists like `Set-Cookie`. `localStorage`
  works via serval's in-memory default. Deterministic test green; default build
  untouched (all gated). The scripted rung now runs real pages with shared sessions.
  Next: phase 3 (input→event bridge — interactivity) and durable storage (6b host impl).
- **2b — external scripts.** *(built 2026-06-23)* Resolved the fetch-model gap with the
  actor's existing blocking fetch: a `ScriptFetcher` (pelt `ResourceFetcher` over
  `script::ContentNetFetcher`, which `block_on`s the routing fetch — so scripts ride the
  session jar + the same SSRF/scheme floors as `net.fetch`). A new serval
  `ScriptedDocument::from_body(html, fetcher, base_url)` (serval `e20a5ce`) runs the
  *host-supplied* body and fetches only the external `<script src>` — no document
  re-fetch (unlike `load`). `build_scripted` uses it; verified by a deterministic test
  (a mock fetcher supplies the script; the injected text renders). Both scripted tests
  green under `--features scripted`.
- **2c — `document.cookie`.** *(built 2026-06-23)* pelt's `ScriptedDocument::from_body`
  now takes an optional `CookieProvider`, installed on the runtime *before* scripts run
  (serval `a8e7cae`). meerkat passes a `JarCookieProvider` scoped to the node's origin
  over the **process session jar** — so a page's JS reads/writes the *same* cookies HTTP
  does (the cookie convergence; native session store 6a). HttpOnly is hidden from script
  (spec); a JS write marks the jar dirty so it persists like an HTTP `Set-Cookie`.
  Deterministic test (a JS write lands in the jar). `localStorage` already works via
  serval's in-memory default; **durable eidetic-backed, partitioned storage** (6b's host
  impl, `StorageProvider` over eidetic) is the follow-up.

The input→event bridge (phase 3) is separate; the extraction lane (phase 4) reuses the
parsed (and optionally scripted) DOM.

## Open questions

- **Per-document runtime lifecycle**: create / destroy a `ScriptedDocument` as nodes
  come and go in the content actor; cost of many live scripted tiles vs. static ones.
- **Extraction profile's home**: a serval profile, a meerkat content-actor mode, or its
  own inker engine? Leaning toward a content-actor mode that reuses the parse + (optional
  scripted) DOM, with the extractors as a shared library.
- **Origin rung policy**: a per-site default rung (and who authors it — user setting,
  mod manifest, a community list) so the common case is not a manual pin.
- **Crawl autonomy**: where the frontier/scheduler lives (a dedicated crawl actor, per
  relational-browse V2) and how it shares the fetch + session substrate without
  polluting the interactive session's jar.

---

## Progress

- **2026-06-23**: plan created from the page-JS-as-rung conversation. Grounded in
  serval's profile-ladder plan (the four-rung taxonomy + dep-graph-witness discipline)
  and the verified state: serval + pelt already implement the scripted rung (the gap is
  the input→event bridge); meerkat is static-only today; the engine picker is the rung
  selector; the extraction primitives (`StaticDocument`, the anchor enumerator, JSON-LD
  ingest, the verso donor extractors, the eidetic derivation sink) largely exist. No
  meerkat code yet — this fixes the framing before the integration.
- **2026-06-23 (phase 1a)**: the rung taxonomy landed in inker — `ServalRung` (ordered
  by capability) + the higher-rung engine ids + `serval_rung` / `is_serval_rung`
  classifiers, the tier-1 counterpart to `is_surface_engine`. `serval.web` stays the
  static rung's id (pins persist). The registry-gated fallback is tested: a pin to an
  unregistered higher rung (e.g. `serval.scripted` before it ships) routes to static, so
  the ladder can be referenced before its rungs are implemented. 27 routing tests green.
- **2026-06-23 (phase 2a)**: the scripted render rung is built (`0213ee7`). A
  `serval.scripted` node runs its page's inline JS (`ScriptedDocument::parse` on Boa) and
  renders the mutated DOM — a fourth, additive content-actor lane behind the meerkat
  `scripted` feature (base build stays JS-free; boa is an optional dep + a resolution-only
  patch). Picker surfacing (1b) landed with it. Compiles both configs, the scripted
  variant links (the image-size limit is not hit), 73 lib tests green. Runtime
  verification pending (a running meerkat instance held the exe; needs a headed pass to
  pin + load an inline-JS page). Next: 2b (external scripts) and 2c (cookie/storage
  providers, which light up the native-session-store seams).
- **2026-06-24 (phase 3 — input → event bridge, done)**: the scripted rung is now
  interactive end to end — pointer → hit-test → dispatch → listeners → re-render.
  serval gained `Runtime::dispatch_event(raw_id, type) -> bool` (`cf44ec4`): a
  `__reflectNode` native (raw `NodeId` → canonical pinned reflector) + a
  `__dispatchSynthetic` global defined inside the DOM bootstrap (where the IIFE-local
  `wrapNode` lives) bridge reflector → `wrapNode` → `dispatchEvent`, returning `false`
  on `preventDefault`. (The prior "eval-scope" snag was a stale build + a missing
  `wrapNode` call; `set_function` is `register_global_callable`, so natives are global.)
  pelt's `ScriptedDocument::click_at` (`b157af1`) hit-tests + dispatches + applies the
  anchor-nav default only when not prevented. meerkat added `ContentCommand::ScriptedClick`
  + `Constellation::click_scripted` / `is_scripted` (`f58ab47`) and routes left clicks on
  a `serval.scripted` card/tile to the page, consumed like a link click (`f277afc`).
  Tested at every layer (serval Boa+Nova `dispatch_event_fires_a_listener`; pelt
  `click_dispatches_to_script` + `prevent_default_blocks_anchor_nav`; meerkat
  `scripted_rung_click_dispatches_to_script`). Default JS-free build verified unchanged.
  Next: keyboard-event dispatch (`KeyboardEvent` shape + a `key` path — thin, the entry
  is event-type-generic), interactive-region refinement, and phase 4 (the extraction lane).
- **2026-06-24 (phase 4 — extraction primitive, slices 1–3)**: the render-free
  extraction lane's foundation is built — the `serval-extract` crate (serval `04d1927`
  / `648325b` / `067e9d6`). It walks any `LayoutDom` into a `PageExtract` with no
  cascade/layout/paint: the rect-free anchor enumerator (the crawl frontier's link
  source — `href`/text/`rel`, unresolved), the `<title>`, declared metadata
  (`description` / canonical / OpenGraph), the `<h1>`–`<h6>` outline, and full visible
  text (script/style/head excluded). Witness-clean: the crate depends only on
  `layout-dom-api` → `markup5ever`, so no render stack can creep into the extraction
  axis (verified via `cargo tree`). 12 tests over `StaticDocument`. The keyboard-event
  follow-on from phase 3 is **not** thin after all — serval has no `KeyboardEvent` /
  `activeElement` / focus model, so it needs a focus model first (deferred).
  **Open decision before wiring**: extraction's home/surface — a `serval.extract`
  engine, a side-output on every serval render, or the existing `Contribution` path
  (which already carries JSON-LD harvest to the kernel/eidetic). An ecosystem-shape call
  for Mark. Then: headless-scripted extract (`extract()` over a post-JS `ScriptedDom`),
  a readability pass, and the crawl frontier.
