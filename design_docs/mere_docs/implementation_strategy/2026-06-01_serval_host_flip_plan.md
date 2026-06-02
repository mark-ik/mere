# Serval-as-Host Flip Plan

**Date**: 2026-06-01
**Status**: Gate open on the interactive items. **P0 (perf spike) run 2026-06-01:
the relayout worry is retired (transform motion is paint-tier → `RepaintOnly`, not
reflow), BUT it surfaced three serval prerequisites for the orrery's continuous
motion — incremental inline-`style` invalidation, repeated-`apply()` restyle, and
transform→paint-position — now Phase-1 gates (see Phase 0).** Execution plan for
flipping Mere's host from Xilem + Masonry (architecture 1) to serval-as-host
(architecture 3). Cross-repo: sequences mere-side work against serval capabilities.
**Decision owner**: the [serval-as-host evaluation](../technical_architecture/2026-05-29_serval_as_host_evaluation.md)
owns the *call* and the §6/§7 worked consequences; this doc is the *execution*.
**Related**: [host architecture roadmap](2026-05-20_host_architecture_roadmap.md),
[adoption roadmap](2026-05-27_adoption_roadmap.md),
[scrying integration plan](2026-05-27_scrying_integration_plan.md),
serval [`xilem_serval` plan](../../../../serval/docs/2026-05-27_serval_as_host_xilem_serval_plan.md).

---

## Gate status (verified 2026-06-01)

The flip gate (evaluation brief §8) is **open on the two interactive items,
pending the orrery perf spike**:

- **IME — done.** All three tiers plus the underline-styled preedit landed in
  `xilem-serval` (serval `c7a78a5` "IME functionally complete", `944c070` T2
  preedit, `42d9d04` T1 commit + T3 candidate placement). This was the long pole.
- **Form-control breadth — done.** The pointer-drag foundation (`on_pointer` +
  capture, `939dc7d`) and the `slider` on it (`e2886d8`, wired `52de93f`) landed.
  Slider was the last open control; pointer-drag also unlocks scrollbar-thumb
  drag, resize handles, and drag-tab-out.
- **Perf spike — pending (Phase 0).** §8's check that transform-only node motion
  lands on serval's `RepaintOnly` path, not `full_relayout`, at orrery scale is
  not yet measured. serval has the mechanism (fine-grained restyle / `RepaintOnly`,
  `2026-05-25_fine_grained_restyle_plan.md`) but no orrery-scale result. It can
  send the orrery flip back, so it leads.

## Findings (grounded 2026-06-01)

- serval and the mere Xilem fork share **vello 0.9 / wgpu 29**, so there is no
  renderer-stack reconciliation.
- `xilem-serval` is at Stage 7 (scrolling, z-index, overlays, a11y, select / radio
  / textarea, IME, slider), validated on screen.
- The **host-agnostic core is unaffected** (brief §5): kernel, forme, inker
  contracts, mere-domain, eidetic, gyre, and the whole field system (aether eval,
  gyre forces, the platen visual pass). The orrery **scene-paint underlay producer
  already exists** (`platen::orrery::orrery_paint_list`, `1110a26`); it is Phase-1
  layer 1, host-neutral by design.
- netrender has `compose_external_texture` / `ExternalTextureItem` (brief §3), so
  scrying re-homes; the GPU-interop core is host-agnostic.

## Plan (done-conditions, not dates)

### Phase 0 — The perf spike (gates the orrery flip)

Model node motion as `transform`, not `left`/`top`, and measure relayout incidence
on a moving N-node orrery (hundreds to thousands of nodes, 60fps physics) against
the `canvas_behavior_contract` scenarios, on serval. Serval-side; gates the orrery
phases below. Chrome phases (2–4) can proceed in parallel since they are flex/DOM,
not transform-animated.

**Done (2026-06-01, serval `6bf33947f`; spike + writeup in
[serval/docs/2026-06-01_orrery_transform_perf_spike.md](../../../../serval/docs/2026-06-01_orrery_transform_perf_spike.md)).**
The relayout worry is **retired**: a transform value change is paint-tier on
serval's pinned stylo (`RECALCULATE_OVERFLOW` < `RELAYOUT`), so
`IncrementalLayout::apply()` returns `RepaintOnly` — layout skipped, box geometry
untouched, at N up to 1000 (test-proven + source-verified). Transform motion does
NOT force reflow.

But the spike surfaced **three serval prerequisites** the orrery's *continuous*
transform-driven motion needs (the relayout classification is necessary, not
sufficient). They move into Phase 1 as explicit gates, all serval-side:

- **(A) incremental restyle ignores inline-`style` changes** — `snapshot.rs` marks
  them `other_attributes_changed` (only `[attr]`-selector invalidation); inline-style
  re-cascade needs a `RESTYLE_STYLE_ATTRIBUTE` hint. The full `run_cascade` does
  apply inline style, so this is an incremental-path gap. The orrery moves nodes by
  mutating inline `transform`, so this must be wired.
- **(B) a second sequential `RepaintOnly` `apply()` drops the change** — repeated
  per-frame applies don't re-register (the layout-skip leaves stylo restyle state
  uncleared), so continuous motion via repeated `apply()` doesn't work yet.
- **(C) paint doesn't fold the CSS transform into painted position** —
  `paint_emit` uses the taffy `Layout.location`, so a cascaded transform wouldn't
  visibly move the node.

Net: the gate's fear is gone, but the orrery's motion mechanism is gated on A+B+C.
A and B are pinned as tripwire tests in serval-layout that flip when fixed.

### Phase 1 — The orrery element (brief §6)

A serval custom-layout element composing three layers:

1. **Scene-paint underlay** — `platen::orrery::orrery_paint_list` (built)
   contributes its `PaintCmd` list as the element's paint sublist, under
   `PushTransform(camera)`.
2. **Physics-positioned DOM children** — gyre `cull_aabb` selects visible nodes;
   each materializes as a serval DOM subtree, `position: absolute` + a per-frame
   `transform: translate(x, y)` from the sim. Off-screen nodes demote to underlay
   glyphs (virtualization rides `cull_aabb`); focus/state survive demotion.
   **Gated on P0's prerequisites A+B+C** (serval-side): incremental inline-`style`
   invalidation, repeated-`apply()` restyle correctness, and folding the CSS
   transform into painted position. Until those land, per-frame inline-transform
   motion is either ignored or invisible; this phase starts once they do.
3. **Camera** — one `PushTransform(TransformSpec)` over both layers; the navigation
   defaults (wheel = pan, ctrl+wheel = zoom, inertia, infinite canvas) live in the
   element's input handling.

Two-hit-test split: node content via serval `FragmentQuery`; scene geometry (empty
space, edge pick, marquee) via gyre's `QueryPipeline`. **Done:** the orrery renders,
pans/zooms, drags a node, and shows force + visual couplings, hosted by serval,
fed by the producer + gyre. Node positions transition from committed to gyre-live
in this phase (the producer reprojects unchanged).

### Phase 2 — platen retarget (brief §7)

Morphorm → taffy/flex. platen becomes an `xilem_serval` consumer: it diffs the
forme tile-tree into serval DOM (flex containers, draggable dividers, tile
content-roots) and handles resize by updating flex-basis. Within-tile content is a
serval content-root (an inker/nematic engine's output as DOM) or an
`ExternalTextureItem` (WebView/scrying). Hybrid: flex for the docked split-tree,
absolute for floaters / sticky-notes. The canvas swatch is the Phase-1 orrery
element placed as a tile/region. The tiling *model* + interaction + serialization
stay platen's (they were never CSS). **Done:** the workbench tiling renders,
resizes, and rearranges through serval; the Morphorm dependency is dropped.

### Phase 3 — Chrome rebuild in `xilem_serval`

Rebuild the toolbar / omnibar / frametree / panes as `xilem_serval` views
(chrome-as-DOM), on a `pelt-live`-shaped serval host. Hold the **separate-roots
discipline** from the first commit: the chrome-root (diffed by `xilem_serval` from
app state) and each content-root (mutated by its engine/JS) are distinct document
authorities; neither sees the other's tree. `register-theme` becomes real CSS.
**Done:** the mere chrome runs as `xilem_serval`, on screen, with navigation +
panes working.

### Phase 4 — External content re-home

Web / scrying tiles move from Masonry's external layer to netrender
`compose_external_texture` / `ExternalTextureItem` (`texture_key` +
`content_generation` as the frame-arrival hint). The scrying GPU-interop core is
host-agnostic; only the compositor seam moves, and the destination exists (the
counter demo exercises it). **Done:** a web/scrying tile composites through
serval's external-texture path inside a content-root tile.

### Phase 5 — Cutover

A serval host (pelt-live-shaped) owns the window, input, layout, paint, script, and
accessibility; AccessKit emits from the one semantic DOM (no two-tree merge). The
`crates/mere/app` Masonry path retires. **Done:** mere runs on serval-as-host; the
Xilem + Masonry host is removed.

## Standing constraints (brief §9)

- **Stop deepening Masonry investment**; keep every new host-coupling retargetable.
- **Hold separate-roots** from day one (the invariant that goes wrong quietly).
- **Run Phase 0** before committing to render the whole chrome through serval.
- The **host-agnostic core needs no flip work** — kernel/forme/inker/mere-domain/
  gyre and the field system are consumed by the new host unchanged. The flip is a
  rebuild of the thin host-coupled layer, not an excavation.

## Progress

- **2026-06-01** — Plan created. Gate verified open on IME + form-control breadth
  against the serval git log; the §8 perf spike (Phase 0) is pending (mechanism
  present, orrery-scale measurement not run). The orrery scene-paint underlay
  producer already landed host-neutral (`platen::orrery`, `1110a26`) and is Phase-1
  layer 1. No flip code written yet; this is the coordination artifact, and
  execution is cross-cutting across mere + serval.

- **2026-06-01** — **P0 run (serval `6bf33947f`).** Orchestrated a read-only recon
  workflow (6 agents) mapping serval's incremental-layout, then implemented the
  spike in `serval-layout` (instrumentation + 4 tests; serval-layout 80 tests pass).
  Verdict: the relayout fear is **retired** — a transform value change is paint-tier
  (`RECALCULATE_OVERFLOW` < `RELAYOUT`) → `apply()` returns `RepaintOnly`, layout
  skipped, box geometry untouched, N up to 1000 (test + pinned-stylo source). The
  spike then surfaced three serval prerequisites for *continuous* transform motion,
  now Phase-1 gates: (A) incremental restyle ignores inline-`style` changes; (B) a
  second sequential `RepaintOnly` `apply()` drops the change; (C) paint doesn't fold
  the CSS transform into painted position. A + B are pinned as serval-layout
  tripwire tests. Writeup:
  [serval/docs/2026-06-01_orrery_transform_perf_spike.md](../../../../serval/docs/2026-06-01_orrery_transform_perf_spike.md).
  So P0's measurement is done; the orrery flip (P1) is gated on A+B+C, all serval-side.
