# Meerkat render-path refactor + GUI performance plan

**Date:** 2026-06-24
**Status:** plan. Spun out of the grand audit (`repos/serval/docs/2026-06-24_grand_audit.md` §3 + §4).
**Thesis:** meerkat's hot path is a single ~1700-line `render()` method that violates Mere's 600-LOC ceiling, repaints unconditionally on every actor wake, and rebuilds per-frame scene/state/allocation work with no dirty-gating. The transform-motion relayout fear is retired (serval spike 2026-06-01, restyle stays RepaintOnly), so the budget is scene rebuilds, allocations, and redraw cadence, not layout. This plan splits the monolith, then closes the redraw/allocation hotspots, then lands per-surface scenes (which both fixes a correctness wart and unblocks multi-window-same-graph).
**Related:** `2026-06-19_tearout_composability_plan.md` (multi-window fan-out folds there), `2026-06-23_render_ladder_and_extraction_plan.md` (HTML-lane parity folds there), the scrying tile plan (`2026-06-10_scrying_tile_plan.md`, the per-tile cache-flush this plan batches).

## Phases (done-conditions, not dates)

### M1 — Split the hot path under the 600-LOC ceiling

- Today: `render.rs` 2001 LOC with `render()` a ~1700-line method (`render.rs:299-1999`); `input.rs` 2405; main.rs 1862, content.rs 1300, window_view.rs 1186, card.rs 1100, constellation.rs 994 all over ceiling.
- Work: factor `render()` into staged passes (pane-layout, orrery-snapshot, workbench-projection, content-card-banding, compositor, decorations); do the same for `input.rs`. Wire per-pane profile timers while here (the C0 harness emits `total_us`/`chrome_us` at `render.rs:1988-1993` but lacks per-pane granularity).
- **Done when** no meerkat source file exceeds 600 LOC on the hot path and the staged passes are independently testable.
- Prerequisite for every move below; they all edit `render()`.

### M2 — Dirty-gate the redraw loop

- Today: `user_event` ends with an unconditional `wc.view.request_redraw()` on every actor wake (`app_handler.rs:441-446`), so any fetch/sync/comms/find/physics poke repaints the whole window even when nothing visible changed; a second driver on top of the orrery's own `orrery_redraw` self-sustain (`render.rs:1995-1998`).
- Work: gate `request_redraw()` on the `card_changed`/`graph_changed`/`comms_changed` flags that already exist immediately above (`app_handler.rs:438-440`); let the orrery settle drive itself.
- **Done when** an idle/settled window holds near-zero repaint cost; this is the largest idle CPU/GPU win and the treadmill every per-frame cost below rides on.

### M3 — Kill per-frame rebuilds and allocations

- Orrery scene + node state/shape: `render()` calls `node_states()` + `node_shapes()` unconditionally every frame (`render.rs:573-578`, `node_states` a third time at `:852`), each allocating a fresh HashMap with per-node `pages.get` (`node_ops.rs:275-311`); `Orrery::frame()` rebuilds the arrangement, reprojects all positions, culls, and rebuilds the underlay every call even when settled (`frame.rs:36-126`). Add a settled-scene cache keyed on generation/camera; track node_states/shapes incrementally or cache until pages/activation change; compute once, reuse the third read.
- Favicons + per-card strings: the `orrery_cards` closure (`render.rs:631-714`) calls `favicon_data_uri` (PNG encode + base64) for every visible node every frame (`:699-701`), plus per-card label `String`/`color.to_string()`/hull clone, all paid before the `PartialEq` diff gate (`:722`). Cache the favicon data-URI per `(node, favicon-version)` like `snapshot_data_uris` already does (cap 256, `:1632`); move the diff gate earlier so unchanged cards skip the allocation-heavy path.
- **Done when** a settled large graph does O(0) arrangement rebuilds and no per-frame favicon re-encode (verified by the per-pane timers from M1).

### M4 — Per-surface scenes

- Today: one `Activation` holds one scene/packet/band per member (`constellation.rs:56-133`), so the focus card is suppressed when the focused node is an open tile because a second card would drive the one content actor at a different viewport size (last-writer-per-frame thrash; "the proper fix is per-surface scenes; until then the tile wins", `render.rs:1093-1101`).
- Work: generalize `Activation` to a small per-`(member, viewport)` scene set so a node renders as a live tile and a focus card at once without thrash.
- **Done when** a node shows live in a tile and as a focus card simultaneously with no reflow on card open; also the prerequisite for clean multi-window-same-graph.

### M5 — Cheap compositing redundancies

- Overlay textures: while find is open, two 1x1 amber overlay scenes are rasterized fresh every frame (`render.rs:1566-1588`), unlike the cached `divider_tex`/`window_controls_tex`. Cache them like the other decoration textures.
- Scrying flush batching: `drive()` issues a separate `create_command_encoder` + `queue.submit` for a 1x1 cache-flush per live tile per redraw (`scrying_host.rs:533-563`, flagged deferred in `2026-06-19_native_surface_compositing_plan.md`); batch the per-tile flushes into the main encoder for one submit/frame.
- Netrender tail: external-texture interleaving re-renders the whole scene tail into a full-viewport scratch per boundary (`repos/netrender/netrender/src/renderer/mod.rs:1449-1481`); prefer the topmost-overlay path where surfaces allow. (Cross-repo; netrender side.)
- **Done when** find-overlay textures are cached, scrying does one submit/frame regardless of tile count, and the netrender tail-redraw is avoided on the focused-card path.

## Items that fold elsewhere (not duplicated here)

- **Multi-window fan-out (MW3 5/6):** sync/comms writes target the primary runner (`app_handler.rs:324-349`), secondaries lack an AccessKit bridge and present full chrome not the slim leaf (`app_handler.rs:826-848`; `window_view.rs:42-53` Leaf is a bare marker). Tracked in `2026-06-19_tearout_composability_plan.md`.
- **HTML-lane link + find parity (render ladder Phase 5):** `link_at` walks an empty `Activation.links` Vec for the HTML lane (`constellation.rs:643-657`); the lane rasterizes one capped texture until parity (`render.rs:1286-1287`). Tracked in `2026-06-23_render_ladder_and_extraction_plan.md`.
- **Scripted/extraction lane (pump-before-extract, keyboard dispatch, crawl frontier):** tracked in the document-script + extraction plans; keyboard dispatch is gated on a serval focus model ("not thin after all").

## Sequencing

M1 first (unblocks all). M2 next (biggest idle win, smallest change). M3 + M5 are independent allocation/submit cleanups. M4 is architectural and also a §4 perf item; do it once M1 has made `render()` tractable.

## Findings

- 2026-06-24 (grand audit, verified): file LOC and line refs above are code-grounded. The transform-motion relayout fear is retired (serval spike 2026-06-01: transform/translate declares `recalculate_overflow`, not RELAYOUT; stays RepaintOnly), so perf budget is rebuilds/allocations/cadence, not layout. Per-pane profiling is half-built.

## Progress

- 2026-06-24 — Plan created from the grand audit. No code yet. M1 is the entry point.
