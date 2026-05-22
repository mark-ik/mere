# Inker PaintList adoption plan

**Status (2026-05-22): complete.** document-canvas now produces a
`paint_list_api::PaintList` and lowers it through the shared
`paint_list_render` translator, replacing its bespoke
`DocumentRenderPacket → netrender::Scene` walk. This is phase **P4** of the
cross-repo PaintList-layer extraction; the master plan and the P1–P3
receipts (the serval side + the new neutral crates) live at
[`serval/docs/2026-05-20_paintlist_extraction_plan.md`](../../../../serval/docs/2026-05-20_paintlist_extraction_plan.md).

## Why

Before P4, document-canvas reached the renderer through its own
hand-rolled scene walk ([`netrender_backend::scene_from_packet`]),
bypassing `paint_list_api` entirely. That made it a *second door* to the
renderer alongside serval — proving the polyglot PaintList story was
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
  rasterization concern, validated serval-side.

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

**Status: scoped, not started.** Fixes the caveat above.

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

### Fidelity tiers

| Tier | Carries | Fixes | Cost |
| --- | --- | --- | --- |
| **0** (current/P4) | label re-resolve | — | — |
| **1** (v2 target) | parley's real `FontData` (blob + index) per run | wrong-face-on-fallback (the actual bug) | `text.rs` + `GlyphRun` + `paint_list.rs`; **no netrender change** |
| **2** (further) | + `synthesis` (embolden/skew) + normalized var-coords | synthetic bold/italic + variable fonts matching exactly | also extends netrender's glyph API (`Glyph{id,x,y}` + `push_glyph_run` model neither today) — reaches into the renderer |

Recommend **Tier 1** as the v2 deliverable — it eliminates the
correctness bug with bounded, mere-side changes. Tier 2 is a renderer
capability and gets its own netrender-side plan (lands in
`paint_list_render` + `netrender`).

### Design tension — where the bytes live

`GlyphRun` is part of the `Serialize + Deserialize + PartialEq`
`DocumentRenderPacket`; `FontData` (an Arc-backed `Blob`) isn't trivially
those. Options:

- **(a) `GlyphRun` gains `font_face: FontFaceId` (u32); a companion font
  table holds the `FontData`, out-of-band from the serialized packet.**
  Recommended — the **`PaintList`, not the packet, is the IPC-self-contained
  form** (it already carries `FontResource` bytes). So the packet/`LaidOutText`
  carries Arc-cheap font handles in-process; owned bytes materialize only
  at the `paint_list_api` boundary.
- (b) Packet carries owned `Vec<u8>` per face (serializable packet,
  heavy). Only if document packets themselves must serialize/cache.
- (c) Keep the resolver but key it on `Blob::id()` (actual identity) not
  the label. Lighter, but keeps an indirection parley makes unnecessary.

Lean (a).

### Consequence for P4's `resolve_font_data`

Under Tier 1 the bytes come from parley, so `FontResolver`'s render-side
method (`resolve_font_data`, added in P4) **disappears** — the trait keeps
only `register_with_parley` (layout-time). P4's `resolve_font_data` was
the interim stopgap; v2 removes it (or repurposes it for hosts injecting
faces parley can't discover).

### Phasing (Tier 1)

1. `text.rs` — capture `run.font()` (`FontData`) per `GlyphRun`; build a
   `Blob::id()`-keyed font table on `LaidOutText`; record a `FontFaceId`
   on each run.
2. `types.rs` — `GlyphRun` gains `font_face: FontFaceId`; keep
   `font_family`/`weight`/`style` for a11y/debug. Font table rides
   out-of-band per option (a).
3. `paint_list.rs` — drop resolver re-resolution; map `FontFaceId →
   FontInstanceKey`; populate `InkerPaintList.fonts` from the font table
   (dedup by blob id).
4. `font.rs` — remove `resolve_font_data`'s render role; keep
   `register_with_parley`.
5. Tests — a fallback case (resolver advertises family A; parley falls
   back to B) asserts the shipped face matches parley's, not A.

### Open questions

- **Large system-font blobs** (e.g. CJK TTC) in the `PaintList`
  side-table — acceptable under the IPC-containment contract, or warrant
  a lazy/streamed font channel? netrender already dedups by blob id across
  resends, which softens repeated sends.
- **Tier 2 sequencing** — netrender glyph-API extension (synthesis +
  var-coords) is a renderer capability; own plan, netrender-side.
- **Cross-check serval-layout** — does serval's `paint_emit` have the same
  latent label-re-resolution issue, or does it already carry real shaped
  bytes? If serval does it right, mirror its approach; if not, this scope
  applies there too. Worth a look when the work starts.

## Progress

- 2026-05-22: P4 implemented and validated.
  - `cargo test -p document-canvas --features netrender` → **23 passed,
    0 failed** (11 layout, 8 portable producer tests in `paint_list.rs`,
    4 scene-level integration tests through the shared translator).
  - `cargo build -p document-canvas` (no `netrender` feature) green —
    the producer path stays wgpu-free / wasm-light.
  - `paint_list_api` `engine_id_sentinels_are_stable` green with `INKER`.

This completes the extraction's payoff: serval and inker are now two
producers of the same `paint_list_api` vocabulary, both lowering through
the one shared `paint_list_render` translator. The v2 font-identity
follow-up is scoped above (Tier 1: thread parley's real `FontData`); this
doc graduates to a fresh dated plan when that work begins.
