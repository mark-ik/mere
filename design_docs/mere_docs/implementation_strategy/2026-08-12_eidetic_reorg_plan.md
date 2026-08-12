# Eidetic Reorg Plan

**Date**: 2026-08-12
**Status**: open; authorized by Mark 2026-08-12 ("you can reorg eidetic").
Execution is timed around the sibling sessions currently in mere's tree
(distillery v0, moothold): the moves touch the workspace manifest, so they
land as one commit when the tree quiets, with the search wiring plan's W4
consuming the new homes.

**Related**:
[search_surface_wiring_plan](2026-08-12_search_surface_wiring_plan.md) (W4
is the consumer of both moves),
[leverage census](../../2026-08-10_leverage_census_brief.md) (§2 rows this
plan resolves), the esp consolidation plan (whose split left the glue
behind).

## The finding this executes

`mere-embed` ("Mere's statistical-intelligence glue over esp::embed") holds
three live modules and has zero consumers. Its own description names the
two halves: eidetic persistence, and the quint field-algebra / canvas-search
bridge. Neither half's natural consumer imports the crate, and the crate
was **never published** (verified against the registry 2026-08-12), so it
can dissolve without a shim — cheaper than the vates/sibylla retirement.

Two facts make the moves clean:

- esp's default feature set is empty (serde-only, wasm-clean), so a
  default-features `esp` dependency is light enough for eidetic-core.
- eidetic-core's lib.rs already names "vector indices" as one of its
  own-schema lanes, beside model storage and browsing memory. The
  persistence move is the stated home arriving, not a new idea.

## Moves

- **E-R1 — `embed::persistence` → `eidetic-core`**, as a `vector` module
  behind a `vector-index` feature carrying the default-features esp dep.
  Save/load a `VectorIndex` through the typed-payload API beside the
  browsing and model-manifest lanes. Deliberately NOT eidetic-search:
  that crate's principle is zero engine dependencies (`fuse()` takes both
  rankings from the caller), and it keeps it.
- **E-R2 — `embed::field_bridge` + `embed::canvas_search` →
  `crates/canvas/canvas`**, behind a feature if canvas does not already
  carry quint. Their coupling is quint field algebra projected over the
  canvas — canvas-cluster code that was only ever parked in intel.
- **E-R3 — delete `mere-embed`.** Remove the crate and its workspace
  entries; sweep its doc references (the search plan's W4 paths update in
  the same commit). No published version exists, so no compatibility shim
  and no registry tombstone.
- **E-R4 — the fetchers stay crates.** `eidetic-https-fetcher` and
  `eidetic-iroh-fetcher` keep real dependency walls (reqwest; iroh) that
  must not enter eidetic-core; their wiring destinations are already named
  (gazette feed pipeline, mesh blob lane).

Non-moves, stated so the reorg has edges: muniment, codicil, chartulary,
scholia, tulpa, eidetic-fjall, and eidetic-search all stay put. The family
directory is coherent; the reorg is the dissolution of one orphan crate
into the two homes its halves always had.

## Done conditions

- `cargo test -p mere-eidetic --features vector-index` passes with the
  moved persistence round-trip tests.
- Canvas compiles with the bridge modules and their existing tests.
- `mere-embed` is gone from the tree and the workspace manifest; grep
  finds no `embed::` path outside the census/history record.
- The census §2 rows for `mere-embed` and the search plan's W4 wording
  point at the new homes.
