# Archived-plan tails — deferred items spun out of the 2026-07-03 archive pass

**Date**: 2026-07-03.
**Status**: backlog holder. Each item below was explicitly deferred by a plan that
is otherwise complete and now lives in
[`archive_docs/2026-07-03_completed_plans/`](../../archive_docs/2026-07-03_completed_plans/).
None of these gate anything today; pick up when the relevant lane is quiet. Items
already tracked by an active plan are *not* repeated here.

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
  [retained_text_tiled_render_plan](2026-06-15_retained_text_tiled_render_plan.md):
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

## Progress

- **2026-07-03** — created during the archive/reconcile pass; 11 completed plans
  moved to `archive_docs/2026-07-03_completed_plans/`, their deferred items
  collected here.
- **2026-07-04** — reconciled the find-in-page carry-over: the document-lane
  retained-text find/copy acceptance slice is now closed in the retained-text
  plan, while paste into the find field remains open.
