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

1. **Rung taxonomy in the picker** — register `serval.static` (default) /
   `serval.scripted` (and stubs for `interactive` / `fullweb`) as engine ids the picker
   pins; keep the static path JS-free. Small; mostly inker + meerkat routing.
2. **Scripted rung** — port `ScriptedDocument` into the content actor (B): fetcher +
   cookie/storage providers + the `pump`/`frame` loop + per-document runtime lifecycle.
   Lights up `document.cookie` / `localStorage`. The big integration chunk.
3. **Input → event bridge** (C) — interactive scripted rung.
4. **Extraction profile** (D) — static-parse extractors → eidetic first; then
   headless-scripted-DOM for SPAs. Wire to relational-browse + eidetic derivation.
5. **Refinements** — script-added stylesheets, retain-until-dirty layout, origin rung
   policy, and the web-API long tail (Canvas2D / WebSocket / fetch-driven re-render;
   serval already has a WebGL factory seam) as pages demand — standards by standards.

---

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
