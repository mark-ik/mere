# Host Cheap-Path Plan

> **ARCHIVED 2026-06-15** (DOC_POLICY §8). The perf chain (C0–C5 + C4c) shipped and
> is proven (chrome cascade+layout 4.3×, whole frame −40%). The spun-out C6 grab-bag
> lives in `host_wiring_grabbag_plan` (still active). Original location:
> `mere_docs/implementation_strategy/`; internal relative links resolve against that path.

**Date**: 2026-06-10
**Status**: C0–C5 + C4c DONE (the perf chain). C6 spun out →
[host_wiring_grabbag_plan](../2026-08-06_completed_plans/2026-06-11_host_wiring_grabbag_plan.md). Ready to archive.
**Scope**: Move meerkat's DOM panes off the stateless per-frame pipeline and onto
the incremental machinery that already exists on both sides of the seam
(genet's `IncrementalLayout` + the always-recorded `DomMutation` stream), and
stop re-running cascade+layout for point queries. Cross-repo: genet supplies
the session/query seam; meerkat adopts it.
**Related**: the archived
[genet-as-host flip plan](../../archive_docs/2026-06-10_completed_plans/2026-06-01_genet_host_flip_plan.md)
(this plan picks up the perf story the flip deferred);
genet's [layout infrastructure scope](../../../../genet/docs/2026-06-07_genet_layout_infrastructure_scope.md)
(§1 interaction state is what makes `:hover`/`:focus` styling work under
sessions; §2's shared-font-collection note merges with C2 here);
genet's [orrery transform perf spike](../../../../genet/docs/2026-06-01_orrery_transform_perf_spike.md)
(the proof the session's `RepaintOnly` path holds at N=1000);
the [window composition plan](2026-06-11_window_composition_plan.md)
(its P2+ pane-heavy phases build on C6's composition-enabling subset and benefit from
the per-pane sessions, so this plan is the runway for that one).

---

## The problem (2026-06-10 stack audit, code-verified)

The orrery rides the cheap path; everything else pays the expensive one.

- **Per redrawn frame**, meerkat runs the stateless pipeline
  (`scene_from_scripted_dom` = fresh Stylist incl. UA+author sheet re-parse,
  full cascade, box tree + taffy layout, paint emit, plus a fresh parley
  `FontContext` whose construction does system-font discovery) once per DOM
  pane: chrome (`render.rs:202`), workbench (`:241`), apparatus (`:806`),
  utility panes (`:852`). The roster is worse: it rebuilds its DOM from scratch
  each frame, then runs the pipeline twice on it
  (`fragments_from_scripted_dom` at `:736` for scroll clamping,
  `scene_from_scripted_dom` at `:752` for paint).
- **Per point query**, the full pipeline runs again: `hit_test_node` per click
  and divider probe (`input.rs:408-481`), `fragments_from_scripted_dom` for
  popup anchors / scroll boxes / a11y rects (`render.rs:293`, `main.rs:1026`).
  One interactive frame can run cascade+layout 4-8 times on identical DOMs.
- **The mutation logs of the two session-persistent DOMs leak.** Every
  `set_attribute`/insert/remove pushes a `DomMutation` (with cloned old-value
  String); `drain_mutations` is the only sink
  (`genet-scripted-dom/lib.rs:394-396`), and across mere only the orrery
  drains (`orrery/src/frame.rs:101,145`). The chrome DOM gains at least one
  `AttributeChanged` per frame from the unconditional shellbar inline-style
  write (`render.rs:167-180`); the workbench DOM grows on state changes.
  Frame-local pane DOMs (roster/apparatus/utility) are dropped wholesale, so
  they are bounded but wasteful.
- The eager-apply design of xilem-serval exists *for* this batching boundary
  ("genet batches at the drain_mutations → relayout boundary",
  `xilem-serval/src/lib.rs:14-23`); the product host never exercises it.
- **The cost is plausible but unmeasured.** No chrome-update profile exists.
  C0 records a baseline so the win (or its absence) is a number, not a guess.

## What already exists (do not rebuild)

- `IncrementalLayout` (`genet-layout/incremental.rs`) retains StylePlane +
  persistent Stylist + FragmentPlane + BoxTree + TextMeasureCtx;
  attribute-only batches take `RepaintOnly` (layout skipped, retained box tree
  feeds `emit_paint_list`); proven in the orrery loop over 400 sustained
  frames crossing stylo's rule-tree GC interval.
- The orrery frame loop (`orrery/src/frame.rs:99-152`) is the working template:
  mutate, drain, apply, emit, compose via `composite_paint_layers`.
- netrender's byte-deterministic Scene snapshots (`scene.rs` postcard/json,
  behind `serde`) give parity fixtures for the migration.

## Known constraints (budget for these; they are why "just use sessions" is not a one-liner)

1. **Frozen stylesheets**: a session's sheet set is fixed at `new()`;
   the check is debug_assert-only, so a release-mode sheet change silently
   restyles against old sheets (`incremental.rs:74-80,151-159`). Theme
   switching (`frame_ops.rs:1291-1295`) must recreate the session. Acceptable:
   themes change rarely; the orrery already recreates on viewport resize.
   Stylesheet hot-reload stays a genet follow-up; user-CSS editing is its
   first real consumer.
2. **Structural splices invalidate paint**: a `Spliced` apply sets
   `paint_side_valid=false`, after which `emit_paint_list` is unusable
   (`incremental.rs:84-99`). Chrome diffs do insert/remove nodes (palette
   rows, suggestion lists), unlike the orrery's attribute-only pool. The
   session path must fall back to full recompute on splice frames. Measure
   splice incidence in C0; if most frames splice, the win shrinks and we say
   so in this doc.
3. **Dynamic pseudo-classes** (`:hover`/`:focus`) restyle only on full
   cascade today; the incremental state-change path is the infra-scope §1
   work. Chrome hover styling via class mutation (what xilem-serval does now)
   rides the attribute path and is unaffected.
4. **Incremental restyle passes `base_url: None`** (`cascade.rs:348-350`), so
   relative `url()` refs are not re-resolved incrementally. Chrome sheets are
   token-generated colors; verify none carry relative urls before C3.

## Phases (done-conditions, not dates)

### C0 — Stop the leak, take the baseline (mere)

Drain-and-discard `DomMutations` once per frame on the chrome + workbench
DOMs. Add a frame profile (tracing spans or netrender's A4 FrameTimings
pattern) around the per-pane pipeline calls and record a representative
interaction (omnibar typing, palette open, divider drag) in this doc, with
splice incidence per pane. **Done when** mutation Vecs are bounded and a
baseline table exists below.

**C0 DONE (2026-06-11).** Drain landed (`render.rs`, chrome + workbench, once per
frame). Headline frame profile captured over ~280 rendered frames of a
representative interaction (omnibar typing, palette open + arrow, roster +
apparatus panes, divider drag, orrery pan), **unoptimized debug build**:

| metric | median | p95 | min | max |
| --- | --- | --- | --- | --- |
| `render()` total | 56 ms | 132 ms | 51 ms | 248 ms (cold first frame) |
| chrome cascade+layout+paint | 29 ms | 40 ms | 27 ms | 55 ms |

**The stateless chrome pipeline is 53% of every rendered frame (median; 56% p95).**
It is the single dominant, *unconditional* per-frame cost — the plan's thesis, now a
number. Absolutes scale down in release, but the **ratio is build-independent** (both
stateless paths scale together), so **sessionizing the chrome pane (C3) roughly halves
the frame.** The p95→max tail (132–248 ms) is the splice/pane-heavy frames; per-pane
granularity (the conditional roster — which runs the pipeline *twice* — apparatus, and
utility panes) is the documented refinement, but the headline already justifies C3 and
C1/C2 under it. Instrumentation lives behind `RUST_LOG=meerkat::profile=debug`
(`render.rs`), so re-measuring the delta after C3 is one run.

### C1 — Laid-out-document query seam (genet)

pelt-live (or genet-layout) exposes a query object over retained layout
artifacts: `hit_test`, `caret_screen_rect`/`caret_byte_at`, `rect_of`/
fragment enumeration, and the a11y tree, as methods over
(styles, fragments, built box tree, text ctx) computed once, instead of each
query re-running cascade+layout (`pelt-live/render.rs:290-348`).
`IncrementalLayout` already retains exactly these fields; the stateless path
can return the same object. **Done when** pelt-live's own bin serves render +
all queries from one cascade+layout per dirty frame.

**C1 seam DONE (2026-06-11, genet `7d87a3b668b`).** `LaidOutDocument`
(`pelt-live/render.rs`) computes cascade+layout once and serves `hit_test`,
`fragments`/`rect_of`, `caret_screen_rect`, `soft_wrap_caret_byte`,
`caret_byte_at`, and `accesskit_tree` as methods over the retained (styles,
fragments, box tree, text ctx). The free `*_from_scripted_dom` / `hit_test_node`
/ caret functions now delegate to it (compute-once-then-query). **Remaining for
the full done-condition:** the bin (`main.rs`/`input.rs`) builds *one*
`LaidOutDocument` per dirty frame and routes its queries through it (the free
functions still compute one layout each, so a multi-query frame still multiplies
until the bin adopts the object).

### C2 — One font system per pass (genet)

Layout entry points accept a persistent font context (host-held or
session-owned) instead of `TextMeasureCtx::new()` per pass
(`box_tree.rs:679`, `text_measure.rs:314-323`). Fold in the infra-scope §2
note: cascade font metrics and text shaping resolve from the same collection.
meerkat's `text.rs` already holds its contexts for the host's life; this
brings layout to the same discipline. **Done when** a steady-state frame
performs no font discovery.

**C2 DONE (2026-06-11, genet `2dc05c84087`).** `TextMeasureCtx::reset()` clears
the per-pass parley-layout caches (stale Taffy keys) while keeping the persistent
`font_ctx`/`layout_ctx`; `layout_via_box_tree` takes a caller-held
`&mut TextMeasureCtx`. `IncrementalLayout` now builds its context once and reuses
it across full relayouts (it previously *replaced* it each time), so a session
runs **no font discovery in steady state** — the done-condition for the path C3
sits on. The stateless `layout()` keeps a fresh-ctx wrapper so existing pelt-live
render/query callers are unchanged; they migrate to a held context in C3+. The
infra-scope §2 shared-collection note (cascade metrics + shaping from one
`fontique::Collection`) is a consistency follow-up, not a discovery cost (the
cascade's metrics collection is already thread-local-amortized).

### C3 — Chrome pane onto a session (mere)

App holds an `IncrementalLayout` session for the chrome DOM. Frame loop:
drain → apply → `RepaintOnly`/`Restyled` fast paths → `emit_paint_list` →
Scene; `Spliced`/`FullRecompute` falls back to full recompute inside the
session; theme switch and resize recreate the session. Before the swap,
capture stateless-vs-session Scene snapshot fixtures (netrender postcard) for
a scripted set of chrome states and assert parity. **Done when** chrome
renders through the session, fixtures are green, and the C0 table shows the
delta.

**C3 DONE (2026-06-11).** Chrome renders through a per-window `IncrementalLayout`
session (`ChromeSession`, its own meerkat module at
`crates/meerkat/src/chrome_session.rs`). Realized slightly differently from the
sketch above: rather than recover from a `Spliced` apply (which invalidates the
paint side), the session is **rebuilt** whenever the drained batch is structural
(or the viewport / sheet changed) — a full layout, the same cost as the old
stateless frame, but only on those frames — so `apply` only ever sees
attribute-only batches and the session is never left un-emittable. Resize and
theme self-heal through a dims/sheet compare; no caller invalidates it. Parity is
a runtime fixture (`session_render_matches_stateless_render`, genet pelt-live):
`scene_from_session` is **op-for-op identical** to `scene_from_scripted_dom`
across plain / caret / selection states. The session overlay sources caret /
selection / `::selection` color from new `IncrementalLayout` query methods (the C1
seam applied to the session). Re-measured over 560 frames of a representative
interaction, **same debug build as C0**:

| metric | C0 (stateless) | C3 (session) | change |
| --- | --- | --- | --- |
| chrome cascade+layout+paint, median | 29 ms | **6.8 ms** | −77% (4.3×) |
| chrome, p95 | 40 ms | 28 ms | −29% |
| `render()` total, median | 56 ms | **33 ms** | −40% |
| chrome share of the median frame | 53% | 20.5% | |

Steady-frame chrome dropped 4.3× (the `RepaintOnly` path skips cascade+layout),
taking the whole frame down ~40% (56 → 33 ms) — essentially the full achievable
chrome saving (floor ≈ 33.8 ms = 56 − (29 − 6.8)). The frame is now dominated by
the non-chrome panes + orrery; chrome is a fifth of it. The p95 tail (132 → 90 ms)
is the rebuild frames (typing / palette structural batches) plus the pane-heavy
frames C5 addresses. The ~6.5 ms steady residual is apply + emit + translate +
overlays; an equality-guard on the per-frame shellbar inline-style write (→
`Unchanged`, skipping even the restyle) is the obvious next trim. Instrumentation
stays behind `RUST_LOG=meerkat::profile=debug`.

### C4 — Queries ride the seam (mere)

Port `input.rs` hit-tests, popup anchor, scroll clamp, caret, and a11y rect
reads to C1's query object. Where a pane is DOM-backed, swap the hand-built
UxTree for pelt-live's DOM-derived `accesskit_tree` (tested, currently
zero callers in mere) reparented under the window root; graph-semantic nodes
stay host-built. **Done when** an interactive frame runs cascade+layout at
most once per dirty pane, and the chrome a11y subtree derives from the same
DOM that renders.

### C5 — Remaining panes (mere)

Workbench (equality-guarded writes already skip most frames), roster
(persistent DOM instead of rebuild-per-frame), apparatus, utility panes.
**Done when** no per-frame `scene_from_scripted_dom` callers remain in
meerkat's render path.

### C6 — Host wiring parity (mere + genet; the audit's adjacent gaps)

**SPUN OUT (2026-06-11) → [host_wiring_grabbag_plan](../2026-08-06_completed_plans/2026-06-11_host_wiring_grabbag_plan.md).**
With C0–C5 + C4c done, C6 is all that remained here, so it grew into its own plan
(eight items, two phases: G1 composition-runway, G2 host-completeness). The text
below is the snapshot it was scoped from; the grab-bag plan is now the checklist
of record. This plan (the perf chain) is **ready to archive**.

This is the **grab-bag of unused genet/xilem-serval host capability** — wired and
tested one layer down, with zero or stub meerkat callers. Each lands separately; spin
out into its own plan if it grows.

**Sequencing split (2026-06-11).** Four of these are **composition-enabling**:
[window composition](2026-06-11_window_composition_plan.md) P2+ (per-pane input/hit-test,
interactive DOM under the orrery, the growing pane tree, cross-graph drags) builds
directly on them, so they are the runway-clearing subset to do before the pane-heavy
phases:

- **`on_wheel` event view** → per-pane wheel routing (retires meerkat's hand-routed wheel).
- **transform-aware hit-testing** → interactive DOM under the orrery camera.
- **pointer cancellation** (the `Propagation` cell) → drags routed through `on_pointer`.
- **`memoize` the stable chrome subtrees** → the rebuild cost of a growing pane tree.

The other three are **host-completeness** (correctness, independent of composition; do
anytime): **IME**, **environment threading**, **keyboard-model escape hatches**. The
**a11y `accesskit_tree` swap** (in C4) is also host-completeness. The full grab-bag is
the eleven items across C1–C6; this plan (C1–C6) is the checklist of record.

*Adjacent but a different crate (not genet/xilem, so tracked in the scrying plan, not
here):* scrying X2's leftover host wiring — omnibar `load_url`, back/forward + `can_go_*`,
`poll_navigation_event` → omnibar/lineage, `poll_cursor_shape` → winit cursor, Tab focus.

- **IME wiring in meerkat**: the library + demo are complete; meerkat has no
  `winit::Ime` arm, no `set_ime_allowed`, no `set_ime_cursor_area`, so the
  omnibar cannot take CJK/composition input. Done when preedit renders in the
  omnibar via the C1 caret seam.
- **`on_wheel` event view** (genet, the one open Stage 3 item): registry +
  dispatch parallel to `on_pointer`, so meerkat's hand-routed wheel
  (`app_handler.rs:370-414`) and host-owned ScrollOffsets become view-owned.
- **Pointer cancellation**: give `PointerEvent` a Propagation cell and have
  `route_pointer` record `default_prevented` (today it leaves the stale
  click/key value). Relevant the moment drags route through `on_pointer`.
- **`memoize` the stable chrome subtrees** (re-exported and tested over
  GenetCtx, unused in meerkat) to cut the O(view tree) rebuild per event.
- **Transform-aware hit-testing** (genet): `walk_for_hit`
  (genet_lane.rs:310-374) composes box locations only; paint already computes
  the needed matrices in the same crate (`compute_transform_matrix` +
  `conjugate_at`). Gates any interactive DOM content under the orrery camera,
  and (with the pointer cell above) the *interactive* external-texture element.
- **Environment threading** (xilem-serval): dispatch builds
  `MessageCtx::new(Environment::new(), ...)` while builds use the real one, a
  split-brain that surfaces the moment an environment-dependent view (theming,
  scaling) lands in chrome. Mechanical: pass `ctx.environment` at the three
  construction sites.
- **Keyboard model escape hatches** (xilem-serval, per the configurability
  rule): an explicit `focusable()` marker + synthetic Enter/Space activation
  (today focusable == has-on_key, so a plain button is keyboard-unreachable),
  an overridable Tab default (today swallowed pre-routing: no tab character in
  textareas, no custom order), and Vec-per-node listener registries (today a
  second `on_click` on the same node silently clobbers the first).

## Findings

- 2026-06-10 audit receipts are inlined above. Headline: the product host
  runs the expensive path while the cheap path sits proven one layer down;
  three independent review lenses converged on the same diagnosis.
- The flip plan's standing constraint "keep every new host-coupling
  retargetable" survives this plan untouched: sessions and the query seam are
  genet-side seams; meerkat's adoption is confined to render.rs / input.rs /
  frame_ops.rs call sites.

## Progress

- **2026-06-10** — Plan created from the stack audit. No code yet. C0 is the
  entry point and is deliberately tiny (a drain call and a profile) so the
  baseline lands before any structural change.
- **2026-06-11** — **C0's drain landed** during the multi-window work: chrome +
  workbench `DomMutation`s are drained once per frame (`render.rs:1092-1093`), so
  those Vecs are bounded. C0's **baseline table is still pending** (needs the frame
  profile + a recorded interaction). **Grab-bag folded:** C6 now calls out the
  composition-enabling subset (`on_wheel`, transform-aware hit-test, pointer
  cancellation, `memoize`) that [window composition](2026-06-11_window_composition_plan.md)
  P2+ builds on, vs the host-completeness items (IME, environment threading, keyboard
  escape hatches, the C4 a11y `accesskit_tree` swap). Eleven items total across C1–C6;
  this plan is the checklist of record. Decided (with Mark) to pick this plan up next:
  the composition-enabling subset clears the runway before the pane-heavy composition
  phases, and the C0 baseline quantifies the per-pane cost before it is multiplied.
- **2026-06-11** — **C0 done: baseline captured, thesis confirmed.** Added headline
  frame-profile instrumentation (`render.rs`, behind `RUST_LOG=meerkat::profile=debug`)
  and ran a representative interaction (~280 frames, debug build). Result: the stateless
  **chrome cascade+layout+paint pipeline is 53% of every rendered frame** (median 29 ms
  of a 56 ms frame; table in C0). The ratio is build-independent, so **C3 (sessionize the
  chrome pane) roughly halves the frame** — the win is a number, not a guess. Next:
  C1 (the laid-out-document query seam in genet) + C2 (persistent FontContext) are the
  genet-side infra C3 sits on; or pick up the composition-enabling C6 subset in
  parallel (independent of the perf chain). Re-measuring after C3 is one run.
- **2026-06-11** — **C1 + C2 landed (genet-side).** C2 (`2dc05c84087`):
  `TextMeasureCtx::reset()` + `layout_via_box_tree` takes a persistent
  `&mut TextMeasureCtx`; `IncrementalLayout` reuses one context across relayouts
  instead of recreating it, so the session path runs no steady-state font
  discovery. C1 (`7d87a3b668b`): `LaidOutDocument` serves every point query
  (hit-test, fragments, caret ×3, a11y) off one cascade+layout; the free query
  functions delegate to it. Both are tested in genet-layout / pelt-live and
  build clean. **Now C3 (sessionize the chrome pane in meerkat) has both pieces
  it sits on**; the remaining C1 step is the bin building one `LaidOutDocument`
  per dirty frame (rolls into C3/C4's render.rs+input.rs adoption).
- **2026-06-11** — **C3 done: chrome on a session, frame down ~40%.** meerkat's
  chrome renders through a per-window `IncrementalLayout` (`ChromeSession`), rebuilt
  only on a structural / resize / theme frame and riding the `RepaintOnly` path
  otherwise; genet gained session-side `caret_rect` / `selection_rects` /
  `selection_style` query methods + `scene_from_session` (the C1 seam applied to the
  session). Parity is op-for-op (`session_render_matches_stateless_render`). Re-measured
  over 560 frames (same debug build as C0): chrome cascade+layout+paint **29 → 6.8 ms**
  median (4.3×), the whole frame **56 → 33 ms** (−40%), chrome now 20.5% of the median
  frame (was 53%). Table in C3. The win matches the C0 prediction (the achievable chrome
  saving was captured in full). Next: **C4** (queries ride the C1 seam — port input.rs
  hit-tests / popup anchor / a11y rects off the stateless `fragments_from_scripted_dom`,
  and have the bin build one `LaidOutDocument` per dirty frame) and **C5** (workbench /
  roster / apparatus / utility panes), or the composition-enabling C6 subset (independent
  of the perf chain). A cheap follow-up trim: equality-guard the per-frame shellbar
  inline-style write so a static chrome frame returns `Unchanged` (skips even the restyle).
- **2026-06-11** — **C4 + C5 done; render glue extracted.** C4: the chrome
  hit-test and per-press region-gate read the session's retained layout (generic
  `IncrementalLayout::hit_test`; `find_by_source_id` exposed), so the chrome lays
  out once per frame. C5: generalized `ChromeSession` → **`PaneSession`**
  (`pane_session.rs`); the workbench rides it (render + slot reads + click hit-tests
  on one layout, was two-plus); roster + apparatus collapsed to one
  `IncrementalLayout` per frame (its fragments feed the scroll-clamp / hit-rects, the
  scene emits from the same layout); utility was already single-layout. **No DOM pane
  lays out more than once per frame now.** Alongside, the **pelt-live glue extraction**
  ([own plan](2026-06-11_genet_render_glue_extraction_plan.md)): meerkat owns its
  `ScriptedDom → Scene` glue (`genet_render`), so C5's emit-once path is a
  meerkat-local `scene_from_session` over a fresh session (no genet coordination), and
  `pelt-live` is dropped (~30 transitive crates pruned). All green (110 meerkat tests).
  **Deferred:** C4's a11y half (the chrome a11y subtree from the rendered DOM, a
  UxTree → accesskit swap, correctness-sensitive) and the C6 grab-bag.
- **2026-06-11** — **C4c done (a11y half): chrome a11y derives from the rendered DOM.**
  A `genet_a11y` module walks the chrome `ScriptedDom` and the chrome session's
  retained fragments into a `UxTree` (roles by tag, names folded from text, bounds
  from the session's layout, ids salted out of the path-hash space), replacing the
  single hand-built "Chrome" placeholder node; the bridge stitches it under the host
  window as before, and focus lands on the actual focused field. It reads the session's
  fragments, so it adds **no** layout (the a11y rides the render's one cascade+layout),
  which satisfies C4's full done-condition (≤ one layout per dirty pane **and** the
  chrome a11y from the same DOM that renders). Adapted from the dropped pelt-live a11y
  builder, retargeted `accesskit::TreeUpdate` → `UxTree`. Green (111 meerkat tests).
  **Still deferred:** chrome a11y *actions* (a screen reader activating the omnibar /
  a toolbar button) — the chrome was unactionable before too, so no regression; and the
  C6 grab-bag. So **C0–C5 + C4c are done**; C6 is what remains of the plan.
- **2026-06-11** — **C6 spun out; this plan is closed.** With the perf chain
  (C0–C5 + C4c) done, the only remaining work was the C6 host-wiring grab-bag, so
  it grew into its own plan:
  [host_wiring_grabbag_plan](../2026-08-06_completed_plans/2026-06-11_host_wiring_grabbag_plan.md) (eight items,
  G1 composition-runway + G2 host-completeness). This plan is ready to move to
  `archive_docs/` — the cheap-path thesis (sessionize → halve the frame) is proven
  and shipped (chrome 4.3×, whole frame −40%).
- **2026-06-11** — **Re-measured on a live orrery (post freeze-fix), win holds.**
  The earlier C3 numbers were taken before the orrery animated continuously, so
  re-ran the profile over 582 frames of a representative interaction (same debug
  build). chrome cascade+layout+paint **7.2 ms median / 8.9 ms p95** (was 6.8 ms
  at C3 — the +0.4 ms is the genet-layout F4 box-tree style refresh now on every
  `RepaintOnly` apply, negligible), whole frame **34.8 ms median** (≈ the C3 33 ms),
  chrome **~21%** of the frame. `total_us` includes `frame.present()` and quantizes
  near 16.7 ms multiples (vsync-paced), so the clean CPU win lives in `chrome_us`.
  Confirms C3–C5 intact with the orrery live; the frozen-orrery skew worry is
  cleared.
