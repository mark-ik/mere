# Archived-plan tails — deferred items spun out of the archive passes

**Date**: 2026-07-03, extended 2026-08-06.
**Status**: backlog holder. Each item below was explicitly deferred by a plan that
is otherwise complete and now lives under
[`archive_docs/`](../../archive_docs/), in the checkpoint folder named by the
section it sits under. None of these gate anything today; pick up when the
relevant lane is quiet. Items already tracked by an active plan are *not*
repeated here.

This holds the tails from every pass rather than one per pass: a deferred item
is easier to find in one backlog than across a folder of dated stubs, and the
sections say which plan each came from.

## From native_surface_compositing (complete 2026-06-21)

- **wgpu-scry non-blocking capture settle** — `start_capture`'s ~500ms settle blocks
  the UI thread on a (now rare, backed-off) stall-restart; make it non-blocking in
  wgpu-scry. Needs demo runtime verification, not a blind edit.
- **Cache-flush per-tile submit batching** (flagged cleanup).
- **Silent implicit-sync fallback** when the explicit D3D12 fence fails — should
  warn + fail the tile under D3D12.
- **`favicon_data_uri` misnomer** — it also encodes the snapshot peek; rename.

## From documentscript_net_hardening (substantially complete 2026-06-24)

- **E1 refinement, uncredentialed same-origin fetch** — a same-origin `net.fetch`
  still carries that origin's own cookies; drop even those (Mark's session-store
  domain, A1 step 1).
- **E1 refinement, cross-origin `net` declarations** — a mod manifest declaring
  extra origins beyond its page, with broad-glob guards.
- **E2 — true non-blocking fiber suspension** for `net.fetch`: dispatch onto the
  off-thread fetch actor and resume the fiber, so the content actor keeps servicing
  commands during I/O (A2's 30s timeout mitigates today).
- **E3 — mod install/approval + optional signing** (the proper B1 fix; today any
  approved-capability `.wasm` in `mods/` auto-attaches).
- **B4 — `.cwasm` AOT mods invisible to discovery** (loader supports them; discovery
  matches only `*.wasm`).
- **D2 — `Fetched` carries no HTTP status** (non-2xx collapses to `Err`; guests
  can't observe 404/3xx). Touches `fetch.rs`.
- **DNS-rebinding defence** — the SSRF guard is a literal-host deny-list;
  resolve-then-pin is the follow-on.

## From find_in_page_host_ui (feature complete 2026-06-16)

- **Gemtext/document-lane find — closed 2026-07-03.** The user-facing retained-text
  slice landed in the
  [retained_text_tiled_render_plan](../../archive_docs/2026-07-04_completed_plans/2026-06-15_retained_text_tiled_render_plan.md):
  Ctrl+F searches retained document blocks, paints block-scoped highlights, and
  page-text copy works from the retained packet. The precise glyph→char cluster
  map on `GlyphRun` remains only a fidelity follow-on, not an open backlog tail.
- **Paste into the find field** — `handle_clipboard_shortcut` routes to
  omnibar/palette only.

## From context_submenus (implemented 2026-06-25)

- **GUI feel verify pass** — pixel placement of the flyout, live hover/keyboard
  feel (Mark-runs-it; logic + DOM are tested).
- **Hover-open with delay** (cursor-move handler + an `Instant`-timed
  `submenu_hover` field).
- **Submenu mis-anchor when the root menu is scrolled** (`row_y` ignores the root's
  scroll offset; narrow case).
- **Mouse hit-test after keyboard-scroll** uses unscrolled offsets (pre-existing for
  the flat menu).
- **Depth-N submenus** — model is depth-1 by convention; deeper needs a path, not an
  index.

## From keyed_view_sequence (implemented 2026-07-02)

- **P4 — `ElementSplice` move primitive** for state-preserving arbitrary reorder.
  Gated on ui_polish finding-5 (paint-list emission cost) landing first, so
  delete+reinsert's real cost is separately visible. Plus OQ-1 (linear key lookup
  vs hash index — profile first) and OQ-2 (duplicate-key policy).

## From engram_compose_merge (P1–P3 done 2026-06-30)

- **rkyv compaction of graph engrams** (Alembic tail B6) — override
  `TypedPayload::serialize_to_bytes` for `GraphEngram` with rkyv (mind the
  read-alignment `AlignedVec` gotcha). Measure the size win first.
- **Promote `merge_snapshots` into the kernel** (`Graph::merge_from`, reusing the
  URL index + edge API) once the kernel is quiet — the snapshot-level merge was the
  non-colliding path, not the final shape.

## 2026-07-04 archive pass (12 more plans → `archive_docs/2026-07-04_completed_plans/`)

### From document_style_sheet (P0-P4 complete 2026-06-22, inker)

- **Container + non-text roles** (Quote / List) — own plan when consumer
  pressure appears; deliberately not built at the tail of P4.
- **Visual arrow check** on document-lane sheet arrows was deferred at closeout.

### From document_typography_surface (D1-D3 shipped 2026-06-22, inker)

- **D4 — per-role + per-engine typography overrides** (advanced section) and
  **full font enumeration**. The seed-palette plan's "surface per-role document
  knobs in settings" deferral is the same item; do them together.

### From surface_engine_contract_fold (complete 2026-07-04, inker)

- **`SecondaryForward` rename** — the flip shim kept the old `ProducerSurface`
  name while becoming generic over `WebSurface`; rename when touched (cosmetic).

### From retained_text_tiled_render (acceptance met 2026-07-03)

- **Image-only inline links** — an `<a>` wrapping only a replaced element
  establishes no box and is not harvested.
- **Per-band link re-harvest caching** — links re-harvest on every band
  re-emit; cache once a tall many-link page measurably bites.
- **Glyph→char cluster map on `GlyphRun`** — upgrades document-lane find/select
  from block-scoped to exact intra-line geometry (fidelity, not function).

### From seed_palette_theme_system (complete 2026-06-22)

- **TOML swap** for the theme file format (kept format-agnostic); **rescan-on-
  demand** for theme packs (startup-only today).

### From gnode_pool (landed 2026-07-02, follow-ons resolved 2026-07-03)

- No open gnode-pool debt. The residual (loaded-session `chrome_us` /
  `chrome_raster_us` on legitimately-dirty frames) is owned by genet's
  `docs/2026-07-03_shell_paint_emission_raster_plan.md`; the like-for-like
  loaded-session capture is that plan's motivating measurement.

No tails: chrome_bar_refinement (its deferred switcher cleanup was completed
in-plan), ui_dpi_scaling, gloss_scene_to_dom (follow-ups owned by the active
gloss_outline_lens plan), graphlet_wiring (cross-plan leftovers owned by the
active relational_browse plan), tearout_composability (continuation is the
active tearout_gestures plan).

## Progress

- **2026-07-03** — created during the archive/reconcile pass; 11 completed plans
  moved to `archive_docs/2026-07-03_completed_plans/`, their deferred items
  collected here.
- **2026-07-04** — reconciled the find-in-page carry-over: the document-lane
  retained-text find/copy acceptance slice is now closed in the retained-text
  plan, while paste into the find field remains open.
- **2026-07-04** — second archive pass: 11 more completed plans moved to
  `archive_docs/2026-07-04_completed_plans/` (joining the concurrently-archived
  misfin promotion plan); their tails added in the 2026-07-04 section above.

## From stickleback_replication_promotion (complete 2026-07-27, archived 2026-08-06)

- **A sibling repository for `stickleback`, gated on a real external consumer.**
  The promotion deliberately stopped inside Mere: `stickleback` 0.1.0 is
  published from this repository under MIT OR Apache-2.0, and S3 passed the
  publishable-boundary review, so nothing technical blocks the move. What is
  missing is a reason. The plan's own rule was that a domain-neutral crate earns
  a sibling repo when someone outside this workspace depends on it, not when it
  merely could. Revisit when an external consumer appears, and not before.

## From forest_dom (landed 2026-07-18, archived 2026-08-06)

- **F4, per-window multi-DPI.** Per-window DPI, viewport, and cascade, deferred
  on purpose rather than missed: the plan's own instruction was not to
  gold-plate the per-window cascade before F3 had banked the topology, and F3
  has. Revisit when a real multi-monitor case wants it, which is also the only
  situation that can say whether the cascade needs to be per-window at all.

