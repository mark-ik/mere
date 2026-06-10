# Host Cheap-Path Plan

**Date**: 2026-06-10
**Status**: Planned. No code written yet.
**Scope**: Move meerkat's DOM panes off the stateless per-frame pipeline and onto
the incremental machinery that already exists on both sides of the seam
(serval's `IncrementalLayout` + the always-recorded `DomMutation` stream), and
stop re-running cascade+layout for point queries. Cross-repo: serval supplies
the session/query seam; meerkat adopts it.
**Related**: the archived
[serval-as-host flip plan](../../archive_docs/2026-06-10_completed_plans/2026-06-01_serval_host_flip_plan.md)
(this plan picks up the perf story the flip deferred);
serval's [layout infrastructure scope](../../../../serval/docs/2026-06-07_serval_layout_infrastructure_scope.md)
(§1 interaction state is what makes `:hover`/`:focus` styling work under
sessions; §2's shared-font-collection note merges with C2 here);
serval's [orrery transform perf spike](../../../../serval/docs/2026-06-01_orrery_transform_perf_spike.md)
(the proof the session's `RepaintOnly` path holds at N=1000).

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
  (`serval-scripted-dom/lib.rs:394-396`), and across mere only the orrery
  drains (`orrery/src/frame.rs:101,145`). The chrome DOM gains at least one
  `AttributeChanged` per frame from the unconditional shellbar inline-style
  write (`render.rs:167-180`); the workbench DOM grows on state changes.
  Frame-local pane DOMs (roster/apparatus/utility) are dropped wholesale, so
  they are bounded but wasteful.
- The eager-apply design of xilem-serval exists *for* this batching boundary
  ("serval batches at the drain_mutations → relayout boundary",
  `xilem-serval/src/lib.rs:14-23`); the product host never exercises it.
- **The cost is plausible but unmeasured.** No chrome-update profile exists.
  C0 records a baseline so the win (or its absence) is a number, not a guess.

## What already exists (do not rebuild)

- `IncrementalLayout` (`serval-layout/incremental.rs`) retains StylePlane +
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
   Stylesheet hot-reload stays a serval follow-up; user-CSS editing is its
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

### C1 — Laid-out-document query seam (serval)

pelt-live (or serval-layout) exposes a query object over retained layout
artifacts: `hit_test`, `caret_screen_rect`/`caret_byte_at`, `rect_of`/
fragment enumeration, and the a11y tree, as methods over
(styles, fragments, built box tree, text ctx) computed once, instead of each
query re-running cascade+layout (`pelt-live/render.rs:290-348`).
`IncrementalLayout` already retains exactly these fields; the stateless path
can return the same object. **Done when** pelt-live's own bin serves render +
all queries from one cascade+layout per dirty frame.

### C2 — One font system per pass (serval)

Layout entry points accept a persistent font context (host-held or
session-owned) instead of `TextMeasureCtx::new()` per pass
(`box_tree.rs:679`, `text_measure.rs:314-323`). Fold in the infra-scope §2
note: cascade font metrics and text shaping resolve from the same collection.
meerkat's `text.rs` already holds its contexts for the host's life; this
brings layout to the same discipline. **Done when** a steady-state frame
performs no font discovery.

### C3 — Chrome pane onto a session (mere)

App holds an `IncrementalLayout` session for the chrome DOM. Frame loop:
drain → apply → `RepaintOnly`/`Restyled` fast paths → `emit_paint_list` →
Scene; `Spliced`/`FullRecompute` falls back to full recompute inside the
session; theme switch and resize recreate the session. Before the swap,
capture stateless-vs-session Scene snapshot fixtures (netrender postcard) for
a scripted set of chrome states and assert parity. **Done when** chrome
renders through the session, fixtures are green, and the C0 table shows the
delta.

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

### C6 — Host wiring parity (mere + serval; the audit's adjacent gaps, tracked here until picked up)

Each lands separately; spin out into its own plan if it grows.

- **IME wiring in meerkat**: the library + demo are complete; meerkat has no
  `winit::Ime` arm, no `set_ime_allowed`, no `set_ime_cursor_area`, so the
  omnibar cannot take CJK/composition input. Done when preedit renders in the
  omnibar via the C1 caret seam.
- **`on_wheel` event view** (serval, the one open Stage 3 item): registry +
  dispatch parallel to `on_pointer`, so meerkat's hand-routed wheel
  (`app_handler.rs:370-414`) and host-owned ScrollOffsets become view-owned.
- **Pointer cancellation**: give `PointerEvent` a Propagation cell and have
  `route_pointer` record `default_prevented` (today it leaves the stale
  click/key value). Relevant the moment drags route through `on_pointer`.
- **`memoize` the stable chrome subtrees** (re-exported and tested over
  ServalCtx, unused in meerkat) to cut the O(view tree) rebuild per event.
- **Transform-aware hit-testing** (serval): `walk_for_hit`
  (serval_lane.rs:310-374) composes box locations only; paint already computes
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
  serval-side seams; meerkat's adoption is confined to render.rs / input.rs /
  frame_ops.rs call sites.

## Progress

- **2026-06-10** — Plan created from the stack audit. No code yet. C0 is the
  entry point and is deliberately tiny (a drain call and a profile) so the
  baseline lands before any structural change.
