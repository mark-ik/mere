# Inker PaintList adoption plan

**Status (2026-05-22): complete.** document-canvas now produces a
`paint_list_api::PaintList` and lowers it through the shared
`paint_list_render` translator, replacing its bespoke
`DocumentRenderPacket → netrender::Scene` walk. This is phase **P4** of the
cross-repo PaintList-layer extraction; the master plan and the P1–P3
receipts (the genet side + the new neutral crates) live at
[`genet/docs/2026-05-20_paintlist_extraction_plan.md`](../../../../genet/docs/2026-05-20_paintlist_extraction_plan.md).

## Why

Before P4, document-canvas reached the renderer through its own
hand-rolled scene walk ([`netrender_backend::scene_from_packet`]),
bypassing `paint_list_api` entirely. That made it a *second door* to the
renderer alongside genet — proving the polyglot PaintList story was
not actually exercised by a second engine. P4 routes inker through the
same shared vocabulary + translator, making it the second genuine
PaintList producer.

## Findings

- The bespoke walk was small and self-contained: `Text → glyph runs`,
  `Image/Rule → primitives`, `Group → recurse`. It mapped cleanly onto
  `PaintCmd` (`DrawText`, `DrawRect`, `DrawLine`).
- **Font-model fork (the one real design decision).** Inker resolved
  `(family, weight, style)` to a *pre-registered* `netrender::FontId`
  (an in-process assumption: the host registers faces out-of-band, the
  resolver hands back the id). The PaintList contract instead carries
  font **bytes** in the list's `fonts()` side-table, and the renderer
  owns registration (self-contained for IPC / capture-replay).
  **Decision: adopt the canonical bytes side-table.** Inker conforms to
  the PaintList contract; the shared `paint_list_render` translator stays
  unchanged (single font model). Rejected: an "external pre-registered
  font map" entry point on the translator (would have split the shared
  crate's font model to preserve inker's in-process optimization).
- No production consumer existed yet — `scene_from_packet` / `FontResolver`
  were referenced only inside document-canvas (its own tests). So the
  `FontResolver` trait could be reshaped freely.
- `netrender::Scene::push_font` does not validate bytes (it stores the
  blob and returns an id), so unit tests can use a dummy non-empty face
  and still assert a `GlyphRun` op is emitted; real-font fidelity is a
  rasterization concern, validated genet-side.

## What changed

| Area | Change |
| --- | --- |
| `Cargo.toml` | `paint_list_api` added as an **unconditional** dep (portable: euclid + serde, no wgpu). `paint_list_render` added under the existing `netrender` feature (`netrender = ["dep:netrender", "dep:paint_list_render"]`). |
| `font.rs` | `FontResolver::resolve_font_id(...) -> Option<u32>` **replaced** by `resolve_font_data(...) -> Option<FontFaceData>` (face bytes + collection index). New `FontFaceData` struct. `register_with_parley` (layout side) unchanged. |
| `paint_list.rs` (**new**) | `InkerPaintList: impl PaintList` + `paint_list_from_packet(packet, resolver, colors)`. Portable producer — depends only on `paint_list_api`. Interns each resolved face once into the `fonts()` side-table, references by `FontInstanceKey`; unresolved runs fall back to a placeholder `DrawRect`. Rule → `DrawLine` (1px strip at mid-line). |
| `netrender_backend.rs` | Reduced to a lowering shim: build the `InkerPaintList`, call `paint_list_render::translate_paint_list(&list)`. `scene_from_packet` signature preserved. Bespoke walk deleted. |
| `lib.rs` | Exports `InkerPaintList`, `paint_list_from_packet`, `FontFaceData`. `paint_list` module unconditional; `netrender_backend` still feature-gated. |
| `paint_list_api` (netrender ws) | `EngineId::INKER = Self(3)` added (+ sentinel test). |

## Font identity caveat (the v2 follow-up — scoped below)

The producer still *re-resolves* `(family, weight, style)` at packet-emit
time rather than threading parley's actual chosen face through. If parley
fell back to a different concrete face than the resolver advertises, the
glyph **IDs** in the run were shaped against parley's face while the
side-table ships the resolver's face → the renderer draws the wrong
glyphs. The fix is to source the face from parley (which already knows
it); scoped below.

---

## v2 scope: thread parley's actual font identity

**Status: implemented 2026-05-23** (option (a) — sidecar; packet stays
serializable). Fixes the caveat above. See Progress for the receipt and
the one deviation from the phasing below.

### Why the bug is real

`paint_list::Builder::intern_font` calls `resolver.resolve_font_data(
(family, weight, style))`. That tuple is a *label*, recovered in
[`text.rs`](../../../crates/inker/document-canvas/src/text.rs) from the
parley brush (`family_label_from_brush`), not parley's chosen face. On
fallback (missing glyph, unavailable family) parley shapes against a
different face; the glyph ids are indices into *that* face. Shipping the
resolver's advertised face means the renderer indexes the wrong outlines.

### Key API finding (parley 0.9 — confirmed in registry source)

parley already exposes the real per-run font, so no re-resolution is
needed:

- `parley::layout::Run::font() -> &FontData`
  (`linebender_resource_handle::FontData`) with **`data: Blob<u8>`** +
  **`index: u32`** — the actual face bytes + collection index the run was
  shaped against.
- `Blob::id() -> u64` — stable per-allocation id; use as the dedup key
  (no byte hashing).
- `Run::synthesis() -> Synthesis` — `embolden()` (faux bold), `skew()`
  (faux italic), `variation_settings()`.
- `Run::normalized_coords() -> &[i16]` — variable-font axis coords the
  run was shaped at.

### Reference implementation — genet already does Tier 1

This is not an open design question. Genet, the more mature sibling
producer, already does exactly what Tier 1 proposes:
[`genet-layout::paint_emit`](../../../../genet/components/genet-layout/paint_emit.rs)
keys its `FontCollector` on `font.data.id()` (blob id) and interns
`parley_run.font()` — parley's *actual* shaped face, not a re-resolved
label — shipping `font.data.data().to_vec()` + `font.index`. **Inker is
the only one of the four producers doing label re-resolution.** That
collapses Tier 1 from a design exercise to a mirror-task: copy genet's
`FontCollector` shape and its `emit_with_layouts_populates_font_table`
test pattern. Lead the graduated plan with this.

### Fidelity tiers

| Tier | Carries | Fixes | Cost |
| --- | --- | --- | --- |
| **0** (current/P4) | label re-resolve | — | — |
| **1** (v2 target) | parley's real `FontData` (blob + index) per run | wrong-face-on-fallback (the actual bug) | `text.rs` + `GlyphRun` + `paint_list.rs`; **no netrender change** |
| **2a** (var fonts) | + normalized var-coords per run | variable fonts matching exactly | `paint_list_api` (`TextRunItem`/`TextOptions`) + producers + translator. **No netrender change** — `SceneGlyphRun.font_axis_values` + `push_glyph_run_variable` already model it (Roadmap C4, shipped); the translator just drops it (calls `push_glyph_run_full`) |
| **2b** (synthesis) | + `synthesis` (embolden/skew) | synthetic bold/italic exactly | genuine renderer gap — unmodeled at all three layers. Faux-bold needs vello outline work; faux-italic could ride a skew transform. Own netrender-side plan |

Recommend **Tier 1** as the v2 deliverable — it eliminates the
correctness bug with bounded, mere-side changes. Of Tier 2, **2a
(var-coords) is upstream-only** — `paint_list_api` + producers +
translator, with no netrender change since the renderer already models
axis values — and could ride close behind Tier 1. **2b (synthesis) is the
genuine renderer capability** and gets its own netrender-side plan (lands
in `paint_list_render` + `netrender`, possibly vello).

### Design tension — where the bytes live

`GlyphRun` is part of the `Serialize + Deserialize + PartialEq`
`DocumentRenderPacket`; `FontData` (an Arc-backed `Blob`) isn't trivially
those. Options:

- **(a) `GlyphRun` gains `font_face: FontFaceId` (u32); the `FontData`
  table rides as a sidecar return value — *not* on the serialized
  packet.** Recommended — the **`PaintList`, not the packet, is the
  IPC-self-contained form** (it already carries `FontResource` bytes).
  Concretely: `layout_document` returns `LaidOutDocument { packet, fonts }`
  (or `paint_list_from_packet` takes the font table as a second arg); the
  `Arc`-cheap `FontData` handles stay in-process and owned bytes
  materialize only at the `paint_list_api` boundary. Do **not** hang the
  table on the packet behind `#[serde(skip)]` — that makes the packet
  silently lossy on round-trip (deserializes to placeholder-only text) and
  breaks its `PartialEq`.
- (b) Packet carries owned `Vec<u8>` per face (serializable packet,
  heavy). Only if document packets themselves must serialize/cache.
- (c) Keep the resolver but key it on `Blob::id()` (actual identity) not
  the label. Lighter, but keeps an indirection parley makes unnecessary.

Lean (a).

### Architectural note — does the serializable intermediate earn its keep?

Genet has none of this tension because
[`genet-layout::paint_emit`](../../../../genet/components/genet-layout/paint_emit.rs)
emits `PaintCmd`s straight from the live parley `Layout` at walk time — it
never drops to a serializable intermediate, so glyph IDs and face bytes
can't diverge by construction. Inker's tension is *entirely* a product of
the `DocumentRenderPacket` / `GlyphRun` layer sitting between layout and
paint-list production.

That layer is currently **single-consumer**: the only renderers of the
packet are `paint_list_from_packet` and platen's passthrough (which feeds
the same netrender path). The "feeds multiple backends (gpui-native,
AccessKit)" rationale is aspirational — nothing pulls on the packet's
`Serialize` derive yet. Two honest paths:

- **Keep the intermediate, add the sidecar** (option (a)) — smaller,
  reversible, ships Tier 1 now. Recommended near-term.
- **Collapse toward genet's shape** — keep the parley `Layout` alive and
  emit the `PaintList` directly, retiring the serializable packet. Makes
  the bug *and* the tension vanish together; the right move if the
  packet's `Serialize` derive stays unpulled. Decide this consciously
  rather than treating the serializable packet as a fixed constraint.

### Consequence for P4's `resolve_font_data`

Under Tier 1 the bytes come from parley, so `FontResolver`'s render-side
method (`resolve_font_data`, added in P4) **disappears** — the trait keeps
only `register_with_parley` (layout-time). P4's `resolve_font_data` was
the interim stopgap; v2 removes it (or repurposes it for hosts injecting
faces parley can't discover).

**The bigger payoff.** Tier 1 isn't just a rare-fallback fix. Today
`NoFontResolver` makes *every* run a placeholder rect — no real text until
a host wires fonts. Once `run.font()` supplies the bytes, parley's
bundled/system defaults ride in the side-table, so **documents render real
text with zero host font wiring** and the placeholder-rect path retires
for the common case. Caveat: this means copying system-font bytes into
every `PaintList` — exactly when the large-blob question (below) bites.
Resolve the two together.

### Phasing (Tier 1)

1. `text.rs` — capture `run.font()` (`FontData`, an `Arc`-cheap clone) per
   `GlyphRun`; record a `FontFaceId` on each run; merge per-block faces
   into a doc-level `Blob::id()`-keyed table.
2. `types.rs` / `layout.rs` — `GlyphRun` gains `font_face: FontFaceId`;
   keep `font_family`/`weight`/`style` for a11y/debug. `layout_document`
   returns the font table as a sidecar (`LaidOutDocument { packet, fonts }`)
   per option (a) — the packet stays honestly serializable.
3. `paint_list.rs` — drop resolver re-resolution; map `FontFaceId →
   FontInstanceKey`; populate `InkerPaintList.fonts` from the sidecar table
   (dedup by blob id). Mirror genet's `FontCollector`.
4. `font.rs` — remove `resolve_font_data`'s render role; keep
   `register_with_parley`.
5. Tests — a fallback case (resolver advertises family A; parley falls
   back to B) asserts the shipped face matches parley's, not A.

### Open questions

- **Large system-font blobs** (e.g. CJK TTC) in the `PaintList`
  side-table — acceptable under the IPC-containment contract, or warrant a
  lazy/streamed/content-addressed font channel? The "netrender already
  dedups by blob id across resends" softener is **not true on this path as
  wired**:
  [`paint_list_render::register_fonts`](../../../../netrender/paint_list_render/src/lib.rs)
  does `peniko::Blob::new(Arc::new(fr.data.clone()))` — minting a fresh
  blob id from cloned bytes every translate call, which netrender's own
  `FontBlob` doc flags as defeating vello's atlas dedup. The
  [`FontRegistry`](../../../../netrender/netrender/src/registry.rs) built
  for exactly this is never threaded into the translator.
  - **Consumer-reality check (2026-05-23, corrected 2026-05-24).** The
    render path is *not* absent — genet's C4 landed 2026-05-09:
    [`Paint::render`](../../../../genet/components/paint/netrender_painter.rs)
    drives `renderer.render_with_compositor(&state.scene, …)` on a
    persistent `netrender::Renderer` (one per painter id, built in
    `register_rendering_context`). It renders the *stored* `Scene`, whose
    `FontBlob`s were minted once at `SendPaintList`/translate time; so
    re-rendering the same scene reuses them (vello dedups by `Blob::id()`),
    and the fresh-Blob waste is **per re-translate** (a new `SendPaintList`
    — content change / resize / relayout), not per frame. (An earlier
    revision of this note wrongly called the render loop unbuilt, anchoring
    on the stale `GenerateFrame` "C4 territory" comment in the message-loop
    arm, which is a different thing from `Paint::render`.)
  - **Where the lever belongs.** A persistent `FontRegistry` + `FontBlob`
    cache on the painter (it already holds persistent `Renderer`s per
    painter id), threaded into the `SendPaintList` translate step (today
    the stateless `translate_envelope_with_external_textures`), keyed on a
    stable `FontResource.blob_id` (new wire field). Prerequisite still
    missing: that stable id — `FontResource` carries none, and inker
    recreates its `FontContext` per `layout_document`, so even producer
    `Blob::id()`s aren't stable across calls. Low priority: waste is
    per-content-change (not per frame) and current faces are small system
    fonts. The lazy/streamed font channel (wire cost) rides the same id.
- **Tier 2a sequencing (var-coords)** — upstream PaintList work, not a
  netrender extension. Add run-level axis payload to `TextRunItem` /
  `TextOptions`, thread it from parley through producers, and have the
  translator build `SceneGlyphRun` / call `push_glyph_run_variable`.
- **Tier 2b sequencing (synthesis)** — renderer-capability work. Add a
  run-level synthesis payload to the PaintList API, then plan the
  netrender/vello implementation separately (especially faux-bold).
- **Cross-check genet-layout** — *resolved:* genet already threads
  `run.font()` and keys on `Blob::id()` (see "Reference implementation —
  genet already does Tier 1" above). Mirror it; this scope does not apply
  to genet.

## Progress

- 2026-05-22: P4 implemented and validated.
  - `cargo test -p document-canvas --features netrender` → **23 passed,
    0 failed** (11 layout, 8 portable producer tests in `paint_list.rs`,
    4 scene-level integration tests through the shared translator).
  - `cargo build -p document-canvas` (no `netrender` feature) green —
    the producer path stays wgpu-free / wasm-light.
  - `paint_list_api` `engine_id_sentinels_are_stable` green with `INKER`.
- 2026-05-23: v2 scope sharpened against the codebase (parley 0.9,
  netrender, `paint_list_api`, genet-layout). Revisions: genet already
  implements Tier 1 (mirror-task, not a design exercise); option (a) is a
  sidecar return, not a `#[serde(skip)]` packet field; Tier 2 split into 2a
  (var-coords, upstream-only — netrender already models it) and 2b
  (synthesis, the only genuine renderer gap); corrected the "netrender
  already dedups" softener (the translator re-blobs every frame, defeating
  vello's atlas dedup); noted Tier 1 also makes text render with no host
  font wiring.
- 2026-05-23: **Tier 1 implemented** (option (a) — sidecar; packet stays
  serializable). New `font_table` module: `FontInterner` dedups parley
  faces by `Blob::id()` during layout, sealed into a `FontTable` sidecar.
  `layout_document` now returns `LaidOutDocument { packet, fonts }`; each
  `GlyphRun` carries a `FontFaceId` interned from `run.font()` (parley's
  actually-shaped face), with `font_family`/`weight`/`style` kept for
  a11y/debug only. `paint_list_from_packet` / `scene_from_packet` drop the
  resolver, take `&FontTable`, and ship each face's bytes from the sidecar.
  `font.rs` slimmed to the layout-time `register_with_parley` seam
  (`resolve_font_data` / `FontFaceData` / `FontRequest` removed). platen's
  `build_document_scene` returns `LaidOutDocument`.
  - **Deviation from phasing step 5.** The "resolver advertises A, parley
    falls back to B" test is obviated: removing the render-side resolver
    structurally deletes the second font-identity source, so there is no
    "A" to advertise. Replaced by deterministic invariants —
    `each_run_ships_the_face_it_was_shaped_against` (per-run: shipped bytes
    == `sidecar[font_face]` bytes), `shipped_faces_come_from_the_sidecar`
    (set membership), and `document_dedups_shared_face`.
  - **Payoff realized.** Bytes now come from parley, so `NoFontResolver`
    renders real text — the placeholder-rect path is dead for the common
    case (kept only as a defensive branch if a face is absent from the
    sidecar).
  - Tests (Windows): `cargo test -p document-canvas` → **21 passed**;
    `--features netrender` → **25 passed**; `cargo test -p platen` →
    **31 passed**. 0 failed.
  - Implemented **in place** rather than graduating to a fresh dated plan:
    the change is one self-contained pass fully covered by the scope above
    (DOC_POLICY §1 — control doc growth).

This completes the extraction's payoff: genet and inker are now two
producers of the same `paint_list_api` vocabulary, both lowering through
the one shared `paint_list_render` translator. Tier 1 (thread parley's
real `FontData`) landed 2026-05-23 — see Progress. **Tier 2a** (var-coords,
upstream-only) and **Tier 2b** (synthesis, renderer-side) remain scoped
above for their own plans.
