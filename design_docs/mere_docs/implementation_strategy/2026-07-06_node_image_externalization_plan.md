# Node image externalization plan — preview imagery to content-addressed blobs

Status: **Planned.** Motivated by a measurement: the petgraph-RDF plan's Phase 4
footprint probe (`crates/probes/rdf-kernel-footprint/`, see that plan's Phase 4
gate note) found that at 50k nodes the kernel's live heap is **64% inline image
bytes** (`Node::thumbnail_png` + `Node::favicon_rgba`), dwarfing every other
category and dwarfing what a term dictionary could reclaim (~1%). This plan moves
that imagery out of the kernel into the durable content-addressed blob store the
memory model already runs, leaving a small reference in the node. Target: live
graph footprint at 50k nodes drops from ~553 MiB to the low 200s, with a bounded
render-side image cache in place of every node holding its pixels forever.

Cross-refs:

- [petgraph_rdf_plan](2026-06-18_petgraph_rdf_plan.md) — the Phase 4 gate note is
  the measurement that motivates this; that plan ranked image externalization as
  the #1 footprint lever (~64%), ahead of `EdgePayload` slimming and interning.
- [alembic_memory_and_engrams](../technical_architecture/2026-06-09_alembic_memory_and_engrams.md)
  and [alembic_implementation_plan](2026-06-24_alembic_implementation_plan.md) —
  the three-level memory model and the eidetic store this rides on.
- [athanor_steady_heat_actor_plan](2026-06-25_athanor_steady_heat_actor_plan.md) —
  the forgetting daemon that will garbage-collect orphaned image blobs.
- [meerkat_render_perf_plan](2026-06-24_meerkat_render_perf_plan.md) — the render
  path whose card-raster cost the decoded-image cache must not regress.
- [two_natured_kernel_brief](../research/2026-05-30_two_natured_kernel_brief.md) —
  the truth-vs-experience cut this applies: preview imagery is experience.

---

## Why this shape (the decision basis)

- **Preview imagery is experience, not truth.** A favicon or a page thumbnail is
  a derived, re-fetchable, re-derivable view aid. The two-natured kernel keeps
  truth native and pushes experience out. Holding 50k nodes' pixels in the graph
  (the truth, retained for every node for the whole session) puts derived data in
  the authority. Moving it to a bounded cache is where derived imagery belongs, so
  this is a correctness alignment, not only a memory trick.
- **The pattern already exists — reuse it, do not invent it.** Fetched page
  **bodies** are already externalized: they live in the eidetic blob store under
  `content/body/<url>` (`session-runtime/src/content_store.rs`), not inline in the
  node, and the node reaches them through `self.shared.content.store` with a
  `pollster::block_on` whose fjall futures are ready (`meerkat/src/node_ops.rs`
  `load_cached` / `save_cached`). Images are the same problem with the same
  solution: raw bytes in the blob store, a small reference in the node.
- **Content-addressed (BLAKE3), because the whole store already is.** eidetic
  content-addresses everything through `Hash::of(bytes)` / `ManifestId::from_hash`
  (`eidetic-core/src/engram.rs`), and iroh verifies BLAKE3 natively for sync
  (`manifest.rs`). Keying an image blob by the hash of its bytes buys three things
  for free: **dedup** (every github.com node shares one favicon blob), integrity,
  and sync portability (a forked or synced node's image reference resolves by the
  same hash, fetchable as an iroh blob).
- **Authored `body` stays inline, deliberately.** `Node::body: Option<String>` is
  a knot-note's djot source (authored in place), which must travel on snapshot /
  sync / fork and is small. It is truth, not a cache, so it is out of scope here.
  Fetched nodes already carry `body: None`. This plan touches only the two derived
  image payloads.
- **Forgetting already has an owner.** Athanor (`session-runtime/src/athanor.rs`)
  proposes and applies eviction of short-term cached content under the visible
  memory policy (`memory_levels.rs`), never touching graph truth or engrams.
  Orphaned image blobs are the same kind of droppable cached derivative, so their
  GC is an Athanor pass, not a new subsystem.

## The change, in one picture

Today (per node, held for the whole session in the kernel truth):

```
Node { … thumbnail_png: Option<Vec<u8>>, thumbnail_width/height: u32,
          favicon_rgba:  Option<Vec<u8>>, favicon_width/height:  u32, … }
```

After:

```
Node { … thumbnail: Option<ImageRef>, favicon: Option<ImageRef>, … }
ImageRef { hash: Hash, width: u32, height: u32 }   // ~40 B, no pixels

// pixels live once, content-addressed, in the eidetic blob store:
//   content/image/<blake3-hex>  ->  PNG bytes
```

The kernel holds references (a few MB across 50k nodes). The pixels live once each
in the durable store (dedup collapses shared favicons). The render layer decodes
only what it is about to show, into a bounded cache.

**Unify on PNG-in-blob.** The favicon is stored raw RGBA today but the render path
already re-encodes it to a PNG data URI (`textures.rs` `favicon_data_uri` →
`png_bytes_from_rgba`). Storing both thumbnail and favicon as PNG in the blob lets
the render path treat them identically (`png_data_uri` over the loaded bytes, no
re-encode), shrinks on-disk size, and keeps `ImageRef` format-free (the blob is
always PNG; the reference just carries the decoded dimensions).

## Phases

Each phase states its done-condition (a checkable property), not a duration.

### Phase 1 — Content-addressed image blob store + `ImageRef`

- New `session-runtime/src/image_store.rs`, mirroring `content_store.rs`:
  `save_image(store, png_bytes) -> Hash` (content-addressed via `eidetic::Hash::of`,
  written under `content/image/<hash>`), `load_image(store, hash) -> Option<Vec<u8>>`,
  `delete_image(store, hash) -> bool`. Saving the same bytes twice is one blob.
- `ImageRef { hash, width, height }` as the small handle. Home: the kernel
  (`kernel::types`) since `Node` holds it and `PersistedNode` persists it, with the
  hash stored as bytes/hex so the kernel does not take an eidetic dependency it
  should not (mirror the existing `UuidAsBytes` treatment; a plain 32-byte array +
  hex is enough — the kernel never hashes, it only carries the handle).
- **Done when:** a round-trip unit test over an in-memory `Store` (the `MemStore`
  pattern already in `content_store` / `athanor` tests) shows save→load returns the
  bytes, two saves of identical bytes produce one blob at one key, and distinct
  bytes produce distinct keys.

### Phase 2 — Kernel `Node` swap + persistence + migration

- Replace the six inline fields (`thumbnail_png`, `thumbnail_width`,
  `thumbnail_height`, `favicon_rgba`, `favicon_width`, `favicon_height`) with
  `thumbnail: Option<ImageRef>` and `favicon: Option<ImageRef>` on `Node` and
  `PersistedNode`; update `snapshot/to.rs`, `snapshot/from.rs`, and the accessor
  sites listed in Findings.
- **Migration.** Existing `graph.json` snapshots carry inline PNG/RGBA. On load,
  detect the legacy fields (keep them as `#[serde(default)]` deserialize-only
  shadow fields), write their bytes to the image store, and populate the new
  `ImageRef`. One-time, lossless, and it runs through the same host that already
  holds the store at load. A snapshot with no images migrates trivially.
- **Done when:** a graph with a thumbnail and a favicon survives
  snapshot→load→re-render with the images intact; a legacy snapshot (inline bytes)
  loads, externalizes, and re-saves with references only and no pixels in the JSON;
  the snapshot-size gate (`kernel graph::tests::snapshot_size`) shows the per-node
  snapshot no longer carries image bytes.

### Phase 3 — Write sites store to the blob, keep a reference

- The capture / import / favicon paths that set image bytes today
  (`browse_capture.rs`, `import/web_clip.rs`, favicon fetch, `node_ops.rs`,
  `graph_engram.rs` — see Findings) call `save_image` and set the returned
  `ImageRef` on the node instead of moving `Vec<u8>` inline. Favicon capture
  encodes RGBA→PNG once at store time (reusing `png_bytes_from_rgba`).
- **Done when:** capturing a thumbnail or favicon writes one blob and leaves only a
  reference on the node; no write path constructs an inline image `Vec<u8>` on a
  `Node` anymore (enforced by the field simply no longer existing).

### Phase 4 — Render resolver + bounded decoded-image cache

- The card builder (`render/cards.rs`, `render/paint.rs`) resolves an `ImageRef`
  to a data URI through a new resolver that: checks a bounded in-memory cache keyed
  by hash; on a miss, `block_on(load_image(store, hash))` (the ready-fjall-future
  pattern `load_cached` already uses), wraps via `png_data_uri`, and inserts into
  the cache. The cache holds the encoded data URI (or PNG bytes) for recently shown
  nodes, evicted LRU under a byte bound.
- This is the one perf-sensitive seam: the bound replaces "every node's pixels,
  forever" with "the visible working set." It composes with the existing per-surface
  netrender tile caches (render perf plan) rather than replacing them.
- **Done when:** visible cards render their imagery identically to today; the
  meerkat render-perf harness shows no regression on the steady-state card-raster
  path; the cache respects its byte bound (an eviction test).

### Phase 5 — Orphan GC as an Athanor pass

- Image blobs are shared (content-addressed), so eviction is **mark-sweep against
  live references**, not per-URL drop: an image blob is droppable only when no live
  node's `ImageRef` names its hash. Add `propose_image_gc` (pure: diff the store's
  `content/image/*` keys against the set of hashes referenced by the snapshot's
  nodes) and `apply_image_gc` (drop the unreferenced blobs) alongside Athanor's
  existing forgetting pass, honoring the same R0 propose/apply split and the same
  "never drop what a long-term node references" guard.
- **Done when:** a blob referenced by any node (short- or long-term) is never
  proposed; a blob no node references is proposed and dropped; re-running the pass
  is a safe no-op.

### Phase 6 — Re-measure (the receipt)

- Re-run `crates/probes/rdf-kernel-footprint/` with images externalized: the
  probe's "images" category should collapse from ~352 MiB to the reference size
  (~2 MB of `ImageRef`s), moving the graph's live truth footprint at 50k nodes
  from ~553 MiB into the low 200s, with the render cache accounting for the
  visible working set separately and boundedly.
- **Done when:** the probe confirms the drop, recorded in this plan's Progress and
  the petgraph-RDF Phase 4 gate note.

## Design space (configurability, not baked defaults)

Per the "expose design choices as settings, track the full space" discipline:

- **Render image-cache bound** — byte ceiling or card-count for the decoded-image
  cache. Ties into the memory-levels UI (a visible knob), and shrinks first under
  memory pressure.
- **On-disk image format** — PNG (smaller, needs decode) is the default;
  raw-RGBA-in-blob (larger, zero-decode) is a latency/space trade to expose if the
  decode cost ever shows up. `ImageRef` is format-agnostic, so this stays a store
  policy, not a node change.
- **Dedup** — content-addressing dedups by construction; there is no "off," but the
  GC cadence (how eagerly Athanor sweeps orphans) is the tunable.
- **Thumbnail capture policy** — which nodes get a thumbnail at all (all fetched
  pages vs a subset) is an upstream capture setting that this plan does not decide;
  it only changes where the bytes land.

## Risks

- **wasm / OPFS resolve is genuinely async.** On native, fjall futures are ready so
  `block_on` does not stall (today's `content_store` reality). On wasm the OPFS
  store's futures are not ready, so a render-time `block_on` could stall the frame.
  Mitigation: on wasm, resolve through an async prefetch (kick the load when a card
  enters the viewport, render a placeholder until the cache is warm) rather than
  blocking. Native keeps the synchronous path. Flag this early; it is the one place
  the two hosts diverge.
- **Migration must not lose imagery.** The legacy-field shadow-deserialize path is
  load-bearing; guard it with a test that a real pre-migration snapshot round-trips
  its images. Back up `graph.json` before the first externalizing save (the
  petgraph-RDF plan's persistence-migration risk applies here too).
- **GC correctness.** Dropping a blob a node still references would blank an image.
  The sweep must diff against *all* live nodes (every graph, if multi-graph), and a
  reference appearing after a proposal but before apply must be safe (re-check at
  apply, or only drop blobs unreferenced across a full snapshot).
- **Render latency on cold cache.** First show of a node after load pays one blob
  read + decode. The bounded cache keeps the steady state hot; the risk is a
  scroll/pan across many cold nodes. Measure on the render harness; prefetch on
  viewport-enter if it bites.
- **Reference churn on re-capture.** Re-capturing a changed favicon mints a new
  hash and orphans the old blob (GC reclaims it). Expected, not a leak, but it means
  the store grows until Athanor sweeps; keep the sweep cadence visible.

## Findings (the investigation behind this plan)

Verified against the code, 2026-07-06:

- **Blob substrate exists.** `eidetic::Store` (`load_blob` / `save_blob` /
  `delete_blob` / `iter_keys`, async, fjall on native and OPFS on wasm, wasm-clean)
  is the durable KV the memory model runs on.
- **The externalization pattern exists.** `content_store.rs` stores fetched page
  bodies raw under `content/body/<url>`, and `node_ops.rs` reaches them via
  `self.shared.content.store` + `pollster::block_on` (ready fjall futures, no UI
  stall). Images mirror this exactly, keyed by hash instead of URL.
- **Content-addressing is BLAKE3.** `eidetic::Hash::of` / `ManifestId::from_hash`
  (`engram.rs`, `manifest.rs`); iroh verifies BLAKE3 natively, so hash-keyed image
  blobs are sync-portable.
- **Forgetting has an owner.** Athanor's `propose_forgetting` / `apply_forgetting`
  (R0 propose/apply, short-term only, promoted-exempt, never graph truth or
  engrams) is the model the image-orphan GC extends.
- **`body` is authored truth, not a cache.** `Node::body` is knot-note djot source,
  inline by design (must sync/fork); fetched nodes carry `None`. Excluded from this
  plan. Not a footprint concern (the probe measured images, and body is small and
  minority-populated).
- **Consumer path.** `render/textures.rs` turns image bytes into `data:image/png`
  URIs embedded as `<img>` in the orrery card that serval renders
  (`favicon_data_uri`, `png_data_uri`). The resolver slots in right here.
- **Field / accessor sites to change (23 files touch the image fields).** Kernel:
  `graph/node.rs`, `graph/node_props.rs`, `graph/mod.rs`, `persistence.rs`,
  `graph/snapshot/{to,from}.rs`, `graph/cross_graph.rs`, `graph/capture.rs`, plus
  the snapshot tests. Meerkat: `node_ops.rs`, `render/{cards,paint,orrery_scene,textures}.rs`,
  `window_view/gnode_pool.rs`, `graph_delta_log.rs`, `frame_ops/config.rs`,
  `browse_capture.rs`. Session-runtime: `graph_engram.rs`, `memory_levels.rs`
  (test node builder). Import: `web_clip.rs`. Orrery: `frame.rs`. This is the
  change surface Phase 2/3 walk.

## Progress

- **2026-07-06** — Plan authored from the Phase 4 footprint measurement plus a
  codebase investigation. Key outcome of the investigation: the entire blob /
  content-address / forgetting substrate already exists (eidetic `Store`,
  `content_store`, Athanor, BLAKE3), so this is a reuse-and-reference change, not a
  new subsystem. No code written yet.
