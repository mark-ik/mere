# Search Surface Wiring Plan

**Date**: 2026-08-12
**Status**: open. Spun out of the
[leverage census](../../2026-08-10_leverage_census_brief.md) (step 2), and
carries the census's audit answer for `mere-embed` inside it.

**Related**: eidetic-search's own crate docs (Phase 9, producer half), the
esp consolidation plan (the split that left embed's glue behind), the
2026-05-08 local-intelligence integration research (the architectural
anchor embed's lib.rs cites).

## 1. Audit results (verified 2026-08-12)

- **`mere-embed` is not a husk; the census's "retire" branch is closed.**
  It is the re-export shim over `esp::embed` plus three genuinely
  mere-coupled modules, all built and all unwired: `persistence`
  (save/load a `VectorIndex` through eidetic's typed-payload API),
  `field_bridge` and `canvas_search` (project query similarity into
  quint's field algebra over the graph canvas). Zero importers means
  capability awaiting wiring, not deadness. Keep, and wire below.
- **`eidetic-search` is the lexical half, ready.** `TrailIndex` minted
  *from* `BrowsingTrace` engrams (derived state, re-mintable, format
  version carried with the index): BM25 recall over titles/URLs/domains,
  fast-field reports (`top_domains`, `visits_histogram`), and `fuse()`,
  the engine-agnostic reciprocal-rank seam that deliberately takes both
  rankings from the caller.
- **The missing precondition is capture.** `BrowsingTrace` exists only
  inside eidetic-core (model, tests, example). Turnstone authors none.
  Recall without a corpus returns nothing, so the first slice is capture,
  not search UI.

## 2. Slices

- **W1 — capture.** Turnstone authors `BrowsingTrace` engrams at
  navigation commit points (the observe/session-lifecycle seam),
  persona-scoped, into the existing eidetic store. **Done when** a real
  browsing session yields traces that re-mint into a `TrailIndex` (run
  eidetic-search's `eidetic-recall` example against the real store).
- **W2 — recall in the omnibar.** A non-privileged omnibar lane queries
  `TrailIndex::search`; hits render as actionable rows (open the page /
  summon the node). Index staleness is surfaced honestly and
  `FormatMismatch` re-mints rather than erroring, per the crate's own
  doctrine. **Done when** "where did I read about X" answers from the
  user's own trail.
- **W3 — reports.** The trail/steward surface renders `top_domains` and
  `visits_histogram` from the fast-field columns (no re-index needed).
  Small; may ride W2's session.
- **W4 — canvas semantic search.** Wire `embed::canvas_search` +
  `field_bridge` into mere-canvas: a query becomes a similarity field
  over the canvas through quint, with `persistence` saving the
  `VectorIndex` via eidetic. Start on the lexical embedding provider
  (deterministic, no Burn), `bert` behind its existing feature per esp's
  target matrix.
- **W5 — fusion.** `fuse()` merges W2's lexical ranking with W4's vector
  ranking in the omnibar. Gated on both.

## 3. Non-goals

- The moot consume-half of `SearchIndexSpec` (deferred by its own doc).
- A crawl-driven corpus (`mere-crawl` stays parked pending the gazette
  feed pipeline, per the census).
- New embedding backends beyond what esp already ships.

## 4. Sequence

W1 first; W2 and W3 follow it; W4 is independent of W2/W3 and may
interleave; W5 last. Each slice lands with its own receipt against a real
store, not fixtures only.

## 5. Progress

- **2026-08-12 — W1 landed** (turnstone `539dacc`). The trail-memory port
  mirrors the recycle bin's actor shape exactly: session-scoped
  `FjallStore` at `sessions/<id>/memory`, `BrowsingTrace` segments through
  `BrowsingMemory`, `from` chained per owner inside the actor, flush on
  segment fill and every lifecycle edge (switch, close, release — the
  release rides the same Windows rename handshake as the bin, since the
  memory store lives in the session dir). Capture rides the observation
  drain as designed: the shell's `drain_app_events` is the first
  production consumer of `App::take_events`, mapping
  AddressOpened/NavigatedBack/NavigatedForward/Reloaded onto
  UrlTyped/Back/Forward/Reload with the root identity's public key hex as
  the owner tag. Three unit tests green (round trip with origin chaining,
  self-flush on a full segment, event mapping); lib check clean. Still
  open in W1's done-condition: the headed receipt — a real browsing
  session's store re-minted through eidetic-search's `eidetic-recall`
  example. W2 is unblocked.
- **2026-08-12 — W1 done condition closed** (turnstone `f22f61f`). Building
  the receipt exposed one real gap: nothing flushed on a normal quit, so a
  short session would have left an empty store. `ApplicationHandler::exiting`
  now releases the trail store under a bounded ack (the scenario driver's
  Done exits through the same hook). The receipt itself
  (`scenarios/trail_capture.scn`, fresh profile): three `mere://`
  navigations through the real shell landed as **1 trace, 3 traversals, 3
  distinct pages**; `eidetic-recall index` minted a 3-document trail index
  from the session's store; `search alpha` answered `mere://alpha` at 0.98
  with the capture-time timestamp. "Where did I read about X" answers from
  a real session's store. W1 is complete.
