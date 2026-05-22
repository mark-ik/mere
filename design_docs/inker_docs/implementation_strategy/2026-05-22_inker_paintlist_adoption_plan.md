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

## Font identity caveat (unchanged from v1)

The producer still *re-resolves* `(family, weight, style)` at packet-emit
time rather than threading parley's actual chosen face through the
packet. If parley fell back to a different concrete face than the
resolver advertises, rendered glyphs could mismatch the laid-out ones.
Plumbing parley's real font identity into `GlyphRun` is the v2 follow-up
and is orthogonal to this extraction.

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
the one shared `paint_list_render` translator. Eligible for
`archive_docs/` once the v2 font-identity follow-up is scoped.
