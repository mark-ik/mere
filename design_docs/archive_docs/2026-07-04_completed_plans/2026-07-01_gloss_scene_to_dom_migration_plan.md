# Gloss Scene-to-DOM Migration Plan

**Status: P1-P3 landed, headed-verified, 247/247 tests green (2026-07-01/02).** See Progress
below. Converts the gloss pane's remaining two
Scene-textured sections — the whole-graph **minimap** swatch and the
**recently-visited** list — into real DOM, folded into the same unified shell
document as the roster and the gloss outline. This finishes the migration the
[gloss outline lens plan](2026-06-23_gloss_outline_lens_plan.md) explicitly
deferred (its Open Decision #5) until the outline itself proved the path.

This plan **implements** an already-settled direction; the DOM-not-Scene
architecture call was made in the
[gloss Navigator design](../design/2026-06-07_gloss_navigator_design.md) §2a
and re-affirmed by the outline lens plan realizing it first. Design-reviewed
before being written down (see Design review, below) — not a fresh spike.

---

## Where it fits

The gloss pane has three stacked sections: minimap (top), outline (middle,
DOM since P1), recent (bottom). `crate::gloss::gloss_sections(rect)` already
splits the pane's rect into the three bands every frame, consumed by both the
outline's DOM fold-in (`render/mod.rs`) and the minimap/recent Scene
rasterize (`render/paint.rs`) — this plan removes the second consumer's need
to exist as a separate code path at all.

Precedent already lives in this codebase for the exact split this plan
proposes: the **orrery pane itself**. `window_view/views.rs`'s
`orrery_element()` builds a DOM container whose first child is an
`<external-texture>` element carrying the graph's edges/backdrop raster
(`ORRERY_SCENE_KEY`), positioned by DOM layout, with real DOM squares
(`gnode_view`) for every node layered on top, click-routed and
NODE_SHEET-colored. The minimap becomes the same pattern at pane scale.
(Terminology: the DOM node square is a **gnode**, not a card — a card is
the summonable preview/object/connections family; see
[node_card_summoning_design](../design/2026-07-01_node_card_summoning_design.md).)

---

## Design review (2026-07-01, before implementation)

A Plan-agent independently checked this approach against alternatives and the
actual genet primitive set before it was committed to:

- **Is DOM-nodes-plus-embedded-Scene-edges right for the minimap, or should it
  stay pure Scene?** The minimap is genuinely unbounded — `Orrery::minimap_geometry()`
  draws every node/edge in the graph with no downsampling (recent, by
  contrast, is capped at 8 via `graph.recent_visited(8)`). For a real session
  that's plausibly dozens to low hundreds of nodes — a real cost question for
  per-node DOM. But it's the same math the orrery's own node cards already
  run today, at *larger* element size (favicon `<img>`, label text run, hull
  clip-path). If that performance profile is acceptable there, the
  smaller/simpler minimap squares (no favicon, no hull, no label) won't be
  the bottleneck. Verdict: proceed; add a node-count fallback-to-Scene
  threshold only if it actually shows up as a problem in practice, not
  pre-emptively.
- **Is there a genet vector primitive (SVG-like) that beats "Scene raster for
  edges"?** No. Checked directly against the pinned genet commit:
  `xilem-serval`'s tag set special-cases exactly one non-standard element,
  `<external-texture>` (`components/xilem-serval/src/tags.rs`). No `<svg>`,
  `<path>`, `<line>`, `<circle>`. The display primitive set is
  box/text/image/external-texture, full stop. Embedded Scene isn't just the
  best option for edges/rings, it's the *only* one that isn't "hundreds of
  hand-rotated 1px divs" (materially worse than the Scene it'd replace).
  Closed — no further spike needed.

**A required addition the review surfaced, not optional cleanup:**
`frame_a11y_panes.rs`'s existing `gloss_a11y_tree()` (~lines 294-320) reads
`self.view.gloss_node_rects` directly to set a11y bounds for minimap nodes
today (and, symmetrically, `gloss_recent_rects` for recent rows). Deleting
those fields as part of cleanup would silently break minimap/recent
accessibility unless this a11y construction is *also* migrated in the same
pass — onto DOM-layout-derived bounds, the same `all_with_class` +
layout-fragment lookup the outline's own a11y tree uses (see the
[gloss outline lens plan](2026-06-23_gloss_outline_lens_plan.md)'s P1a).

---

## Current state (code-verified 2026-07-01)

- **Minimap** (`crate::gloss::minimap_scene` in `gloss.rs`): takes
  `nodes: &[(GraphMemberId, (f32,f32) pos, bool selected, f32 size_factor)]`,
  `edges: &[((f32,f32) start, (f32,f32) end, f32 weight)]`,
  `rings: &[((f32,f32) center, f32 radius_factor, [f32;4] rgba)]` (community/
  bridge halos). Positions come from `Orrery::minimap_geometry()` (mirrors
  the main view) or `gloss_geometry_cached()` (an independent lens
  projection, when a gloss strategy is set). Nodes render as 7px squares
  (10px + brightened when selected); returns
  `Vec<(GraphMemberId, [f32;4])>` pane-local hit-rects.
- **Recent** (`crate::gloss::recent_scene`): takes
  `recent: &[(GraphMemberId, String url)]` (up to 8, from
  `graph.recent_visited(8)`); draws a "Recent" header + 16px rows, label
  fit-truncated; returns the same shape of hit-rects.
- **Hit-testing today** (fully separate from the outline's DOM hit-testing):
  `render/paint.rs` converts the returned pane-local rects to window-space,
  storing them in `WindowView.gloss_node_rects` / `gloss_recent_rects`;
  `pane_geom.rs`'s `gloss_node_at`/`gloss_recent_at` do an AABB scan; a
  bespoke branch in `input/mouse_dispatch/press.rs` (after the DOM-routed
  panes' check fails) resolves a hit to a `GraphMemberId`, looks up its URL,
  and calls `Orrery::select_by_url` — the same terminal call the DOM path
  uses, just reached through a parallel mechanism.

---

## Design

### Recent → pure DOM
One clickable row per `(GraphMemberId, url)`, structurally like
`roster_view_parts.rs`'s `node_table()`: `data-member` attr, wrapped in
`clickable(...)` pushing a select intent. Folded into `ShellState` the same
way the outline was — new `gloss_recent: GlossRecentState` +
`gloss_recent_rect: Option<[f32;4]>` fields, positioned via the `recent` rect
`gloss_sections()` already produces.

### Minimap → hybrid
- Node squares become real DOM: absolutely positioned
  (`transform:translate(x,y)`), sized/colored from NODE_SHEET state +
  selection (mirroring `gnode_view`, but simpler — no favicon, no hull,
  no label run), `data-member` attr, `clickable(...)`.
- Edges + rings stay a Scene raster — the existing `minimap_scene` logic
  minus the node-drawing half — embedded via a new `<external-texture>`
  element inside the minimap's DOM container. Mint a fresh reserved key
  disjoint from `ORRERY_SCENE_KEY` (e.g. `0xF0F0_0000_0000_0002`, same
  high-bit reserved range, different value — the compositor keys the *whole
  document*, not per-pane, so reusing the orrery's key would collide).
- Folded into `ShellState` the same way: `gloss_minimap: GlossMinimapState` +
  `gloss_minimap_rect`, positioned via the `minimap` rect from
  `gloss_sections()`.

### New file
`gloss_view.rs` (parallel to `gloss_outline_view.rs`), holding both
`recent_view()` and `minimap_view()` DOM builders + their state/intent
types — one file, two clearly separated functions (split further only if it
nears the workspace's 600-LOC ceiling). `gloss.rs` keeps `gloss_sections()`
and shrinks to just the trimmed edges/rings Scene function.

Worth folding in during this work: the outline's `GlossOutlineIntent::Select(String)`
is structurally identical to what minimap/recent need — hoist one shared
intent type instead of three near-identical single-variant enums.

### Cleanup (only once minimap + recent both land, and a11y bounds are migrated)
- Migrate `gloss_a11y_tree()`'s minimap/recent bounds lookup off
  `gloss_node_rects`/`gloss_recent_rects` onto DOM-layout-derived bounds
  (required — see Design review above).
- Delete `gloss_node_rects`/`gloss_recent_rects` (`WindowView` fields),
  `gloss_node_at`/`gloss_recent_at` (`pane_geom.rs`), and the bespoke gloss
  branch in `press.rs` — confirmed by the design review that no drag/hover/
  wheel/double-click code depends on them beyond what's covered above.
- Add `PaneContent::Gloss` to `chrome_routed_leaf_at`'s (`input/chrome.rs`)
  whole-leaf match list, alongside Roster/Apparatus/etc. — the outline's
  partial-pane `gloss_outline_at` check (from P1a) becomes unnecessary once
  the *entire* gloss pane is DOM-folded, and can be removed too.

---

## Phases (cheapest-first; done-conditions, not dates)

- **P1 — Recent → DOM.** Lowest risk; re-proves the folded-pane pattern on a
  simple shape (no vector graphics involved) before the minimap. Done:
  recent rows are real DOM, clickable, dispatch through `chrome_click`,
  headed-verified.
- **P2 — Minimap → hybrid.** DOM node squares + embedded-Scene edges/rings,
  new reserved `external_texture` key. Done: minimap nodes are real DOM,
  clickable, colored/selected via NODE_SHEET state; edges/rings still render
  correctly as the embedded raster; headed-verified.
- **P3 — A11y bounds migration + cleanup.** Migrate `gloss_a11y_tree()`'s
  bounds source; delete `gloss_node_rects`/`gloss_recent_rects`,
  `gloss_node_at`/`gloss_recent_at`, the bespoke `press.rs` branch, and (from
  P1a) `gloss_outline_at`; add `PaneContent::Gloss` to
  `chrome_routed_leaf_at`. Done: the entire gloss pane routes through the one
  shell hit-test with no bespoke rect cache left, minimap/recent a11y still
  work (agent_harness `SelectNodeByUrl` test against a minimap node and a
  recent row both pass), full `meerkat` suite green.

---

## Cross-references (consume, do not duplicate)

- [gloss outline lens plan](2026-06-23_gloss_outline_lens_plan.md) — the P1
  DOM section that proved this path, and P1a's a11y-wiring pattern this plan
  reuses for minimap/recent bounds.
- [gloss Navigator design](../design/2026-06-07_gloss_navigator_design.md) —
  §2a, the DOM-not-Scene decision this plan finishes realizing.
- [unified document host plan](2026-06-17_unified_document_host_plan.md) —
  the shell-document architecture (roster/list-panes/settings folding) this
  plan extends to the last two gloss sections.

---

## Progress

- **2026-07-01 (scoped).** Spun out of the gloss outline lens plan's Open
  Decision #5 once its P1 landed and was headed-verified. Design-reviewed by
  a Plan-agent (hybrid approach validated against the actual genet primitive
  set; a required a11y-bounds-migration step surfaced) before being written
  down. No code yet; P1 (recent → DOM) is the first build step.
- **2026-07-01/02 (P1-P3 landed, headed-verified, 247/247 tests green).**
  - **P1 (recent → DOM)** landed cleanly first pass.
  - **P2 (minimap → DOM node squares + embedded-Scene backdrop)** hit two
    real, reproducible rendering bugs, both traced and fixed via the new
    in-app capture harness below:
    1. `.gloss-minimap` carried `position: relative`, a *third* positioning
       level (`gloss-minimap-pane`[absolute] > `gloss-minimap`[relative] >
       `gloss-minimap-node`[absolute]) one deeper than the orrery's own
       gnodes (`orrery`[absolute] > `gnode`[absolute], no relative
       wrapper between) — corrupted the whole chrome document (toolbar/
       shellbar/roster/outline all went blank or partial the moment the
       minimap opened). Fix: drop `position: relative` (static positioning
       resolves the containing block to the pane wrapper directly, matching
       the orrery's depth).
    2. `.gloss-minimap` also carried an opaque `background-color`. Since
       that div is part of `chrome_scene`, composited *after* the backdrop
       `<external-texture>` (the edges/rings Scene) at the same rect, the
       opaque fill painted over every edge — only the node squares (drawn
       within that same opaque layer) survived. Symptom: minimap nodes
       showed correctly positioned, but with zero edges, even though the
       graph had them. Fix: drop the `background-color`; the backdrop's own
       `ColorLoad::Clear` already fills the identical panel color from
       underneath.
  - **P3 (a11y bounds migration + cleanup)** landed: `gloss_a11y_tree` now
    builds Minimap/Outline/Recent groups off live DOM-layout bounds (a new
    shared `dom_member_bounds` helper, factored out of the outline's
    pioneering version) instead of the retired `gloss_node_rects`/
    `gloss_recent_rects` caches; deleted those fields, `gloss_node_at`/
    `gloss_recent_at`/`gloss_leaf_rect` (`pane_geom.rs`), the bespoke Scene
    hit-test branch in `press.rs`, and the three now-redundant partial-pane
    checks (`gloss_outline_at`/`gloss_recent_dom_at`/`gloss_minimap_at`) —
    `PaneContent::Gloss` joined `chrome_routed_leaf_at`'s whole-leaf list
    instead, since all three sections are DOM-folded now.
  - **Harness fix (unplanned, load-bearing for the above).** The existing
    `SetForegroundWindow` + `CopyFromScreen` screenshot driver became
    unreliable mid-session — captures silently showed a different, unrelated
    topmost window instead of meerkat, with no error, because this
    environment's automation can't reliably hold OS window focus against
    its own host. Fixed two ways: (a) an in-app self-capture
    (`maybe_dump_chrome_capture` in `render/textures.rs`) that polls a
    `MEERKAT_CAPTURE_DIR/request.txt` file once per frame and, on a hit,
    reads the already-rasterized chrome texture straight off the GPU via
    the same `read_texture_rgba` path the snapshot-card feature already
    proves out — zero OS-compositor dependency, one `Path`-read per frame
    when unset; (b) `ffmpeg`'s `ddagrab` lavfi filter (DXGI Desktop
    Duplication) as the full-desktop fallback when the external-texture
    backdrop also needs to be seen, immune to the same focus/staleness
    issue GDI `CopyFromScreen` had. Both proved out and used to get every
    fix above to a real, confirmed-correct screenshot.
  - **Perf finding (Mark: "the app is pretty laggy," asked for robust
    diagnostics).** Added a permanent `meerkat::profile`-target trace in
    `PaneSession::scene` logging the rebuild/RepaintOnly decision alongside
    the existing per-frame `total_us`/`chrome_us`. Measured 150-500ms/frame
    (debug) and 155-270ms/frame (release) even fully idle after settling —
    not a runaway redraw loop (frame count stops growing), not primarily a
    debug-build artifact (release only ~2x better on the worst frames).
    `chrome_us` stayed 100-145ms even on `rebuild=false` (RepaintOnly, layout
    skipped) frames regardless of mutation count (16 mutations/frame cost
    about the same as 203) — pointing at paint-list emission scaling with
    total DOM size rather than the changed delta. That's a `genet-layout`
    question, out of scope for this session; worth its own investigation/
    plan given the shell document (roster + gloss + orrery gnodes) is
    only going to grow. See the [UI polish plan](../2026-09-02_retired_plans/2026-07-01_ui_polish_plan.md)
    §5 for where this finding now lives as design_docs record.
