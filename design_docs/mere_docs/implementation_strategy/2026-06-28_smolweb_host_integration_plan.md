# Smolweb Host Integration Plan — the serval native lane in meerkat

**Date**: 2026-06-28
**Status**: **P1 landed 2026-06-28** (serval `1bbbfdb`, mere `476880b`). P2–P4 remain.

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
  retention).
- **Focused-vs-card selection**: confirm the trigger for "focused tile → serval lane"
  vs "card → block lane" in the existing focus model (`compute_focus_cards` /
  `collect_cards`).
- **Click→navigation**: how a serval-lane link target becomes a kernel node op
  (existing omnibar/navigate path reused).

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
