# Scrying Integration Plan — real WebView surfaces

**Date**: 2026-05-27
**Status**: Plan. Turns the proven external-surface *stub* (verso P3,
[verso adoption plan](2026-05-27_verso_adoption_plan.md)) into a real
`scrying` WebView surface. The stub composites a solid color into an
external GPU layer end to end (runtime-verified); this swaps the color for
live WebView frames. It is **not** a one-line swap — it's a platform WebView
integration with four real hurdles, scoped below.

---

## What's already proven (the foundation)

`widget reserves External layer → SurfaceRegistry → with_external_compositor →
wgpu composite → on screen`, outside Masonry's paint, positioned + persistent.
The compositor hook gets the shared `wgpu::Device`/`Queue` + render target +
the layers (`widget_id` + bounds). Replacing the stub fill with WebView pixels
is the whole job.

## The scrying surface, as it actually is

**Corrected 2026-05-27 after reading the producer.** The earlier two-tier
framing (CpuRgba first, zero-copy later) was wrong *for Windows*. The
WebView2 producer's streaming path (`acquire_full_frame` /
`try_acquire_frame`, backed by Windows.Graphics.Capture) emits **only**
`NativeFrame::Dx12SharedTexture` (Bgra8Unorm); it never streams `CpuRgba`.
The `CpuRgba`/`PngSnapshot` `WebSurfaceFrame` variants exist for snapshots and
for backends without a zero-copy path — not WebView2 streaming. So:

- **Windows real lane = zero-copy DX12 import.** `try_acquire_frame()` (the
  non-blocking, event-loop-safe path) →
  `WebView2CompositionFrame { frame: Native(Dx12SharedTexture), shared_handle,
  resource_is_new }` → `WgpuTextureImporter::import_frame` →
  `wgpu::Texture` on our device → `copy_texture_to_texture` into the composite
  target. Re-import only when `resource_is_new`; reuse the imported texture
  otherwise.
- **The hard gate**: `import_dx12_shared_texture` requires
  `host.backend == InteropBackend::Dx12` (it calls `device.as_hal::<Dx12>()`).
  We default to **Vulkan**, which cannot import a D3D12 shared texture. Fix:
  masonry reads `wgpu::Backends::from_env()` (`vello_util.rs`), so
  **`WGPU_BACKEND=dx12` forces the whole app onto DX12 with no fork edit** —
  vello renders fine on DX12. (Verified the app runs on DX12 before building
  the producer; see step 0.)

`HostWgpuContext::new(device, queue)` + `WgpuTextureImporter::new(host)` is the
import setup; `WebSurfaceCapabilities::probe` reports `imported_texture:
Supported` once the host is DX12.

macOS (`WkWebViewProducer` → IOSurface) and Linux (DMABUF) have their own
same-backend import paths; CpuRgba may still be the simplest *first* proof on
those, but Windows goes straight to zero-copy because that's all WebView2
streams.

## The four hurdles

1. **Parent window handle.** `WebView2CompositionProducer` needs the OS window
   (`CompositionRoot::new(parent_hwnd)`) for its DirectComposition target — and
   WkWebView/GTK likewise need the host window. The `with_external_compositor`
   ctx hands over wgpu but **not** the window, and `MasonryState` keeps its
   windows private (`roots()` exposes only render roots). → **Step 1**: add the
   window handle to `ExternalCompositeCtx` — masonry's `render()` already has the
   `Window` in scope right where it builds the ctx, so this co-locates the handle
   with the `device`/`queue` the producer also needs (one place, main thread, per
   frame; the closure lazily creates + owns the producer on first call). Cleaner
   than an `on_start` + shared-cell + window-id dance.
2. **Producer lifecycle.** A live system WebView: long-lived (one per surface
   tile, keyed by `TileId`), created lazily on first navigate, **never** rebuilt
   per frame. Owned in a host-side holder, not in the view tree.
3. **Frame pump, event-loop-safe — and it can't run inside `render()`.**
   *(Sharpened 2026-05-27 reading the WebView2 producer.)* The producer pumps the
   Win32 message loop in three places: construction (`create_environment` +
   `create_controller` → `wait_for_async_operation` → `pump_until`, up to ~5s),
   navigation-wait (the blocking `navigate_to_url`), and the **first**
   `try_acquire_frame` (a ~500ms first-frame nudge). The masonry
   `composite_external_layers` hook runs **inside `render()`** (inside winit's
   WndProc). Pumping the message loop there re-enters winit → a reentrant
   `RedrawRequested` → a reentrant `render()` borrowing the same `RenderRoot`
   mutably → panic/deadlock. So the producer **must be driven from outside
   render**. Steady-state `try_acquire_frame` after the first frame does *not*
   pump (it checks `frame_ready()` and returns), so the per-frame import is safe
   inside the compositor — only construct / navigate / first-frame need the
   outside-render context. Masonry today has **no** app-facing non-render
   main-thread hook (`handle_about_to_wait` is an empty stub that doesn't forward
   to `AppDriver`; `on_start` can't reach the window — `MasonryState::windows`
   is private). → **the step-2 fork** (below): add one.
4. **Platform variance.** WebView2 (Windows) first; the `PlatformWebSurfaceProducer`
   alias + `capabilities.probe` abstract the rest. macOS/Linux are follow-ups.

## Sequencing (each step runtime-verified; none verifies headless)

- **Step 1 — window-handle seam.** Add `pub window: &winit::window::Window` to
  `ExternalCompositeCtx` (masonry `render()` sets it from `window.handle()`). In
  mere-app's compositor closure, read `ctx.window.window_handle()` (via
  `raw-window-handle`) to get the Win32 HWND. Verify: log the captured HWND.
- **Step 0 — DX12 backend check.** Run `WGPU_BACKEND=dx12 cargo run` and confirm
  the orrery + the blue stub still render (vello on DX12). If DX12 breaks
  rendering, that blocks the whole Windows zero-copy lane and surfaces here, not
  after the producer's built. (Done 2026-05-27.)
- **Step 2 — producer holder + create + navigate. ← FORK (hurdle 3).** The
  producer can't be built/navigated inside the compositor closure (pumps the
  message loop → reentrant render). It needs an outside-render, main-thread,
  device-reachable home. Masonry has none today. Two ways:
  - **(B, recommended) Add an `AppDriver::on_tick` hook**, forwarded from the
    (currently empty) `handle_about_to_wait`, plus device access (via
    `MasonryState`'s `render_cx`). The producer lives in the app's tick handler:
    construct + `load_url` (non-blocking) + `try_acquire_frame` + import → write
    the imported `wgpu::Texture` into a shared cell. The compositor closure
    (inside render) just `copy_texture_to_texture`s the cell's texture — no
    producer touch, no pump, safe. One new forwarded `AppDriver` method +
    exposing the device to it; same shape as the `composite_external_layers` /
    `window` additions, slightly larger.
  - **(A, smaller, risky) Drive the producer in the compositor anyway**, accept
    reentrant pumping. Likely panics on the first construct/navigate/first-frame;
    only steady-state is safe. Not recommended — the pumping paths are exactly
    the ones that bite.
  - Verify (either): producer constructs on DX12 with our HWND, a navigation
    event fires (log). Frames come in step 3.
  - **Done 2026-05-27 (B, `on_tick`).** Added `AppDriver::on_tick` + xilem
    `with_on_tick` (masonry fork `3756054d`). mere-app's host builds a
    `WebView2CompositionProducer` for the surf tile's URL and `load_url`s it
    (`7493776`). Two fixes the runtime forced: WebView2's WinComp `Compositor`
    needs a `DispatcherQueue` on the UI thread first (host creates + holds a
    `DispatcherQueueController`); single-producer keyed by URL (the surf tile's
    `WidgetId` churns across rebuilds, which would spawn N WebViews). Full nav
    lifecycle logs; the page renders **as a native DirectComposition overlay**
    (see step 3b — the producer attaches its visual to the HWND target).
- **Step 3a — Dx12 frame import. Done 2026-05-27 (`e358353`).**
  `try_acquire_frame` (non-blocking after the first) → import the
  `Dx12SharedTexture` into our wgpu DX12 device via `WgpuTextureImporter`.
  Verified: `imported frame 1379x926 (Bgra8Unorm) gen 1`. Re-import only on
  `resource_is_new` (else the reused texture's shared handle is stale →
  "handle is invalid"); cache the import otherwise.
- **Step 3b — composite + suppress the overlay (next).** Share the cached
  imported `wgpu::Texture` to the compositor (a `Arc<Mutex<Option<Texture>>>`
  cell; `Texture` is `Send`). In the compositor: **blit** it into the tile rect.
  The frame is `Bgra8Unorm`, the target `Rgba8Unorm` — *not* copy-compatible, so
  `copy_texture_to_texture` won't do; a sample-based blit (render pass into the
  target's RENDER_ATTACHMENT, viewport = tile bounds, sampling the imported
  texture) handles both positioning and channel order (sampling normalizes BGRA
  → RGBA). **Suppress the native overlay**: WGC captures via `CreateFromVisual`
  (the visual's *content*, position-independent), so offsetting the producer's
  visual off-screen hides the overlay while capture keeps working — to verify at
  runtime. Result: the page shows only in the composited tile, our scene
  controls layering. Verify: page renders in the tile rect, no overlay over the
  chrome.
- **Step 3b — composite + suppress. Done 2026-05-27 (`5ca7a58`).** Verified on
  DX12: example.com renders inside the tile (no overlay over the chrome) via the
  `BlitPipeline`; offscreen-suppression kept WGC capturing. B complete.
- **Step 4 — input + live frames. Done 2026-05-27 (`8b116fa`).** The
  `SurfaceTileWidget` queues pointer events (down/up/move/wheel) into the shared
  `SurfaceChannel`; the host drains, scales logical→physical, forwards via
  `send_mouse_input` (+ `move_focus` on press). The producer is **sized to the
  tile** (compositor records bounds → host `resize`s), so it renders 1:1 (crisp
  + 1:1 input). Live updates: while a web tile is shown the host
  `request_redraw`s each tick (the WebView produces frames continuously but the
  compositor only blits on render + the loop sleeps when idle). Verified:
  clicking navigates, smooth scrolling.
- **Step 5 — keyboard + DX12-default. Done 2026-05-27 (`69c04f1`).** Verified
  (typed into DuckDuckGo, Enter searched):
  - **Keyboard**: the widget is focusable (`accepts_focus` for web tiles), takes
    focus on click, and `on_text_event` maps ui-events keys → `TileKey`
    (printable via `characters` + best-effort VK; named keys → Windows VK),
    queued on the channel; the host forwards `send_keyboard_input` with modifier
    flags.
  - **`WGPU_BACKEND=dx12`** is now a Windows host default (`main()` sets it if
    unset) — no manual env var; the D3D12-shared-texture import needs DX12.
- **Step 5 — polish.**
  - ~~**Frame-arrival-driven redraw**~~ **Addressed 2026-05-27 (`9797bc8`).**
    Idle-quiet via a mere-side *activity heuristic*, not a cross-crate waker:
    keep redrawing only while there's activity (a new frame arrived or input was
    forwarded — so video/scroll/animation stay smooth, since continuous content
    keeps the loop awake by itself), and after a ~0.5 s grace with no activity on
    a static page, stop requesting redraws and let the loop sleep. Narrow residual
    gap (a static page that *spontaneously* animates after idling won't repaint
    until the next interaction) — a true `FrameArrived → winit` waker would close
    it but needs scrying + xilem plumbing; not worth it for the gap.
  - ~~**Resize tracking** is janky mid-drag~~ **Addressed 2026-05-27
    (`1a0e842` + `e4796c3`).** Two parts: **debounce** the producer resize (only
    after the tile size is stable a few ticks, so a drag doesn't thrash the
    capture pipeline — the cached frame blits scaled meanwhile), then a
    **resync-snap** (one blocking `acquire_full_frame` in on_tick after settle,
    so the tile grabs a fresh new-size frame rather than holding the
    stretched-stale one). **Finding:** true *drag-time* smoothness (content
    tracking the size live) is **producer-capped** — WebView2/WGC restarts
    capture on every resize, so live-resize inherently flickers; the
    debounce+settle-snap is the pragmatic point on the debounce↔throttled↔
    continuous spectrum. Smooth live-resize would need scrying to support
    resize-without-capture-restart.
  - ~~Fuller key coverage~~ **Done 2026-05-27 (`9797bc8`)** — F1–F12 + Insert
    added to the named-key → VK map.
  - **Multi-tile + retire — deferred to verso/workbench (roadmap R2).** Not a
    standalone fix: multi-tile needs a *stable* per-tile identity (forme
    `TileId`), which the fixed `MainView::Surface` tile lacks, and retire-on-close
    needs a close gesture that doesn't exist yet. The current single-producer
    keep-alive across view-switch is correct until then (instant re-show vs the
    ~5 s WebView2 env rebuild). Folds into the `WorkbenchTiling` widget (verso P1).
  - **IME — deferred to its own careful pass.** Enabling `accepts_text_input`
    reroutes Latin input from `Keyboard(Character)` onto the IME session's
    `Ime::Commit`, a platform-nuanced text-path change with double/missed-insert
    regression risk to the *verified* keyboard. Worth doing for CJK/dead-keys,
    but with dedicated runtime iteration — not a tail-of-session bolt-on.

## Boundaries

- `inker`'s `SurfaceProducer` trait emits `NativeTextureHandle` (platform
  handles) — the host would do the cross-API import itself. We **bypass** that
  for the host path and use scrying's `WebView2CompositionProducer` +
  `WgpuTextureImporter` directly (same-backend DX12 import into our device).
  inker's routing (which engine backs a tile) still decides *that a tile is
  scrying-backed*; the realization is host-side.
- One producer per `TileId`; the `SurfaceRegistry`'s value grows from `[u8;4]`
  to a frame handle. The `WorkbenchTiling` widget (verso P1) will host these
  tiles once it lands; for now the single `MainView::Surface` tile drives it.
- No DOM bridge here — that's the [scrying DOM-bridge brief](../../../../serval/docs/2026-05-26_scrying_dom_bridge.md),
  orthogonal, riding the same producer.
