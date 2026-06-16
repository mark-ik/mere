# Retained-Text / Tiled-Render Foundation Plan

The audit's Lane 2 / §5 fix
([browser-bar audit](../research/2026-06-15_in_the_wings_and_browser_bar_audit.md)).
The single highest-leverage item on the board: one foundation that clears the
tall-page blocker and unlocks find-in-page, text selection/copy, true scroll,
and live HTML-page links.

## Problem (verified against the code, 2026-06-15)

Fetched document content is laid out once, lowered to one scene authored at the
**full content height**, rasterized into **one** offscreen GPU texture that is
**capped**, then scrolled by shifting a UV window over that texture. The body is
pre-truncated so the laid-out height stays under one texture. Concretely:

- `card.rs:144` `render_card_scene` lays out via `document_canvas::layout_document`,
  then `card.rs:159` rewrites `packet.viewport` to the full `content_height` so the
  lowering renders the whole document into a tall texture.
- `render.rs:760` caps the texture at `MAX_CARD_TEX_H = 8192` px and `render.rs:767`
  at `MAX_CARD_TEX_AREA = 30 MiB`; `tex_h` is `content_height.min(8192).min(area/w)`.
- `render.rs:846-871` scrolls by a UV window over that texture, so the reachable
  range is `tex_h - visible_h` (at most ~8192 px minus the viewport).
- `card.rs:296` `bound_document_body` truncates the body to `MAX_DOC_BYTES = 12 KiB`
  at a line boundary, sized to keep the laid-out height near one texture at the
  narrow card width (~1.7 bytes/px). A taller page renders its head, truncated.
- A scene authored taller than the texture does not clip cleanly; large enough it
  rasterizes to nothing (the reported "no fetched media yet" blank on a 166 KB
  gemtext capsule laid out to ~19000 px).
- `card.rs:337` the HTML/serval lane (`html_scene`) returns `(scene, h, Vec::new())`:
  it reports the viewport height as the content extent (so web pages never scroll)
  and empty link rects (so their links are inert).

Four table-stakes trace to this one decision: the page is a single capped texture
with no retained, queryable text model.

## What the packet already carries (so this is harvest, not rebuild)

`document_canvas::DocumentRenderPacket` (`crates/inker/document-canvas/src/types.rs:191`)
is a pure-data record the host currently **drops** right after lowering:

- `content_bounds: Rect` (`types.rs:195`): the true total extent, already computed.
- `blocks: Vec<RenderedBlock>` (`types.rs:148`): each block has `source_block_index`
  (correlates rendered geometry back to the `EngineDocument` source blocks, which
  hold the text), `bounds` (packet-local), and a `kind` carrying `glyph_runs`.
- `GlyphRun` (`types.rs:119`): positioned glyphs (`origin`, per-glyph `x`/`advance`,
  `baseline_y`, `font_face`).
- `interactions: Vec<InteractionRegion>` (`types.rs:177`): the link rects, in
  full-document space, already harvested as `LinkHit` at `card.rs:154`.

Retaining the packet per node gives true height, block geometry, and link
geometry immediately. The one gap for **precise** find-highlight and caret
selection is a character-to-glyph cluster mapping: `PositionedGlyph` (`types.rs:93`)
holds a `glyph_id` but no source-char offset. parley exposes clusters during
layout; surfacing a char range per glyph (or per cluster) is a document-canvas
change, isolated to Phase 3.

## Approach

Stop authoring one full-height scene into one capped texture. Instead **lay out
once, retain the packet, and lower plus rasterize only the window the user is
scrolled to**, with the full `content_bounds.height` driving the scroll range. The
paint-list translator already culls outside `packet.viewport`; today the code
defeats that cull by expanding the viewport to full height. Inverting that (set
the viewport to the visible band, translated to texture origin) renders just that
band into a viewport-sized texture, removing both caps and the body truncation.

Windowed rendering and the retained-text model are the same move: once the packet
is retained per node, the band lower falls out of it, and find/selection query the
same retained packet.

## Phases

Each phase is independently shippable and verifiable; later phases depend on the
retained packet from Phase 1.

### Phase 0 — Probe (no behavior change)

Confirm the two load-bearing assumptions before touching the render path:
- The paint-list translator culls strictly to `packet.viewport` (so a band-viewport
  lowers only that band). Verify in `document-canvas` `paint_list.rs` / the netrender
  backend.
- parley clusters are reachable at layout time to map a source-char range to glyph
  rects (sizing the Phase 3 cluster work).

Done: a short findings note in this doc; no code change.

### Phase 1 — Windowed render + retained packet (clears the blocker)

- Host keeps the laid `DocumentRenderPacket` (plus its font sidecar) per node,
  keyed by `(node, width)`; re-layout only on width or content change.
- `render_card_scene` becomes scroll-aware: lower the band `[scroll_y, scroll_y + h]`
  (translated to origin) into a viewport-tall texture, instead of the full height.
- Rasterize with vertical overscan (a band taller than the viewport) and keep the
  free UV-shift within the overscan; re-raster only when the scroll leaves the
  retained band. Scroll range is `content_bounds.height - visible_h`.
- Retire `bound_document_body`'s 12 KiB truncation and the `MAX_CARD_TEX_H` height
  cap. Keep an area/limit guard sized to the **band** texture, not the whole page.

Done: the 166 KB capsule renders and scrolls top to bottom with no truncation and
no blank; memory per card stays bounded by the band, not the page.

### Phase 2 — Query API over the retained packet

- Expose `point -> (block, link?)` and `block -> rect` queries on the retained
  packet (the host's link hit-test moves onto it; today it walks a separate
  `Vec<LinkHit>`).
- Foundation for find and selection; no new user surface yet.

Done: link hit-testing and hover-status read from the retained packet; the
parallel `LinkHit` vec is subsumed.

### Phase 3 — Find-in-page (Ctrl+F)

- Search the retained source text (via `source_block_index` into the
  `EngineDocument` blocks).
- Add per-glyph (or per-cluster) source-char offsets to `GlyphRun` in
  document-canvas so a match maps to precise glyph rects; coarse fallback
  highlights the matching block until that lands.
- Ctrl+F overlay: query field, match count, next/prev, highlight rects composited
  over the card, scroll-to-match.

Done: Ctrl+F highlights matches on a fetched gemtext page and steps between them.

### Phase 4 — Text selection + copy

- Caret + drag selection over the retained packet (reusing the Phase 3 cluster
  mapping for caret placement); Ctrl+C copies the selected source text.

Done: a paragraph on a fetched page is selectable and copyable.

### Phase 5 — HTML/serval lane parity

- `html_scene` reports the true laid-out height (so HTML cards scroll) and harvests
  `<a href>` rects off serval's fragment plane (so HTML links navigate), replacing
  the `(scene, h, Vec::new())` stub at `card.rs:337`.

Done: a fetched HTML page scrolls fully and a link on it navigates.

## Done condition (whole plan)

Matches the audit's Lane 2 acceptance: a 166 KB capsule renders and scrolls fully;
Ctrl+F highlights; a paragraph is selectable and copyable; a link on a fetched HTML
page navigates.

## Risks and notes

- **Scroll smoothness.** Today scroll is a free UV shift. Windowed render trades
  some of that for unbounded height. The overscan band keeps the common case a UV
  shift and re-rasters only on band exit; tune the overscan against measured scroll
  feel rather than guessing a constant.
- **Per-surface scenes interaction.** This sits next to the deferred per-surface
  scene work (a node drawn in card and tile at once at different sizes). The
  retained packet is keyed by `(node, width)`, which is the same seam that fix
  needs; sequence them so they share the key rather than fork it.
- **document-canvas is a tier-1, wasm32-portable crate.** The cluster-offset
  addition (Phase 3) must stay CPU-only and serializable, consistent with the
  crate's existing constraints (`types.rs` derives `Serialize`/`Deserialize`).
- **No placebo.** Phase 1 must actually remove the caps, not raise the constant.
  A taller constant is the same failure deferred.

## Findings

(Phase 0 results land here.)

## Progress

- 2026-06-15: Plan drafted. Current state verified against `card.rs`, `render.rs`,
  and `document-canvas/src/types.rs` (line refs above confirmed this session).
  Awaiting sign-off on phasing before Phase 1.
