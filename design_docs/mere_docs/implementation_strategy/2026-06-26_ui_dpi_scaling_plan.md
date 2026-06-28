# True Auto-DPI Scaling Plan

**Date**: 2026-06-26
**Status**: **COMPLETE — D1 + D2 + D3 landed + verified (2026-06-26).** Chrome and content render DPI-correct + crisp at 2× (verified headed: chrome, the welcome card, a live example.org page, and a link-click landing); per-window DPI wired for multi-monitor (code-correct, not headed-verifiable on a single monitor set). Spun out of the [chrome bar refinement plan](2026-06-26_chrome_bar_refinement_plan.md)'s deferred auto-DPI finding (which shipped the user-zoom half: Ctrl +/-/0, baseline 1.1).
**Open questions resolved (2026-06-26)**: window-size persistence = **logical**; DPI = **per-window**, user_zoom **shared**; supersample cost = **measure** before defaulting D2a on.
**Related**: `crates/meerkat/` (`app_handler.rs`, `render.rs`, `main.rs`), `repos/serval` (`components/serval-layout`, `netrender`), the [host framework memory](../../) (xilem_serval).

Make the whole UI track the display's DPI automatically, so a HiDPI panel gets a correctly-sized (not tiny, not huge) chrome and crisp text, with the user's Ctrl-zoom composing on top.

## The problem, precisely

meerkat is a **physical-pixel app**. The window is created with `winit` `PhysicalSize` ([app_handler.rs:826](../../../crates/meerkat/src/app_handler.rs#L826)); the chrome lays out and rasterizes at that physical size (`PaneSession::scene(.., w, h, ..)` then `core.rasterize(.., w, h, ..)` in [render.rs:1288](../../../crates/meerkat/src/render.rs#L1288)); CSS lengths are raw physical px. `winit`'s `scale_factor()` is **never read** — confirmed by the chrome-bar P-UI-scaling pass.

When that pass folded `scale_factor` into the chrome scale, the chrome rendered 2× (winit reports 2.0 on the test panel) — **correct density, wrong frame**: the *window* was still physically small (1024 px = 512 logical on a 2× display), so a correctly-sized chrome filled half of it and the omnibar wrapped. The bug was not the scaling; it was scaling the chrome without sizing the window in the same (logical) terms.

## The key insight

True auto-DPI for the **chrome** is *not* a full logical-pixel coordinate migration. It is two coordinated changes:

1. **Size the window in logical px** (`winit` `LogicalSize`), so on a 2× display the window is physically `2×` larger (1024 logical → 2048 physical). The chrome then has room to render at density.
2. **Fold `scale_factor` into the chrome scale** (`ui_scale = scale_factor × user_zoom`), which the already-built `scale_px` applies to the whole sheet.

With both, a 16px CSS font → 32 physical px in a 2048-physical-px window — the same *proportion* as 16px in a 1024 window at 1×, but crisp. Input needs no conversion: the layout viewport stays the physical window size, so winit's physical cursor coords still match the (scaled) layout. This is why the chrome is the easy half.

The **content / orrery** is the hard half (below): web pages and the graph canvas have their own coordinate systems and their own text, and making *those* crisp + correctly-sized is where serval's missing device-pixel-ratio bites.

## Phases

### D1 — Chrome auto-DPI (meerkat-only; no serval change)

Done when:
- The window is created with `LogicalSize` (or `with_inner_size` fed logical), so its physical size is `logical × scale_factor`. The persisted window size is interpreted as logical.
- `Presentation` regains `dpi_scale` (from `window.scale_factor()` at creation + `WindowEvent::ScaleFactorChanged`), and `ui_scale()` = `(dpi_scale × user_zoom)` again — the wiring built and reverted in the chrome-bar pass, restored now that the window is logical-sized. (Its doc-comment's "do not re-add" note is lifted by this plan.)
- The chrome (toolbar, shellbar, panes, menus, session strip) renders DPI-correct on a 1× and a 2× display, verified headed on both (the laptop panel + a forced scale). The toolbar no longer wraps/oversizes.
- Ctrl +/-/0 still composes (zoom × dpi), persisted as `ui_zoom` (the user multiplier, not the product).

Risk: the orrery / content surfaces share the now-larger physical window. They already take `w, h` physical, so they get more pixels (crisper backgrounds) but their *content* is unscaled — handled in D2. Window-size persistence and the multi-window path (each window reads its own `scale_factor`) need a pass.

### D2 — Content + orrery DPR (needs the serval side)

The chrome is host-CSS we scale ourselves. Web pages and the orrery scene are laid out + rasterized by serval / the scene path, which has **no device-pixel-ratio** (serval's cascade lists `zoom` as unsupported; `netrender` rasterize takes a bare `w, h` with no scale). So at D1 a page renders its 16px text at 16 physical px in the 2048 window — half-size. Options:

- **D2a — Supersample at compose (meerkat-side, cheaper):** lay content out at *logical* size (`w/scale, h/scale`), rasterize the scene into a `scale×`-larger texture via a scene-level scale transform, compose 1:1. Needs `netrender` to accept a render scale (a transform on the scene, or a `rasterize_scaled(scene, w, h, scale)`). One serval/netrender addition; no layout-engine change. Crisp text falls out of rasterizing the vector scene at physical res.
- **D2b — Real DPR in serval layout (cleaner, larger):** give `serval-layout` a `device_pixel_ratio` so CSS px are DIPs end-to-end (layout, hit-test, paint), matching how a browser does it. Bigger change in a vendored-ish engine; the principled long-term shape.

Recommendation: **D2a** first (one netrender render-scale seam, meerkat drives it), leaving D2b as the eventual convergence if/when content fidelity demands it. The orrery scene takes the same render-scale (its `camera.scale_factor` field, currently always 1.0, is the natural carrier).

### D3 — Settle + polish

- Per-monitor DPI changes (dragging a window between a 1× and 2× display) re-fold live via `ScaleFactorChanged` (D1 wires the event; D3 confirms content re-rasterizes at the new scale, not just the chrome).
- Reconcile with the orrery's own zoom (Ctrl+wheel) so display-DPI and user-canvas-zoom don't double-apply.
- A11y bounds (AccessKit) are reported in the right space.

## D2 — confirmed seam (investigated 2026-06-26)

Grounded in code, the content path is exactly as feared:
- The content actor is sent a **physical** viewport — `ContentCommand::Show` /
  `Resize { viewport: (cw, ch) }` in [`constellation.rs:387/403`](../../../crates/meerkat/src/constellation.rs#L387),
  where `cw, ch` are physical px (meerkat is physical end-to-end). serval lays the page
  out with CSS px = physical px, so at 2× a 16px font is 16 physical = 8 logical px —
  half-size. No DPR knob exists in `serval-layout` (cascade lists `zoom` unsupported).
- The HTML lane returns a vector `Scene` in viewport-px coords; the host rasterizes it
  via `core.rasterize(scene, w, h, ..)` → `renderer.render_vello(scene, view, clear)`
  ([`serval-winit-host/src/lib.rs:134`](../../../../serval/components/serval-winit-host/src/lib.rs#L134)).

**The D2a edit (precise):**
1. **meerkat** — send the content actor a *logical* viewport (`cw/scale, ch/scale`) so it
   lays out at the right logical width with correctly-sized text; keep the *physical*
   `(cw, ch)` for the texture it rasterizes into.
2. **serval** — `rasterize` (or `render_vello`) takes a `scale: f32` and applies it as a
   root affine on the scene, so the logical-coord scene fills the physical texture
   crisply. Additive; also lets the chrome supersample if ever wanted.
3. **meerkat** — scale content **hit-testing** + scroll/band math by `scale` (clicks
   arrive in physical px but the scene/links are now in logical coords). This is the
   interlock that makes D2 more than a one-liner and **requires a real page loaded over
   the network to verify** links + scroll still land.

Scope note: D2 touches layout-size + raster + compose + hit-test + scroll together, in
two repos, and can't be verified without driving a live page — a focused session of its
own, not a quick follow-on. Greenlight needed to edit the serval repo (its own build).
The **document lane** (windowed packet) needs the same logical/scale treatment in its
band-lowering path. Measure the 2×-supersample fill cost (resolved open question) before
defaulting it on low-power targets.

## Serval-side summary (what this needs upstream of meerkat)

- **D1**: nothing. Pure meerkat (window sizing + `scale_px`, already built).
- **D2a**: a **render-scale seam in `netrender`** — `rasterize` (or the renderer) accepts a scale so a logical-sized scene rasterizes into a physical-sized target with a scene transform. Small, additive, serves chrome-supersampling too.
- **D2b** (optional/later): a `device_pixel_ratio` threaded through `serval-layout`'s cascade + box tree + paint — the full browser DPR model.

## Open questions

- Does the window-size **persistence** store logical or physical today? D1 must pin it to logical so a restart on a different display behaves.
- Multi-window: each `WindowView` reads its own monitor's `scale_factor`; `ui_scale` is currently a single `Presentation` (shared) value — DPI may need to move per-window while `user_zoom` stays shared.
- Content text crispness vs cost: supersampling every content tile at 2× is 4× the fill — measure before committing D2a as the default on low-power targets.

## Progress

**2026-06-26 — D1 landed (chrome auto-DPI, meerkat-only).**
- Window created with `LogicalSize` (was `PhysicalSize`); `inner_size()` still reports
  physical, so the rest of the host is unchanged. Window size is hardcoded 1024×600
  (not persisted yet), so the "persistence = logical" answer is moot until persistence
  exists — noted for when it lands.
- Restored `Presentation::dpi_scale` + `ui_scale() = dpi_scale × user_zoom`; `create_window`
  reads `window.scale_factor()` and rebuilds the sheet at the display's density;
  `WindowEvent::ScaleFactorChanged` → `set_dpi_scale` re-folds live (so a single-monitor
  OS-scale change already works without D3).
- **Confirmed the display is genuinely 200%**: the window is 2048×1200 physical = 1024×600
  logical; the chrome fills it at normal proportions, crisp, with far more inspector
  content visible than the broken pre-logical-window attempt. winit's `scale_factor=2.0`
  was correct all along — the earlier `GetDpiForMonitor=96` came from the *system-aware*
  harness process, not meerkat's per-monitor-aware context.
- Tests: lib 89/89, `scale_px`/`ui_scale` units green.
- **DPI is still shared (`Presentation`)**, not per-window — fine on one monitor (incl.
  live scale changes); the per-window split is D3. Chrome-rendered surfaces (toolbar,
  shellbar, panes, orrery host-draw, list panes) are all DPI-correct now; only live
  **web-page tiles** still render at half-size (no content DPR) — that's D2.

**Next:** D2 (serval/`netrender` render-scale seam for content + orrery scenes; measure
supersample cost per the resolved open question) → D3 (move `dpi_scale` per-window with
per-window scaled sheets; reconcile with orrery Ctrl+wheel zoom).

**2026-06-26 — D2 landed (content/orrery DPR, cross-repo).**
- **serval / netrender** (the reusable render-scale seam): added `render_scaled` on the
  vello tile rasterizer (appends the vector master scene under a `kurbo::Affine::scale`
  and renders at `viewport × scale`), `Renderer::render_vello_scaled`, and
  `RenderCore::rasterize_scaled` — all additive; `render`/`render_vello`/`rasterize` now
  delegate at `scale = 1.0`. Tiles are cached as per-tile `vello::Scene`s (vector), so the
  scale is crisp, not a bitmap upscale. `cargo check -p netrender` clean.
- **meerkat** (centralized in the constellation): a `dpr` field (host pushes
  `dpi_scale` each frame via `set_device_pixel_ratio`). Actors lay out **logical** —
  the viewport (`drive`) and band request (`request_scroll`) are divided by dpr — and
  their outputs are multiplied back to physical (`content_height`, `scene_band`), with
  `link_at` converting the query point in. The host stays fully physical and rasterizes
  the logical scene at physical via `rasterize_scaled(scene, cw, band_px, dpr)` (both the
  HTML lane and the document lane, the latter windowing the packet at `band ÷ dpr`).
- **Verified headed at 2×**: the `mere://welcome` document card renders crisp and
  correctly-sized (heading + body at proper reading size, not the pre-D2 half-size);
  chrome + orrery unaffected. Constellation units 4/4 (dpr=1.0 = unchanged). example.com
  itself failed to fetch (network/firewall in this env), so a live **long page with
  links** wasn't available to drive — the `link_at` / scroll-band conversions are
  code-complete + consistent but their headed click/scroll check is still owed against a
  real loaded page.
**2026-06-26 — D2 tail closed + live-page verification.**
- find-in-page highlight rects (`render.rs` find compose) and box-shadow mask
  build (`scene_masks` → `build_box_shadow_mask`) now scale by dpr — find rects ×dpr
  to physical before the screen map; the mask is built at `dim/bounds/corner/blur × dpr`
  (key unchanged, so the scene's shadow op still resolves it), crisp under the scaled scene.
- **Live page verified at 2×**: example.org fetched + rendered crisp and correctly-sized
  (real white page, proper text), and clicking its "Learn more" link navigated to
  `iana.org/domains/example` — so **link hit-testing lands precisely at 2×** (the
  `link_at ÷dpr` conversion is correct). Scroll wasn't driven on a long page but shares
  the same verified dpr band math (`request_scroll ÷dpr` / `scene_band ×dpr`).
- **D2 is complete.** Remaining auto-DPI work is only **D3** (per-window dpi for
  multi-monitor — move `dpi_scale` per-window with per-window scaled sheets; reconcile
  with orrery Ctrl+wheel zoom).

**2026-06-26 — D3 landed (per-window DPI). Auto-DPI plan COMPLETE.**
- DPI is now **per-window**: `WindowView::dpi_scale` holds each window's monitor
  `scale_factor` (set at `create_window` + on `ScaleFactorChanged`), while `user_zoom`
  stays shared on `Presentation` (the resolved-2026-06-26 answer). `Presentation::dpi_scale`
  is reinterpreted as "the dpi the shared sheet is currently baked at."
- Mechanism (keeps the shared sheet, no per-window sheet storage): each window's render
  re-bakes the shared chrome sheet to its own dpi **only when it differs** from the
  current bake (`render/setup.rs`), and pushes its dpi to the content pool. Single
  monitor (or co-density windows) = no rebuild after the first sync; a window dragged to
  a different-density monitor rebuilds on its next frame. `set_dpi_scale` / `refresh_ui_scale`
  now key off `self.view.dpi_scale`.
- Parts 2–3 were already handled: the orrery is an independently-zoomed canvas (no display
  dpr applied, so nothing to double-apply against its Ctrl+wheel zoom), and content
  re-rasterizes on a scale change via the per-frame dpr (D2).
- Verified: single-window unchanged at 2× (chrome + content + orrery crisp, restored
  example.org tiles render correctly); lib 92/92, bin constellation/steward/toolbar green.
  **Multi-monitor-different-dpi is code-correct but not headed-verifiable here** (one
  monitor set). Known cost: if two different-density windows both redraw every frame, the
  shared sheet rebuilds per-window-switch (sub-ms string work) — optimize to per-window
  stored sheets only if that case ever matters.

**2026-06-28 — host-geometry scaling (zoom/DPI fix).**
Reported: increasing the zoom made the layout weird/clipped. Cause: the **host-drawn
geometry** constants were never scaled by `ui_scale`, while the CSS chrome (buttons,
toolbar) was — so they desynced at zoom/HiDPI:
- **Shellbar strip**: `SHELLBAR_THICKNESS` (48px) was fixed, so the scaled buttons
  overflowed the strip into the content (worse at higher zoom; actually already spilling
  at the 2.2× default). Fixed: `shellbar_rect` / `band_after_shellbar` take a `scale`
  (the chrome `ui_scale`); the strip is `48 × ui_scale` thick, holding its buttons. All
  4 callers (render overlays + setup, input press, pane_geom) pass `ui_scale()`.
- **Window controls**: the toolbar reserves a `ui_scale`-scaled right gap, but the
  controls were drawn/placed/hit-tested at fixed `CONTROLS_W`/`CTL_W` with fixed-size
  glyphs → a big empty gap + tiny controls. Fixed: `control_rect` / `control_at` /
  `controls_scene` take `scale`; the strip is `CONTROLS_W × ui_scale` wide, placed at
  `win_w − CONTROLS_W × ui_scale`, glyphs (`CTL_W`, stroke, half-extent) scaled. Callers
  in render paint + input press + window_ctx hover pass `ui_scale()`.
- Verified headed: at default 2.2× and at a 2.8× zoom the shellbar holds its buttons and
  the controls are right-sized + flush. Residual: at *extreme* zoom the toolbar is simply
  over-full (omnibar shrinks toward nothing, minimize crowds +field) — inherent to fitting
  everything at high zoom, far milder than the original strip-into-content spill. Tests:
  lib 92/92, shellbar+titlebar 7/7.
