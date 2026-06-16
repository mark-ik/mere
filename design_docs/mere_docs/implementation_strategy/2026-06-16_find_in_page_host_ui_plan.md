# Find-in-Page — Host UI Plan

**Date**: 2026-06-16
**Status**: Planned; backend done + in tree, host UI to build.
**Scope**: The Ctrl+F find-bar + match-highlight overlay over the **HTML/serval lane**, on top of
the committed find-in-page backend (serval `28e3e553`, mere `eefbed6`). A Lane 1 browser table-stake
from the [in-the-wings audit](../research/2026-06-15_in_the_wings_and_browser_bar_audit.md) §4.
**Related**: the 2026-06-16 find-in-page handoff addendum (the source of the file:line pointers).

---

## Backend (done, consumed read-only on `Constellation`)

`crates/meerkat/src/constellation.rs`:

- `request_find(member, query: &str)` (`:427`) — send the query to the content actor (deduped vs the
  last query; empty clears). Call whenever the find query changes.
- `find_matches(member) -> &[Vec<[f32;4]>]` (`:449`) — match rects in full-document px (one inner
  `Vec` per match; a wrapped match spans lines). **Same coordinate space as the link rects**, so map
  into the card exactly like `card_link_at` (subtract card origin, add `view.scroll[member]`).
- Matches arriving set `out.any_scene`, so a redraw fires automatically.

**Scope:** HTML/serval lane only (the actor searches serval's laid-out text). The **document lane**
(gemtext/markdown) needs source text + a glyph→char map on `DocumentRenderPacket`
(`inker/document-canvas/src/types.rs` `GlyphRun`/`RenderedBlock` ship glyph IDs, not chars) — a
separate follow-on. So HTML find lands first; gemtext find is a later piece.

## Host pieces (each mirrors an existing pattern)

Confirmed against the live palette pattern (`lib.rs` `palette_open`/`palette`/`palette_input` at
:88/:92/:95; `toggle_palette`/`open_palette`/`close_palette` at :432/:442/:450; `run_palette_selection`
at :476).

1. **Chrome find state + methods** (`lib.rs`). Add `find_open: bool`, `find_input: TextInput`,
   `find_active: Option<usize>` (active-match index) to `Chrome`; `toggle_find` / `open_find` /
   `close_find` mirroring the palette trio. Init the fields in the `Chrome` constructor. *(Lowest
   risk; do first.)*
2. **Ctrl+F binding + `on_find_key`** (`input.rs`). Bind Ctrl+F in `on_key_pressed` (the :1026-1093
   region) before the clipboard/field tail; add `on_find_key` (mirror `on_palette_key` at :1365):
   query-edit / Enter = next / Shift+Enter = prev / Esc = close, routed via a `find_open` guard near
   :1130.
3. **Query → `request_find`**. On the find query changing (after an edit in `on_find_key`), call
   `constellation.request_find(focused_member, query)`. The actor pipe does the rest.
4. **Find bar render** (`views.rs`). Render `find_bar` in `chrome_view` (:153) mirroring
   `palette_overlay` (:238), docked top-right under the toolbar; show the active/total match count.
5. **Highlight overlay** (`render.rs`). A new composite layer right after the card composite loop
   (~:978), before scrying. For each rect from `find_matches(member)`, convert content-local →
   window with the composite loop's `band_y`/`scroll`/`dest` math (:946-963), then draw a translucent
   rect via `compose_external_texture` (the drop-target overlay idiom at :1189-1216, or the favicon
   image-layer idiom). Tint the `find_active` match differently.
6. **Next/prev + auto-scroll**. Cycle `find_active` (mod match count); auto-scroll by setting
   `view.scroll[member]` to bring the active match's y into view.

Everything reuses existing host infrastructure (palette chrome, drop-target overlay, `view.scroll`);
no new plumbing.

## Build order (compile-checked stages)

- **S1 — input half:** pieces 1 + 2 + 3 + 4 (chrome state, Ctrl+F + `on_find_key`, query→request_find,
  the find bar). At the end the bar opens, takes a query, sends it to the actor, and matches arrive
  (redraw fires) — not yet highlighted. Compile-check.
- **S2 — overlay half:** pieces 5 + 6 (the highlight overlay + next/prev + auto-scroll). The
  user-visible payoff. Compile-check; on-screen verify against a long HTML page.

## Progress

- **2026-06-16** — Plan written from the handoff + a read of the live palette pattern. Backend
  confirmed in tree (`request_find`/`find_matches` on `Constellation`). Next: S1.
- **2026-06-16** — **S1 (input half) DONE.** Chrome `find_open`/`find_input`/`find_active` +
  `toggle_find`/`open_find`/`close_find` (mirroring the palette); a `WindowCtx::toggle_find` wrapper
  (focuses the `.find-bar` field on open, clears matches + refocuses the omnibar on close); `on_find_key`
  (Enter = next, Shift+Enter = prev, Esc = close, edit → `dispatch_key` + `submit_find_query`);
  `submit_find_query` → `constellation.request_find(focused_member, query)`; Ctrl+F binding +
  a `find_open` key guard (after the palette guard); a `find_bar` view docked top-right under the
  toolbar + its CSS. `cargo check -p meerkat` clean. So: the bar opens, takes a query, sends it to the
  actor, matches arrive (auto-redraw) — not yet highlighted. **Known follow-up:** paste-into-find
  (`handle_clipboard_shortcut` routes to omnibar/palette, not the find field). Next: S2 (overlay).
