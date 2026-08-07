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

```rust
Node { … thumbnail_png: Option<Vec<u8>>, thumbnail_width/height: u32,
          favicon_rgba:  Option<Vec<u8>>, favicon_width/height:  u32, … }
```

After — the node holds a small **role-keyed set** of references, no pixels:

```rust
Node { … images: BTreeMap<ImageRole, ImageRef>, … }
ImageRef  { hash: Hash, width: u32, height: u32 }   // ~40 B, no pixels
ImageRole { Favicon, Preview, Snapshot(lane) }      // extensible; deterministic order

// pixels live once, content-addressed, in the eidetic blob store:
//   content/image/<blake3-hex>  ->  PNG bytes
```

**A role-keyed map, not two fixed slots.** Preview imagery is three roles, not two:
the **favicon** (site icon on the node face), the **preview** (the small default
thumbnail), and the **snapshot** (the last-rendered peek the preview card shows) —
and snapshots want to key by render lane (last rasterized band for genet lanes,
last captured frame for scry tiles), so a node can hold more than one. Today
`thumbnail_png` conflates preview and snapshot into one slot: depositing a snapshot
*overwrites* the thumbnail, so a node cannot hold both a tiny orrery-face sprite and
a larger last-render peek. The content-addressed store already holds N images per
node trivially, so only the node's *reference shape* has to generalize — from two
`Option` fields to a small `BTreeMap<ImageRole, ImageRef>` (deterministic order for
stable snapshots). New roles (per-lane snapshots) are added by extending the key,
touching no storage.

The kernel holds references (a few MB across 50k nodes). The pixels live once each
in the durable store (dedup collapses shared favicons *and* repeated snapshots of an
unchanged page). The render layer decodes only what it is about to show, into a
bounded cache.

**Unify on PNG-in-blob.** The favicon is stored raw RGBA today but the render path
already re-encodes it to a PNG data URI (`textures.rs` `favicon_data_uri` →
`png_bytes_from_rgba`). Storing every role as PNG in the blob lets the render path
treat them identically (`png_data_uri` over the loaded bytes, no re-encode), shrinks
on-disk size, and keeps `ImageRef` format-free (the blob is always PNG; the
reference just carries the decoded dimensions).

## Snapshots and preview cards ride the same store

This is not a bolt-on: the preview / snapshot card **already** stores its pixels in
`Node::thumbnail_png`, so externalizing that field externalizes snapshots for free,
and the connection tightens the plan in three ways (verified in `render/cards.rs`,
card design §4):

- **The deposit path is already a thumbnail write.** A snapshot deposit is
  `GraphDelta::SetNodeThumbnail { png_bytes, width, height }` (compat-view tiles
  capture a WebView frame on teardown and persist it as the node thumbnail); the
  card *prefers* `node.thumbnail_png` as its source ("the deposited last-seen
  pixels", summoning design §5), re-rendering only on absence. Under this plan a
  deposit becomes `save_image → ImageRef` under the `Snapshot`/`Preview` role.
- **The bounded decoded cache already exists.** `window_view::snapshot_data_uris`
  (member-keyed, URL-guarded so in-place nav invalidates, capped at 256 with a crude
  clear-all-on-full) *is* the Phase 4 cache. The resolver generalizes it: keep the
  member/url guard, replace clear-all-on-full with a real byte-bounded LRU, and key
  by `ImageRef` hash so identical snapshots share a decode.
- **Content-addressing makes re-deposit idempotent.** Today every `SetNodeThumbnail`
  rewrites the bytes and dirties the graph even when the page is unchanged. Under
  content-addressing an unchanged page re-deposits to the same BLAKE3 hash → same
  blob → and if the `ImageRef` is unchanged, no graph-dirty churn. It also lets the
  crude `BLANK_THUMBNAIL_MAX` blank-guard (treat a suspiciously-small PNG as absent)
  stay a render-time heuristic rather than something baked into storage.

Retention, though, differs by role — see Phase 5. A favicon is trivially
re-fetchable; a scry-tier snapshot may be the *only* offline representation of
content that cannot be cheaply re-rendered, so the two are stored the same way but
forgotten under different rules.

## Phases

Each phase states its done-condition (a checkable property), not a duration.

### Phase 1 — Content-addressed image blob store + `ImageRef`

- New `session-runtime/src/image_store.rs`, mirroring `content_store.rs`:
  `save_image(store, png_bytes) -> Hash` (content-addressed via `eidetic::Hash::of`,
  written under `content/image/<hash>`), `load_image(store, hash) -> Option<Vec<u8>>`,
  `delete_image(store, hash) -> bool`. Saving the same bytes twice is one blob.
- `ImageRef { hash, width, height }` as the small handle, plus `ImageRole`
  (`Favicon`, `Preview`, `Snapshot(lane)` — a flat enum now, the snapshot lane a
  minimal `Default` to start, extensible later). Home: the kernel (`kernel::types`)
  since `Node` holds them and `PersistedNode` persists them, with the hash stored as
  bytes/hex so the kernel takes no eidetic dependency it should not (mirror the
  existing `UuidAsBytes` treatment; a plain 32-byte array + hex is enough — the
  kernel never hashes, it only carries the handle).
- **Done when:** a round-trip unit test over an in-memory `Store` (the `MemStore`
  pattern already in `content_store` / `athanor` tests) shows save→load returns the
  bytes, two saves of identical bytes produce one blob at one key, and distinct
  bytes produce distinct keys.

### Phase 2 — Kernel `Node` swap + persistence + migration

- Replace the six inline fields (`thumbnail_png`, `thumbnail_width`,
  `thumbnail_height`, `favicon_rgba`, `favicon_width`, `favicon_height`) with the
  `images: BTreeMap<ImageRole, ImageRef>` map on `Node` and `PersistedNode`; update
  `snapshot/to.rs`, `snapshot/from.rs`, and the accessor sites listed in Findings.
  Accessor ergonomics: small helpers (`node.favicon()`, `node.image(role)`) so call
  sites read a role rather than index the map directly.
- **Migration.** Existing `graph.json` snapshots carry inline PNG/RGBA. On load,
  detect the legacy fields (keep them as `#[serde(default)]` deserialize-only shadow
  fields), write their bytes to the image store, and populate the map: `favicon_rgba`
  → `Favicon`, `thumbnail_png` → `Preview` (the legacy slot was the deposited
  preview/snapshot; the preview/snapshot split begins post-migration). One-time,
  lossless, runs through the same host that already holds the store at load. A
  snapshot with no images migrates trivially.
- **Done when:** a graph with a thumbnail and a favicon survives
  snapshot→load→re-render with the images intact; a legacy snapshot (inline bytes)
  loads, externalizes, and re-saves with references only and no pixels in the JSON;
  the snapshot-size gate (`kernel graph::tests::snapshot_size`) shows the per-node
  snapshot no longer carries image bytes.

### Phase 3 — Write sites store to the blob, keep a reference

- The capture / import / favicon paths that set image bytes today
  (`browse_capture.rs`, `import/web_clip.rs`, favicon fetch, `node_ops.rs`,
  `graph_engram.rs` — see Findings) call `save_image` and set the returned
  `ImageRef` under its role instead of moving `Vec<u8>` inline. Favicon capture
  encodes RGBA→PNG once at store time (reusing `png_bytes_from_rgba`).
- **Snapshot deposit is one of these write sites.** `GraphDelta::SetNodeThumbnail`
  (the compat-view teardown-capture path, card design §4) becomes a `save_image`
  under the `Snapshot`/`Preview` role that sets an `ImageRef` — so the deposit no
  longer carries `Vec<u8>` through the delta, and an unchanged re-deposit is a
  content-addressed no-op instead of a graph-dirtying byte rewrite.
- **Done when:** capturing a favicon, a preview, or a snapshot writes one blob and
  leaves only a reference on the node; no write path (deposit included) constructs
  an inline image `Vec<u8>` on a `Node` anymore (enforced by the field no longer
  existing).

### Phase 4 — Render resolver + bounded decoded-image cache

- The card builder (`render/cards.rs`, `render/paint.rs`) resolves an `ImageRef`
  to a data URI through a resolver that checks a bounded in-memory cache keyed by
  hash; on a miss, `block_on(load_image(store, hash))` (the ready-fjall-future
  pattern `load_cached` already uses), wraps via `png_data_uri`, and inserts.
- **This generalizes the cache that already exists.** `window_view::snapshot_data_uris`
  (member-keyed, URL-guarded, capped at 256 with clear-all-on-full) is today's
  version for the snapshot card. The resolver *is* this cache, upgraded: keyed by
  `ImageRef` hash (so identical images across nodes share one decode), evicted LRU
  under a byte bound instead of cleared wholesale, and serving every role rather than
  just snapshots. Keep its member/url guard so in-place navigation still invalidates.
- This is the one perf-sensitive seam: the bound replaces "every node's pixels,
  forever" with "the visible working set." It composes with the existing per-surface
  netrender tile caches (render perf plan) rather than replacing them.
- **Done when:** visible cards render their favicon / preview / snapshot identically
  to today; the meerkat render-perf harness shows no regression on the steady-state
  card-raster path; the cache respects its byte bound (an eviction test).

### Phase 5 — Orphan GC as an Athanor pass

- **Orphan sweep (structural).** Image blobs are shared (content-addressed), so
  eviction is **mark-sweep against live references**, not per-URL drop: a blob is
  droppable only when no live node's `ImageRef` names its hash. Add `propose_image_gc`
  (pure: diff the store's `content/image/*` keys against the set of hashes referenced
  across the snapshot's nodes) and `apply_image_gc` (drop the unreferenced blobs)
  alongside Athanor's existing forgetting pass, same R0 propose/apply split.
- **Retention differs by role (policy).** Orphaning happens when a node drops a
  reference — and *which* references a forgotten node drops is role-dependent, the
  point the snapshot convergence surfaced. When Athanor forgets a short-term stale
  node's cached content today, it can also drop that node's **favicon** reference
  (trivially re-fetchable) but should keep a **snapshot** of content that cannot be
  cheaply re-rendered (a scry-tier last-frame is the only offline copy — "don't drop
  what you can't re-derive"). Promoted (long-term) nodes keep all their imagery.
  Encode this as a per-role rule on the existing `memory_levels` axis, not a new
  policy surface: favicon = disposable cache, snapshot = node-precious. The favicon
  then re-materializes on next visit; the snapshot would be gone for good.
- **Done when:** a blob referenced by any node is never orphan-proposed; a blob no
  node references is proposed and dropped; a forgotten short-term node drops its
  favicon reference but retains an un-refetchable snapshot; a promoted node retains
  all imagery; re-running the pass is a safe no-op.

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
- **Image roles** — the `ImageRole` set (favicon / preview / snapshot) is extensible
  without a storage change: per-lane snapshots (genet band vs scry frame) are new
  keys, not new fields. Which roles a given node populates, and at what resolution a
  snapshot is captured, are capture-side policies this plan leaves open.

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
  URIs embedded as `<img>` in the orrery card that genet renders
  (`favicon_data_uri`, `png_data_uri`). The resolver slots in right here.
- **Snapshots already ride `thumbnail_png`.** The preview / snapshot card prefers
  `node.thumbnail_png` (the deposited last-seen pixels) and re-renders only on
  absence (`render/cards.rs`); the deposit path is `GraphDelta::SetNodeThumbnail`
  (compat-view teardown-capture, card design §4). `window_view::snapshot_data_uris`
  (member-keyed, url-guarded, cap 256, clear-all-on-full) is the window-local decoded
  cache derived from that thumbnail — i.e. the bounded cache this plan formalizes.
  So externalizing `thumbnail_png` externalizes snapshots for free; the role-keyed
  map is what un-conflates preview from snapshot going forward.
- **Field / accessor sites to change (23 files touch the image fields).** Kernel:
  `graph/node.rs`, `graph/node_props.rs`, `graph/mod.rs`, `persistence.rs`,
  `graph/snapshot/{to,from}.rs`, `graph/cross_graph.rs`, `graph/capture.rs`, plus
  the snapshot tests. Meerkat: `node_ops.rs`, `render/{cards,paint,orrery_scene,textures}.rs`,
  `window_view/gnode_pool.rs`, `graph_delta_log.rs`, `frame_ops/config.rs`,
  `browse_capture.rs`. Session-runtime: `graph_engram.rs`, `memory_levels.rs`
  (test node builder). Import: `web_clip.rs`. Orrery: `frame.rs`. This is the
  change surface Phase 2/3 walk.

## Progress

- **2026-07-26 — Phases 2 and 3 landed; phase 4 has its seam, not its cache.**
  Executed as lane D0 of the
  [node dissolution plan](../../archive_docs/2026-08-06_completed_plans/2026-07-18_node_dissolution_facets_plan.md), after
  its D-gate measured inline imagery at 18x the snapshot size and 3.8x its
  load time.

  **Phase 2.** `Node` and `PersistedNode` lost the six inline fields for
  `images: BTreeMap<ImageRole, ImageRef>`, with role accessors
  (`image`/`favicon`/`preview`/`set_image`/`clear_image`) so call sites name a
  role. Migration is a **host-side pre-pass**, not a kernel one:
  `image_store::migrate_legacy_images` externalizes a legacy snapshot's bytes
  before `Graph::from(snapshot)`, because hashing and blob storage are async
  and the store's business. The legacy fields survive on `PersistedNode` as
  `#[serde(rename, skip_serializing)]` shadows, so old snapshots still load
  and a re-saved one emits references only. `GraphSnapshot::legacy_image_count`
  exists so a host can assert the pass ran — a skipped migration would
  otherwise drop pixels silently.

  **Phase 3.** The delta spine carries handles, not bytes:
  `SetNodeThumbnail`/`SetNodeFavicon` collapsed into one
  `SetNodeImage { key, role, image }`, likewise the captured/replay pair. An
  unchanged re-deposit now compares equal on the reference and reports "no
  change", which is the content-addressed no-op the plan predicted.

  **Journal compatibility, decided.** `CapturedDelta` is persisted and the
  graph is its replay, so the legacy variants are **kept readable** and
  `replay_delta` returns `None` for them. An old journal still replays; only
  the regenerable pixels are dropped. That follows this plan's own premise —
  preview imagery is experience, not truth — and losing the ability to read an
  existing journal would have been the far worse failure. Tested both ways.

  **Phase 4 is seamed, not finished.** `Canvas` gained
  `resolved_images: HashMap<[u8; 32], (Vec<u8>, u32, u32)>` plus
  `register_resolved_image`, mirroring the existing `scene_sprite_textures`
  pattern: the host resolves a reference through the store and registers
  decoded pixels; an unresolved reference simply does not paint. **The map is
  unbounded** — the plan's bounded LRU is policy on this same seam and remains
  open.

  **Receipts.** kernel 278, session-runtime 219, canvas 145, import 9, all
  green. The migration test asserts a legacy snapshot externalizes, that a
  re-saved snapshot's JSON contains neither `thumbnail_png` nor
  `favicon_rgba`, and that a second pass writes nothing.

  **One trap worth remembering.** `GraphFixtures::set_node_favicon` forwarded
  to `Graph::set_node_favicon`; deleting the inherent method silently turned
  that forward into self-recursion. It compiled, then blew the stack in a
  canvas test. Removing an inherent method that a same-named trait method
  forwards to is a compile-clean infinite loop.

  **Not done:** turnstone's ~28 read sites (it patches mere locally, so its
  build is broken until they are updated), the bounded cache, favicon
  capture's RGBA→PNG-at-store-time write site, and phase 5's orphan GC.

- **2026-07-06** — Plan authored from the Phase 4 footprint measurement plus a
  codebase investigation. Key outcome of the investigation: the entire blob /
  content-address / forgetting substrate already exists (eidetic `Store`,
  `content_store`, Athanor, BLAKE3), so this is a reuse-and-reference change, not a
  new subsystem. No code written yet.
- **2026-07-06 (refinement)** — Folded in the snapshot / preview-card convergence
  (Mark's prompt): snapshots already ride `Node::thumbnail_png`, and
  `snapshot_data_uris` is already the bounded decoded cache, so the store, resolver,
  and GC cover snapshots at no extra cost. Generalized the node's image references
  from two fixed `Option<ImageRef>` fields to a role-keyed `BTreeMap<ImageRole,
  ImageRef>` (favicon / preview / per-lane snapshot), un-conflating the preview and
  snapshot that share `thumbnail_png` today, and added per-role retention (favicon =
  disposable/re-fetchable, snapshot = node-precious for un-refetchable content) on
  the existing `memory_levels` axis.
