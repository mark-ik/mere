> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-18 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/graphshell_docs/technical_architecture/2026-04-16_middlenet_lane_architecture_spec.md`
> - Source commit: `401e2fcc`
>
> It is filed here as research, not as a plan and not as a statement of where
> Mere is going. It speaks the donor vocabulary throughout, all of it retired or
> reassigned per `TERMINOLOGY.md`: *Graphshell* here is the old browser product
> that was absorbed into Mere (the name was reclaimed 2026-07-22 for the remote
> projection host, a different thing), *Verse* is retired in favour of Mere at
> network scope, *Verso* is the peer runtime, and *Middlenet* is now *Nematic*.
> Its relative links point at donor docs that no longer exist locally.
>
> Read the implementation note dated 2026-04-20 near the top with care. It is
> the one part of this file that reports landed code rather than intent, and the
> crates it reports (`middlenet-core`, `middlenet-adapters`, `middlenet-render`,
> `middlenet-engine`) exist nowhere in the current tree. That slice lived and
> died in the donor repository.
>
> Cross-references this recovery closes:
> `turnstone/design_docs/2026-04-16_smolnet_capability_model_and_scroll_alignment.md`,
> recovered earlier today, names this spec for its lane split and async
> rendering lifecycle. Its baseline `2026-03-29_middlenet_engine_spec.md` is
> recovered beside it in this folder in the same pass, and the HTML-lane
> contract it defers to is at
> `turnstone/design_docs/2026-04-16_smolweb_compliance_and_middlenet_html_contract.md`.
>
> The three-lane split survived as an idea and not as this crate layout. The
> Blitz-DOM-painted-through-a-WebRender-fork lane was never built; HTML at any
> rendering depth is Genet's job, and the direct document lane became nematic's
> engines over `inker`.

---

<!-- This Source Code Form is subject to the terms of the Mozilla Public
     License, v. 2.0. If a copy of the MPL was not distributed with this
     file, You can obtain one at https://mozilla.org/MPL/2.0/. -->

# Middlenet Lane Architecture Spec

**Date**: 2026-04-16
**Status**: Active architectural spec with partial implementation
**Scope**: Define the crate split, lane model, async lifecycle, and shared host
plumbing for a protocol-first Middlenet engine that can choose between a
direct portable document renderer, an HTML lane built from Blitz's DOM/style/
layout stack but painted through Graphshell's WebRender fork, and full-web
fallback.

**Related docs**:

- [`2026-03-29_middlenet_engine_spec.md`](2026-03-29_middlenet_engine_spec.md)
  — baseline Middlenet scope, portable-engine framing, and intermediate
  document model direction
- [`2026-03-30_protocol_modularity_and_host_capability_model.md`](2026-03-30_protocol_modularity_and_host_capability_model.md)
  — host-aware degradation and protocol packaging classes
- [`2026-02-18_universal_node_content_model.md`](2026-02-18_universal_node_content_model.md)
  — node as persistent content container independent of renderer
- [`../implementation_strategy/viewer/universal_content_model_spec.md`](../implementation_strategy/viewer/universal_content_model_spec.md)
  — viewer routing and content selection policy
- [`../implementation_strategy/viewer/viewer_presentation_and_fallback_spec.md`](../implementation_strategy/viewer/viewer_presentation_and_fallback_spec.md)
  — fallback/degraded-state expectations
- [`../research/2026-04-10_ui_framework_alternatives_and_graph_tree_discovery.md`](../research/2026-04-10_ui_framework_alternatives_and_graph_tree_discovery.md)
  — vello/parley/AccessKit direction and custom-canvas strategy
- [`../research/2026-04-14_wasm_portable_renderer_feasibility.md`](../research/2026-04-14_wasm_portable_renderer_feasibility.md)
  — portable renderer feasibility and host/WASM constraints
- [`../../verso_docs/research/2026-04-16_smolnet_capability_model_and_scroll_alignment.md`](../../verso_docs/research/2026-04-16_smolnet_capability_model_and_scroll_alignment.md)
  — capability-model framing for smolnet protocols, Scroll's UDC alignment,
  and the recommended transport-plus-format adapter split

---

## Implementation Note (2026-04-20)

The first extraction slice described by this spec is now landed in the
repository.

Implemented now:

- `middlenet-core` exists and owns the canonical semantic document/source
  model.
- `middlenet-adapters` exists and adapts Gemini, Gopher, Finger/plain text,
  Markdown, RSS, Atom, and JSON Feed into semantic documents.
- `middlenet-render` exists and produces a backend-neutral Direct Lane scene
  model for semantic documents.
- `middlenet-engine` now acts as a facade for source detection, adaptation,
  lane choice, and Direct Lane render packaging.
- `viewer:middlenet` on native desktop now renders through
  `PreparedDocument -> RenderScene` instead of the old Middlenet DOM/display
  list scaffolding.
- the old `dom.rs`, `style.rs`, `layout.rs`, `compositor.rs`, `script.rs`, and
  `viewer.rs` modules in `middlenet-engine` are no longer part of the default
  compile path and are gated behind `legacy-scaffolding`.

Still deferred:

- `middlenet-html`
- `middlenet-servo`
- `graphshell-gpu`
- async streamed `DocumentDelta` plumbing beyond the batch-compatible carrier
- shared host cache/offscreen/frame plumbing

One design adjustment surfaced during implementation: `RenderRequest`,
`RenderScene`, and hit-test carriers live in `middlenet-render` for v1, while
lane-selection carriers such as `PreparedDocument`, `HostCapabilities`, and
`LaneOverride` live in `middlenet-engine`. That keeps `middlenet-core` smaller
and purely semantic.

---

## 1. Problem Statement

Graphshell's Middlenet goal is broader than "lightweight HTML rendering."
MiddleNet covers protocol-faithful documents across Gemini, Gopher, Finger,
feeds, Markdown, static HTML, article-mode HTTP, observation cards, previews,
and archived/offline content.

The architecture therefore must not collapse around one renderer's internal DOM.

The design problem is:

- preserve a protocol-first document model,
- support a very portable direct render lane,
- allow a richer HTML/CSS lane where it materially helps,
- keep full web fallback available,
- avoid forcing every surface to pay the cost of the heaviest engine,
- and make the async/streaming lifecycle explicit instead of pretending
  rendering is a synchronous function call.

This spec names that solution the **lane architecture**.

---

## 2. Core Invariants

### 2.1 Semantic document truth is canonical

Graphshell MUST preserve a renderer-independent canonical Middlenet document
representation.

That canonical representation:

- MUST be suitable for protocol-faithful rendering,
- MUST carry provenance and source metadata,
- MUST be portable across native and WASM hosts,
- MUST remain valid even if a renderer crate is replaced.

Blitz DOM, Servo DOM, and any renderer-specific tree are **derived working
representations**, not canonical truth.

### 2.2 Lanes are replaceable execution strategies

A lane is a rendering/execution strategy selected for a request. Lanes MUST be
modular and host-aware. No single lane may define Middlenet as a whole.

### 2.3 Host plumbing is shared

Font resolution, image decode/cache, accessibility projection, offscreen
rendering, hit-testing metadata, and texture/export surfaces SHOULD be shared
across lanes where practical. Renderer-specific internals MAY differ, but the
host-facing contracts SHOULD converge.

### 2.4 Servo remains a complement, not a failure

Selecting the Servo lane for full-web or JS-heavy content is an intended part of
the architecture, not an escape hatch that invalidates Middlenet.

---

## 3. Lane Model

Graphshell SHOULD support at least three execution lanes plus one faithful-
source presentation mode.

### 3.1 Direct Lane

The **Direct Lane** renders canonical Middlenet documents without treating HTML
as the center of the universe.

Primary use cases:

- hover previews
- Verse search results
- observation cards
- Gemini/Gopher/Finger rendering
- feeds and Markdown
- offline/archived document viewing
- highly portable WASM targets

Typical stack:

- `middlenet-core` semantic document
- `middlenet-render`
- `parley`
- `vello`
- optional `anyrender`, but only for offscreen / CPU fallback / preview
  plumbing rather than as an app-wide render coordinator

The Direct Lane MUST NOT grow a DOM/CSS/layout/script mini-browser. Its path is
canonical semantic document -> text/layout model -> portable scene.

### 3.2 HTML Lane

The **HTML Lane** renders static-ish HTML/CSS content through a richer HTML/CSS
engine when that yields materially better fidelity than the Direct Lane.

Primary use cases:

- article-mode HTTP
- captured HTML clips
- archived static pages
- richer local publication/document surfaces
- static observation-card details that benefit from CSS layout

Typical stack:

- `middlenet-core`
- `middlenet-html`
- `blitz-dom`
- `blitz-html`
- `blitz-traits`
- `Stylo`
- `Taffy`
- `Parley`
- Graphshell's `WebRender` fork for display-list submission and GPU compositing

The HTML Lane MUST remain optional. It is a Middlenet lane, not the definition
of Middlenet itself.

The HTML Lane MUST NOT treat Blitz's default paint/backend stack as
architectural law. Graphshell is explicitly choosing Blitz for the integrated
DOM/style/layout work, while retaining its own paint/compositing direction.

### 3.3 Servo Lane

The **Servo Lane** handles content that needs a full browser engine.

Primary use cases:

- live modern web pages
- authenticated surfaces
- JS-heavy or app-like sites
- layout/API cases beyond Middlenet's intended scope

### 3.4 Faithful Source Mode

The **Faithful Source Mode** preserves source-faithful fallback where structural
adaptation is misleading, unsupported, or user-disabled.

Primary use cases:

- raw gophermap
- exact gemtext/source view
- parse failures
- diagnostics and archival fidelity

Faithful Source Mode is a presentation mode of the Direct Lane, not a fourth
renderer implementation with its own backend stack.

---

## 4. Crate Responsibilities

The lane architecture should be expressed as a crate split.

| Crate | Responsibility | Canonical? |
|---|---|---|
| `middlenet-core` | Semantic document model, provenance, metadata, canonical link/action model | Yes |
| `middlenet-adapters` | Gemini/Gopher/Finger/Markdown/feed/plain-text/article adapters into `middlenet-core` | Yes |
| `middlenet-render` | Direct Lane renderer plus render-scene/request carriers for canonical Middlenet documents | No |
| `middlenet-html` | HTML lane adapter: Blitz DOM/style/layout integration plus WebRender translation | No |
| `middlenet-servo` | Servo-backed lane adapter and capability bridge | No |
| `graphshell-gpu` | Shared device/font/image/offscreen/frame-scheduling host plumbing | No |
| `middlenet-engine` | Facade/orchestrator: source detection, lane scoring, selection, fallback, host integration | No |

### 4.1 `middlenet-core`

`middlenet-core` SHOULD own:

- canonical semantic document tree
- canonical source/provenance metadata
- link/action model
- title/snippet extraction
- observation-card rendering payloads
- canonical trust/diagnostics model
- batch-compatible streamed carrier types such as `DocumentDelta`

It MUST NOT depend on Servo, Blitz, egui, or host-native viewers.

### 4.2 `middlenet-adapters`

`middlenet-adapters` SHOULD own:

- protocol parsing for Gemini/Gopher/Finger
- feed parsing
- Markdown/plain-text parsing
- source detection
- article/readability extraction
- HTML import into canonical semantic structures where appropriate

Adapters MUST output `middlenet-core` data.

### 4.3 `middlenet-render`

`middlenet-render` SHOULD own:

- Direct Lane scene derivation
- render request/scene carriers
- text layout
- paint model
- link geometry and hit-testing metadata
- offscreen thumbnail/preview generation
- optional CPU/GPU backend choice

`anyrender` MAY be used here as a backend abstraction layer. If adopted, it is
host/render plumbing, not canonical truth.

It MUST NOT become a staging area for HTML DOM, CSS cascade, or browser-like
layout machinery.

### 4.4 `middlenet-html`

`middlenet-html` SHOULD own:

- sanitized/static HTML ingestion into Blitz's DOM/style/layout stack
- `html5ever` / `blitz-html` population of the integrated DOM
- style resolution and layout through the Blitz top-half integration
- translation of the laid-out tree into Graphshell's WebRender display-list
  vocabulary
- extraction of hit-test/link/selectable metadata back into Graphshell
  contracts

It MUST NOT redefine Middlenet's canonical document model.
It MUST NOT assume Blitz's paint backend or shell crate are part of the core
architecture.

### 4.5 `middlenet-servo`

`middlenet-servo` SHOULD own:

- Servo delegation for complex content
- lane-specific capability probes
- rendering handoff contracts for node viewers/panes
- policy checks for when a request must escalate to full browser behavior

### 4.6 `graphshell-gpu`

`graphshell-gpu` SHOULD own:

- shared `wgpu::Device` / queue / surface-or-offscreen lifecycle where
  applicable
- font registry and fallback lookup
- image decode/cache
- offscreen worker pool and thumbnail queue discipline
- frame scheduling / present coordination across lanes
- texture/surface handoff contracts shared by shell, Middlenet, and future
  graph backends

This crate is the natural home for cross-lane renderer orchestration that does
not belong in `middlenet-engine` itself.

### 4.7 `middlenet-engine`

`middlenet-engine` SHOULD remain the facade visible to the rest of Graphshell.

It SHOULD expose:

- source detection
- lane scoring and selection
- explicit lane override API
- shared fallback behavior
- host-capability routing

### 4.8 Dependency Boundaries

The dependency boundaries SHOULD remain explicit:

| Crate | May depend on | Must not depend on |
|---|---|---|
| `middlenet-core` | `std`, `serde`, core data-model utilities | Servo, Blitz, egui, iced, Vello, WebRender |
| `middlenet-adapters` | `middlenet-core`, protocol parsers (`html5ever`, Markdown/feed parsers, etc.) | Servo, WebRender, host UI crates |
| `middlenet-render` | `middlenet-core`, `parley`, `vello`, optional `anyrender` | DOM/CSS/browser stacks, Servo |
| `middlenet-html` | `middlenet-core`, Blitz top-half crates, Graphshell WebRender fork | `blitz-paint`, `blitz-renderer-vello`, `blitz-shell`, host chrome |
| `middlenet-servo` | `middlenet-core`, Servo fork | Blitz renderer stack, host chrome |
| `graphshell-gpu` | `wgpu`, caches, shared font/image/offscreen infrastructure | canonical content semantics |
| `middlenet-engine` | all lane crates behind feature/envelope gates | hard-coded dependence on one mandatory lane in every envelope |

---

## 5. Canonical Types

The exact final data shapes may evolve, but the architecture assumes the
following carrier classes.

### 5.1 `SemanticDocument`

Canonical Middlenet document truth.

Must represent:

- headings
- paragraphs
- links/actions
- lists
- code/preformatted blocks
- quotes
- separators/sections
- metadata slots for title, summary, timestamps, provenance

It MAY later widen to richer inline formatting and embedded media descriptors,
but it should start from protocol-faithful document primitives, not browser DOM
maximalism.

### 5.2 `PreparedDocument`

The engine facade needs a post-adaptation carrier before lane choice.

`PreparedDocument` SHOULD carry:

- canonical source kind
- semantic document
- provenance
- trust state
- adaptation diagnostics
- raw source availability or raw source handle
- adaptation metadata such as body length / streaming state

In the current implementation this carrier lives in `middlenet-engine`, not in
`middlenet-core`.

### 5.3 `RenderRequest`

A Direct Lane render-time request carrier.

Should include:

- viewport width and height
- scale factor
- preview/detail mode
- theme tokens
- optional font/image resolver handles

Lane-selection inputs such as host capability and explicit lane override SHOULD
remain outside this type. In the current implementation they live beside
`PreparedDocument` in `middlenet-engine`.

### 5.4 `DocumentDelta`

Adapter output SHOULD be streamable rather than all-at-once.

`DocumentDelta` is the conceptual unit of streamed update between adapters and
the engine.

It SHOULD cover:

- appended source bytes / decoded text chunks
- semantic document insert/replace/remove operations
- metadata updates (title, snippet, timestamps, provenance)
- resource discovery (images, stylesheets, linked assets)
- parse warnings / degradation markers

Adapters SHOULD expose an async stream of `DocumentDelta` values rather than
only a single final document payload.

### 5.5 `HostCapabilities`

Describes what the current host can do.

Should include:

- native vs WASM vs WASI
- GPU/CPU rendering availability
- offscreen image export availability
- accessibility surface availability
- embedded web-engine availability
- network policy / offline mode

### 5.5 `RenderLifecyclePhase`

Rendering lifecycle MUST be explicit.

The minimum phases SHOULD be:

- `Started`
- `Partial`
- `Complete`
- `Invalidated`
- `Failed`
- `Cancelled`

These phases describe the evolving render session as content arrives, layout
changes, images resolve, or background work is aborted.

### 5.6 `RenderOutput`

Host-facing lane output.

Should include:

- a handle to the current display list, scene, texture, or viewer surface
- hit-test regions
- link/action map
- accessibility projection payload
- diagnostics payload
- fallback explanation if degraded
- lifecycle state / phase
- update notification channel or equivalent invalidation hook

`RenderOutput` MUST be understood as a live session handle, not a guarantee that
the lane has already finished fetching, parsing, laying out, and painting
everything synchronously.

---

## 6. Lane Selection Contract

Lane choice MUST be deterministic, explainable, and overridable.

### 6.1 Selection phases

1. Detect source/content kind.
2. Build canonical `PreparedDocument`.
3. Score available lanes against prepared document + host capabilities.
4. Apply explicit user override if valid.
5. Choose best lane.
6. Build lane-specific render request if the chosen lane needs one.
7. If chosen lane fails, walk fallback chain with recorded reason.

### 6.2 Baseline precedence

The default preference order SHOULD be:

- Direct Lane for canonical Middlenet documents and preview/archive surfaces
- HTML Lane for static HTML/CSS or richer article/captured surfaces
- Servo Lane for full-web requirements
- Faithful Source Mode when fidelity or parse conditions require it

Exact scoring may vary, but the user-visible result MUST be explainable in terms
of content needs and host capability.

### 6.3 Override model

Each node/document SHOULD support:

- `Auto`
- `Direct`
- `Html`
- `Servo`
- `Source`

If an override is impossible on the current host, Graphshell MUST explain why.

### 6.4 Example routing rules

- Gemini/Gopher/Finger → Direct by default, Source optional
- Markdown/feed/plain text → Direct by default
- captured static HTML/article mode → HTML preferred, Direct acceptable fallback
- observation card / search result card / hover preview → Direct preferred
- JS-required or authenticated page → Servo
- raw protocol/source inspector → Direct with Faithful Source Mode

---

## 7. Shared Host Plumbing

### 7.1 Shared Services Boundary

The following concerns SHOULD be host-shared rather than lane-owned wherever
possible, ideally through `graphshell-gpu` plus closely-related host-side
service modules:

- font registry and fallback
- image decode/cache
- texture allocation or surface handoff
- offscreen render-to-image
- link/action hit-testing contract
- selection/hover metadata contract
- accessibility tree projection
- diagnostics and render-failure reporting
- thumbnail and preview cache

### 7.2 `anyrender` Role

`anyrender` is useful only in a narrow role:

- one scene API
- GPU or CPU rendering backends
- live rendering and image-buffer rendering from the same abstraction

If used, `anyrender` should sit below the Direct Lane and preview/offscreen
subsystems, not above canonical document truth and not as the app-wide
render-scheduling layer.

`anyrender` is NOT the abstraction that coordinates Servo, Direct Lane, and the
HTML Lane together.

### 7.3 Envelope-Dependent Compilation

Not every deployment envelope should carry every lane.

Baseline guidance:

| Envelope | Expected lanes |
|---|---|
| Native desktop app | Direct + HTML + Servo + Faithful Source Mode |
| Native headless / thumbnail worker | Direct + HTML + Faithful Source Mode; Servo optional |
| Browser extension / PWA / browser-hosted WASM | Direct + Faithful Source Mode; HTML Lane usually omitted; Servo unavailable |
| WASI / offline utility / export worker | Direct + optional HTML depending on host GPU/runtime envelope |

`middlenet-engine` SHOULD compile lanes conditionally per envelope instead of
assuming one maximal build everywhere.

### 7.4 Background Rendering

Preview cards, hover previews, Verse search snippets, and thumbnail generation
are background work.

The architecture SHOULD assume:

- offscreen rendering jobs run on a worker pool or background queue
- preview work can be deprioritized relative to visible-pane rendering
- caches may satisfy requests without waking the full lane
- job cancellation is normal rather than exceptional

### 7.5 Concurrency Model

The concurrency model MUST be named explicitly.

- Fetch is async. `middlenet-adapters` perform network or file I/O and therefore
  do not synchronously return finished documents in the general case.
- Adapter output SHOULD be an async stream of `DocumentDelta` values rather than
  a single final result.
- lane jobs MUST carry cancellation support so closed panes, abandoned previews,
  and scrolled-off background jobs can stop early.
- Rendering lifecycle MUST expose explicit phases:
  `Started -> Partial -> Complete`, with `Invalidated`, `Failed`, and
  `Cancelled` as side paths.
- Frame scheduling across Direct, HTML, and Servo lanes is a host concern and
  SHOULD live in shared host plumbing rather than inside one lane crate.
- Hosts MUST have a way to observe visual invalidation from a lane and schedule
  a present on the next appropriate frame.
- For lanes with incremental resource discovery or late updates, layout and
  paint are not a one-shot transaction; updated visual state lands on later
  compositor frames.

---

## 8. Current `middlenet-engine` Mapping

The current repository now contains the first real lane split rather than only
the seeds of one.

| Current file | Future home |
|---|---|
| `document.rs` | moved to `middlenet-core` |
| `source.rs` | moved to `middlenet-core` |
| `adapters.rs` | moved to `middlenet-adapters` |
| `engine.rs` | now the `middlenet-engine` facade |
| `viewer.rs` | no longer default-built; Direct Lane host contract lives in `middlenet-render` plus shell-side viewer code |
| `dom.rs`, `style.rs`, `layout.rs`, `compositor.rs` | gated behind `legacy-scaffolding`; still candidates for retirement or future `middlenet-html` carve-out |
| `script.rs` | gated behind `legacy-scaffolding`; not a seed for a generic Middlenet mini-browser |

### 8.1 Architectural correction

The main correction this spec makes is:

- do not grow the current DOM/style/layout/script scaffolding into one monolithic
  Middlenet mini-browser,
- instead split it into lane-specific modules behind a canonical semantic
  document core.

More concretely:

- the Direct Lane path is `SemanticDocument -> Parley/Vello scene`
- the HTML Lane path is `html5ever/blitz-dom -> Stylo -> Taffy -> laid-out tree -> WebRender display list`
- Servo remains the full-browser lane rather than a failure mode of the others

That preserves the Middlenet dream while avoiding a misleading "HTML engine
first" center of gravity.

---

## 9. Surface Mapping

The lane architecture should map to Graphshell surfaces as follows.

| Surface | Preferred lane | Notes |
|---|---|---|
| Hover preview | Direct | Fast, deterministic, offscreen-friendly |
| Verse search result card | Direct | Observation-card and snippet-native |
| Observation-card detail | Direct, optional HTML | HTML only if richer local styling adds value |
| Offline/article archive | HTML, fallback Direct | Static HTML/CSS lane is attractive here |
| Feed reader / Markdown viewer | Direct | Protocol/document-first |
| Live website pane | Servo | Full browser engine required |
| Raw protocol/source inspector | Direct + Source Mode | Fidelity/debug view |

This "pick your lane" model is a feature, not an implementation accident.

---

## 10. Non-Goals

This spec does not require:

- replacing Servo
- forcing all content through the HTML Lane
- forcing HTML as Middlenet's canonical document model
- immediate support for full browser interactivity in portable/WASM lanes
- locking the project to any single rendering backend abstraction
- adopting Blitz's paint backend when Graphshell already has a stronger
  WebRender direction for HTML

---

## 11. Recommended Execution Order

1. Stabilize and widen `SimpleDocument` into a canonical `SemanticDocument`.
2. Extract `middlenet-core` and `middlenet-adapters`.
3. Build the Direct Lane (`middlenet-render`) first.
4. Route feed/article-native `viewer:middlenet` through Direct.
5. Reuse the same semantic/render path for previews, search results, and
   observation cards.
6. Add `graphshell-gpu`-style shared host plumbing for device/font/image/
   offscreen/frame coordination.
7. Add `middlenet-html` as an optional HTML lane using Blitz's DOM/style/layout
   integration and Graphshell's WebRender fork for paint/compositing.
8. Keep Servo as the full-web lane and narrow delegation only when Direct/HTML
   are truly ready.

This ordering favors the most portable, deterministic, and broadly useful lane
first.

---

## 12. Design Summary

The Middlenet lane architecture turns Graphshell into a **modular browser** in a
concrete, disciplined sense:

- protocol-first core,
- multiple renderer/execution lanes,
- shared host plumbing,
- explicit lane selection,
- no single engine monopolizing the definition of content truth.

That is a better fit for Graphshell than either:

- making Blitz's renderer stack the center of Middlenet, or
- growing Middlenet into a monolithic mini browser engine.
