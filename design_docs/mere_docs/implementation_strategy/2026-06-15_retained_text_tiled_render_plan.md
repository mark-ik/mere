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

### Phase 1 — Windowed render + retained packet (clears the blocker) — DONE 2026-06-16

- Host keeps the laid `DocumentRenderPacket` (plus its font sidecar) per node, on the
  constellation `Activation`; re-laid by the actor only on a new document / size.
- The render loop is scroll-aware: it picks a band centred on the scroll, windows the
  retained packet to it (`DocumentRenderPacket::window`), lowers that band
  (`card::lower_window`), and rasterizes a `band_h`-tall texture. It UV-shifts within
  the band for fine scroll and re-rasters only when the scroll leaves it (the band-y
  is stored in `view.tile_bands`). Scroll range is the full `content_height`.
- Retired `bound_document_body`'s 12 KiB truncation and the `MAX_CARD_TEX_H` (8192 px)
  height cap. The texture is bounded by `BAND_CAP` (6144 px) and the area guard.

Done: verified headed on `gemini://geminiprotocol.net/docs/faq.gmi` (the reported
blank-page blocker). The page renders, scrolls monotonically top to bottom (controlled
steps: intro → 1.2 → benefits → "wrong tool" limits, all forward), reaches deep
content (section 2.5+), and returns cleanly to the top. 91 meerkat bin tests green.

**Architecture note (DOC_POLICY §9):** layout stays off-thread in the content actor
(the expensive parley pass), but band **lowering** moved to the host render loop. A
band is a few visible blocks, so the per-band lower is cheap; the actor model's "scenes
travel as messages" now reads "packets travel as messages" for the document lane (the
HTML/serval lane still ships a pre-lowered scene until Phase 5). The retained packet
host-side is exactly what Phase 2's query API (find / selection / hit-test) reads.

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

**Attempt + finding (2026-06-16, reverted): HTML scroll needs band virtualization,
not just the true height.** serval exposes the height cheaply (added
`document_scroll_range` + a `paint_list_and_scroll_range_from_layout_dom` entry that
emits taller than the viewport). But the host CANNOT window a serval scene the way it
windows the document packet: serval hands back one flat `netrender::Scene`, not a
queryable packet, and the translator culls paint commands below the emit viewport. So:
emitting at the viewport height clips to one screen (scrolling hits blank past it); and
emitting at the full content height makes a dense real page (ycombinator.com, ~7500 px)
**OOM vello at rasterize** — it is the paint-op *density*, not the texture size, so
capping the texture height does not help. The document lane avoids this by windowing
the retained packet (only the visible band's blocks lower). The HTML lane has no packet
to window, so the real fix is **actor-side band re-emit**: a `Scroll` command drives the
content actor to re-emit the page at the scroll offset (one viewport band), shipping a
band scene the host composites — mirroring `window_packet`, but in the actor since only
it holds the serval layout. Links are independent of this (a `<a href>` rect harvest off
the fragment plane needs no scroll mechanism) and can land first. The first attempt's
code was reverted to no-regression; the serval `document_scroll_range` plumbing was kept
as the reusable foundation.

**Built + verified (2026-06-16): actor-side band re-emit, HTML scroll works.** The fix
shipped as the finding prescribed:

- serval grew `paint_list_band_from_layout_dom` (`serval-layout/lib.rs`): cascade + layout
  at the real viewport, compute `document_scroll_range` for the full height, then
  `emit_paint_list_scrolled` with `viewport = (w, band_h)` and `viewport_scroll =
  (0, band_y)`. The translator culls to the band viewport, so the returned flat scene holds
  only the band's ops, not the whole page's. This is what dodges the OOM: density, not
  texture size, was the limit, and a band re-emit bounds density.
- The content actor (`content.rs`) carries a `band_y` / `band_h` and accepts a `Scroll`
  command (distinct from `Resize`: viewport generation unchanged); on it, it re-renders
  the current document at the new band and echoes the band back on `ContentUpdate::Scene`.
- The constellation (`constellation.rs`) records the band each scene covers
  (`scene_band`) and exposes `request_scroll(member, band_y, band_h)`, deduped against the
  last request so holding still does not re-command (and re-lay-out) the actor every frame.
- The host band loop (`render.rs`) branches by lane: the document lane windows its retained
  packet host-side (unchanged); the HTML lane asks the actor for the band centred on the
  scroll (`request_scroll`) and composites whatever band has arrived (`scene_band`), UV =
  `scroll − band_y`. A fresh band bumps `scene_version`, so the texture cache re-rasterizes
  exactly when a new band lands and only UV-shifts when holding still. HTML bands are capped
  near 2× the visible window (`HTML_BAND_CAP = 2560`) to keep per-band op density near the
  single viewport that always rendered.

Verified headed (61 lib + 93 bin tests green): a tall HTML page (a long Wikipedia article)
scrolls fully in both directions, content re-emitting per band, with no OOM, no panic, no
`unknown ImageKey`, and no vanish — the exact failure modes the first attempt hit.
ycombinator.com itself stays a short "peephole" (its body is JS/image-driven and collapses
in serval's static layout, so it has almost no scroll range); that is a serval layout-fidelity
matter, separate from band windowing, which the tall-page test isolates and confirms working.

**Built + verified (2026-06-16): `<a href>` rect harvest, HTML links navigate.** The
host pipe (`Activation.links` → `Constellation::link_at`'s HTML branch → `card_link_at`
→ `resolve_href` → `follow_link`) was already wired and consumed; the only gap was the
producer (`html_scene` returned `Vec::new()`). Filled:

- serval grew a `link_harvest` module, folded into `paint_list_band_from_layout_dom` (so
  it reuses the band's existing layout pass, no second cascade/layout). Two anchor shapes,
  both emitting **full-document px, unscrolled** rects (band-independent — the host adds
  the card's scroll itself): **text anchors** map each `<a href>` to its byte span within
  its inline-formatting leaf (via `BoxTree::inline_sources`) and reuse `caret::selection_rects`
  for one rect per line box (the same per-line geometry selection highlighting uses, so a
  wrapped link is N boxes, not one gutter-spanning union); **boxed anchors** (`display:block`
  / `inline-block`) add their border-box rect for a full-width clickable row.
- `scene_from_layout_dom` + `card::html_scene` thread the `(href, rect)` list through and
  lower it to `LinkHit`s.

Verified headed: on the Rust Wikipedia article, clicking the "Main page" sidebar link
navigated to `en.wikipedia.org/wiki/Main_Page` (relative `/wiki/Main_Page` resolved against
the base) and rendered the Main Page, whose own links harvested in turn. Unit-tested in
serval (`link_harvest`: inline line rect, block box, no-links) and meerkat
(`html_lane_harvests_link_hit_regions`).

Still open in Phase 5: an `<a>` that wraps only a replaced element while staying `inline`
(image-only inline link) establishes no box and carries no text, so it is not yet harvested.
Perf note: links re-harvest on every band re-emit (they are band-independent); layout
already dominates per band, but a tall page with hundreds of links could cache them.

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

### Phase 0 probe (2026-06-15)

1. **The packet is retainable for free.** `DocumentRenderPacket` is pure data,
   built every layout and dropped right after lowering. Retaining it per node
   costs only the keep.
2. **document-canvas does not cull; the cull is downstream.** `paint_list_from_packet`
   (`paint_list.rs:105`) iterates and emits **every** block; `packet.viewport` rides
   along only as metadata into the `InkerPaintList`. The actual viewport cull lives
   in `paint_list_render::translate_paint_list` / the rasterizer (proven by the
   existing workaround at `card.rs:159`, which expands the viewport to full height
   precisely to stop content being culled).
   - **Refinement to the approach.** Windowing cannot be done by shrinking
     `packet.viewport` alone: the builder would still emit blocks at their full-document
     y, then they would be clipped (wasteful, and the over-tall-texture failure path).
     The correct mechanism is a pure `window_packet(packet, band_y, band_h) ->
     DocumentRenderPacket` in document-canvas that **filters** blocks whose `bounds`
     intersect the band and **translates** the survivors (and nested `Group` children,
     and `interactions`) by `-band_y`, then sets `viewport = (w, band_h)`. The host
     lowers that windowed packet and rasterizes a band-tall texture. Portable, testable
     on the packet type, no host coupling.
3. **parley clusters are reachable for Phase 3.** The glyph harvest at `text.rs:231-275`
   already holds the parley `Run` (`text.rs:240` `parley_run.run()`); parley exposes
   per-cluster source text ranges there. Surfacing a source-char range onto
   `PositionedGlyph` / `GlyphRun` is a bounded document-canvas change. Searchable/copyable
   text itself comes via `RenderedBlock::source_block_index` into the `EngineDocument`
   blocks, so find-in-page and copy do not need the glyph-level mapping except for
   precise highlight/caret rects.

**One open question for Phase 1 (resolve empirically, not by static trace):** whether
`core.rasterize` clips to the scene's `viewport_height` or strictly to the passed
texture `(w, h)`. If the latter, `window_packet` need only translate (the texture
bound clips the band); if the former, it must also set `viewport = band_h`. Phase 1
sets both (harmless) and confirms on a headed tall-page run.

## Progress

- 2026-06-15: Plan drafted. Current state verified against `card.rs`, `render.rs`,
  and `document-canvas/src/types.rs` (line refs above confirmed this session).
  Awaiting sign-off on phasing before Phase 1.
- 2026-06-15: Phase 0 probe done (findings above). Sign-off received for Phase 0+1.
- 2026-06-16: Phase 1 landed. `DocumentRenderPacket::window` added + unit-tested in
  document-canvas (commit 35e3c4b). Host wiring: `card::render_content` forks the
  lane (document packet vs HTML scene) and `card::lower_window` lowers a band;
  `content.rs` ships a `Document` update; the constellation retains packet+fonts and
  exposes `packet()`; the render loop bands + UV-windows it; the 12 KiB body cap and
  8192 px texture cap are gone. 91 meerkat bin tests green; headed faq.gmi verified
  (renders, scrolls fully, monotonic). Next: Phase 2 query API over the retained
  packet.
