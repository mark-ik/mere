# H3 local graph product receipt

**Date:** 2026-07-28  
**Plan gate:** H3, ship the local graph product  
**Result:** passed

## Product cut

Graphshell now owns the daily local graph loop in its WASM-safe profile:

- create addressed objects and content-addressed file objects;
- edit titles, normalized tags, arbitrary JSON facets, and typed relations;
- search graph metadata and select a relation family;
- choose an arrangement, pause physics, and apply a representation;
- save and reopen the working scene;
- select an open handler and hand web content to the host browser;
- export and open an object, its direct relations, a selected subgraph, or a
  saved scene and selection.

The browser presenter uses the same operations in `product.rs`; it does not
carry a parallel graph model.

## Portable transfer boundary

The selected unknown file was addressed by
`urn:sha256:dc0e849709554214fb6e5b6b33956c297721c8ed4f98bca072972e24dc0d3f0e`.
Its content hash, byte length, tags, and `research.note/v1` facet crossed the
engram boundary. The `graphshell.local-file/v1` facet did not cross because
the device-local metadata setting was off.

The headed selected-subgraph export contained two stable node ids, one
`Cites` relation, the content and research facets, a two-object saved
selection, the grid arrangement, paused physics, the configured
`system.default` handler, and one sprite face. Opening the export replaced the
working graph with two objects and preserved that scene state.

The native product test adds the remaining provenance case: four objects, two
relations, and thirteen facets round-trip while selected ids, semantic and
provenance relations, unknown facets, and sprite state remain equal.

## Headed receipt

Chromium exercised the complete workflow at 1280×800 and again at 600×800.
The narrow run exposed and then fixed an overflowing three-column product
panel; its final client width and scroll width are both 563 CSS pixels.

The wide run also selected `system.default` for
`https://example.test/i2p-port`. Graphshell accepted the typed intent and
opened that exact address in a host-browser tab. The test domain's DNS failure
occurred after the handoff and is outside Graphshell.

- [wide Chromium screenshot](receipts/h3_chromium_wide.png)
- [narrow Chromium screenshot](receipts/h3_chromium_narrow.png)
- [structured browser receipts](receipts/h3_browser_receipts.json)

H2 remains the cross-engine presenter proof in Chromium and Firefox. H3 adds
Chromium product behavior; this receipt does not claim a new Firefox product
run.

## Evidence

- Native dependency-cone harness:
  `H3 graph product round-trip PASS: 4 objects, 2 relations, 13 facets`.
- `graphshell-web` checked and built for `wasm32-unknown-unknown`; the generated
  WebAssembly was rebound into `web/pkg`.
- Headed Chromium passed create, edit, relation, representation, filter,
  arrangement, physics, scene reopen, external handoff, export, and import.
- The 600px repair was reloaded from the served stylesheet and verified with
  equal panel client and scroll widths.

The full in-repo test was also requested during this pass. Concurrent
workspace builds and the restored Git dependency manifests kept that rerun
from resolving cleanly in its isolated Cargo home. The completed native
harness and isolated full WASM build are the executable receipts for this cut;
this note does not claim a patch-free in-repo test run.
