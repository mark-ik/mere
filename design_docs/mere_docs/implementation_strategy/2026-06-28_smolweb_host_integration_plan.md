# Smolweb Host Integration Plan — the serval native lane in meerkat

**Date**: 2026-06-28
**Status**: **P1–P3 all landed 2026-06-28** (serval `1bbbfdb`, `0b7ca87`, `5c07ad5`;
mere `476880b`, `0dd0c3e`, `3eed418`, `8dc3683`). Core integration complete
(render, theme, scroll, link nav). P4 optional.

**Thesis**: render a focused smolweb capsule (gemini/gopher/feed/…) in the Mere host
through the **serval lane** — the native `smolweb-views` render we built and shipped
in pelt (`SmolwebDocument`) — instead of the block-card reader path. The block lane
stays for cards/orrery; the serval lane is the focused tile. This is one new rung on
meerkat's content render ladder, not a new subsystem.

**Builds on**: the native smolweb rendering effort
([../../nematic_docs/implementation_strategy/2026-06-27_native_smolweb_rendering_plan.md](../../nematic_docs/implementation_strategy/2026-06-27_native_smolweb_rendering_plan.md))
— errand transport+parse, `smolweb-views` (gemtext/gopher/feed views + per-site/app
theming), and `pelt_desktop::SmolwebDocument` (parse → view → `ScriptedDom` →
serval-layout → `Scene`, with scroll + chrome-browser link nav). Extends the render
ladder ([2026-06-23_render_ladder_and_extraction_plan.md](2026-06-23_render_ladder_and_extraction_plan.md)).

---

## The two lanes (settled)

Same fetched source, two distinct renderers — not two LODs of one render:

- **Block lane** (nematic → `Block` → `document_canvas` → `Scene`): the capsule as the
  semantic block model, shown as an **orrery card / reader**. Unchanged by this plan.
- **Serval lane** (errand parse → `smolweb-views` → `ScriptedDom` → serval-layout →
  `Scene`): the **native focused capsule** — themed, scrollable, link-navigable. New
  to meerkat (it already runs a serval lane for *HTML* via `StaticDocument`; this adds
  the *smolweb* serval lane).

Per-surface, automatic (Mark's call A): the focused tile uses the serval lane; cards
keep the block lane. No user toggle in v1 (the engine-activation system can add one
later).

## Why it fits meerkat as-is (findings, verified)

- The **content actor** ([crates/meerkat/src/content/mod.rs](../../../crates/meerkat/src/content/mod.rs))
  runs off the UI thread, owns the serval cascade + nematic engines + a subresource
  cache, runs `render_content_scene`, and **ships a `Send` `Scene` back**; the kernel
  composites and stays sole GPU owner. So a lane that produces a `Scene` already has a
  home — we just produce it via `SmolwebDocument`.
- meerkat **already dispatches lanes**: `card::is_serval_html_lane` routes HTML to the
  serval `StaticDocument` path; everything else falls to the nematic block path. The
  smolweb branch slots in beside `is_serval_html_lane`.
- meerkat **already deps the pieces**: `pelt-desktop` (`tile-surface`) + `xilem-serval`
  + `serval-scripted-dom` (git). `SmolwebDocument` lives in pelt-desktop behind the
  `smolweb` feature; turning it on is the dep step.
- **Thread safety**: `SmolwebDocument` is `Rc`-based (`ServalAppRunner` /
  `ScriptedDom`), so it is **not `Send`** — but it is built, framed, and dropped
  entirely on the actor thread (only the `Scene` crosses), exactly like the existing
  serval cascade the `cascade-offthread` probe blessed. No boundary change.

## Phases

- **P1 — the lane.** Enable `pelt-desktop/smolweb` (+ `errand`) in meerkat. In the
  content actor's render dispatch, add: if the node is a smolweb scheme/engine, render
  via `SmolwebDocument::parse(url, body, theme).frame(w, h)` → `Scene` (retain the
  `SmolwebDocument` per tile in the actor's `current` state so scroll persists). Done:
  a focused gemini node renders the native capsule (glyphs + per-site theme) instead
  of the block card.
- **P2 — host theme.** Map meerkat's tinct theme to a `SmolwebPalette` and pass
  `SmolwebTheme::App(palette)` so capsules match the app chrome; honor the OS scheme
  for `System`. (The seam exists in `smolweb-views`; this is the host-side map, the
  illume `SyntaxKind→SyntaxRole` pattern.)
- **P3 — input.** Scroll the retained `SmolwebDocument` from the tile's wheel/keys;
  route a link click (`SmolwebDocument::click_at → Option<String>`) to a node
  navigation (mint/open the target the way meerkat opens any content node), so
  in-Mere link-following matches the pelt chrome browser.
- **P4 — activation (optional).** Register the serval smolweb lane in
  `engine_activation` so it can be disabled per session (falling back to the block
  lane), turning Mark's "A automatic" into a selectable rung if wanted.

## Open questions

- **Retention shape**: where the per-tile `SmolwebDocument` lives in the actor's state
  and how navigation/resize invalidates it (mirror the HTML lane's `ContentLayout`
  retention). *Settled in P1: a `Content` field, built lazily, left to `frame`'s own
  size-change detection (no explicit invalidation needed).*
- **Focused-vs-card selection**: confirm the trigger for "focused tile → serval lane"
  vs "card → block lane" in the existing focus model (`compute_focus_cards` /
  `collect_cards`). *Not yet confirmed against that model — P1's `is_smolweb_lane`
  branches on the content actor's own dispatch, which the actor only serves for the
  focused tile; cards render through the separate `render_content_scene` path
  entirely, so no explicit cross-check was needed, but this is worth a second look.*
- ~~Click→navigation~~ — resolved, see Progress (P3b landed).

## Progress

- **2026-06-28**: Plan created. Design settled (serval lane = focused tile, block lane
  = cards; fits the content-actor `Scene` model).
- **2026-06-28**: **P1 landed.** `meerkat` `smolweb` feature → `pelt-desktop/smolweb`;
  `Content` gains a retained `SmolwebDocument` (feature-gated), built lazily in the
  content actor for a smolweb-scheme node with a ready body (`ensure_smolweb` /
  `is_smolweb_lane` in `content/handlers.rs`); `render` frames it to one viewport and
  emits `ContentUpdate::Scene` **exactly like the scripted lane** (internal scroll;
  host-band scroll deferred to P3). pelt-desktop re-exports `SmolwebTheme` /
  `SmolwebPalette` so the host can name the theme. Builds green both ways (`meerkat
  --features smolweb` and the base build; all additions cfg-gated). serval `1bbbfdb`,
  mere `476880b`. Compile-verified; runtime/headed check pending (mirrors the proven
  scripted lane). **Remaining**: P2 (host tinct → `SmolwebTheme::App`), P3 (scroll via
  the `band_y` delta → `SmolwebDocument::scroll_by`; link click → node navigation),
  P4 (optional activation rung).
- **2026-06-28**: **P2 landed** (mere `0dd0c3e`). The smolweb lane builds
  `SmolwebDocument` with `SmolwebTheme::App`, mapping the host theme-derived document
  colours (`sheet.colors`) + `CARD_BG` to a `SmolwebPalette` (rgb strings), so a native
  capsule themes like the app chrome instead of the per-site default. Rebuilt on
  `Retheme` (the actor clears the retained doc so the new sheet re-themes). Builds green.
- **2026-06-28**: **P3a (scroll) landed** (serval `0b7ca87`, mere `3eed418`).
  `SmolwebDocument` gained `content_height(w, h)` (viewport height + the layout
  session's `scroll_range` max — 2 new tests) and `scroll_to(y)` (absolute offset via
  `set_viewport_scroll`, the counterpart to the existing delta `scroll_by`/`scroll_at`/
  `scroll_for_key`). The smolweb render branch now reports the real content height
  and scrolls to `content.band_y` before framing, echoing `band_y`/`band_h` back —
  which plugs the lane into the **host's existing HTML-lane band protocol
  unchanged**: `constellation::request_scroll` already gates on `content_height >
  viewport` and dedupes `Scroll` commands against any Scene-based lane (it checks
  `activation.packet.is_none()`, which is already true for smolweb), so **no
  host-side change was needed** — only the actor-side content_height/scroll_to.
  Builds green (smolweb feature + base).
- **2026-06-28: P3b (link nav) — corrected and landed** (serval `5c07ad5`, mere
  `8dc3683`). The prior entry's "empty until Phase 5 lane parity" read was a stale
  doc comment, not the real state — a fresh investigation (dedicated agent pass,
  cross-checked by hand) found the mechanism **already works** for the block lane
  (`DocumentRenderPacket`'s static interaction list) and the HTML/serval lane
  (`serval_layout::link_harvest::harvest_link_rects`, wired through
  `ContentLayout::emit_band`, 4 passing tests). It is **not** a per-click round trip:
  the actor harvests every `<a href>`'s hit rect once at render time, ships the list
  alongside the scene (`ContentUpdate::Scene.links`), and the host caches + does its
  own point-in-rect test locally (`ConstellationOps::link_at`) — the same shape
  `ContentCommand::Find`/`FindMatches` uses for search. Only two lanes never called
  it: scripted-live and smolweb, both hardcoding `links: Vec::new()`. The harvester
  needs exactly the three fields `IncrementalLayout` already retains (`fragments`,
  `built`, `text_ctx` — the same session type `SmolwebDocument` *and* the
  scripted-live rung's `ScriptedDocument` both hold), so the fix was exposing it as
  `IncrementalLayout::link_rects` (was `pub(crate)`) and wiring
  `SmolwebDocument::links()` through the existing `LinkHit` field — no new
  `ContentCommand`, no round trip, architecturally identical to the working lanes.
  1 new serval test (8 total). Builds green. **Follow-on, not done here**: the
  scripted-live rung (`content.scripted_doc`) has the identical gap and the identical
  fix is now available (it retains an `IncrementalLayout` session too) — belongs to
  the render-ladder plan's lane, not this one. P4 unchanged.
