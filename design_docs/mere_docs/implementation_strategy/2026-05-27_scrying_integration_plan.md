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
- **Step 3 — frame pump + zero-copy import + composite.** In the tick hook
  (option B): `try_acquire_frame()`; on a frame, import the `Dx12SharedTexture`
  via the cached `WgpuTextureImporter` (re-import only when `resource_is_new`),
  store the imported `wgpu::Texture` in the shared cell, `request_redraw`. In the
  compositor (inside render): `copy_texture_to_texture` the cell's texture (BGRA
  → target) into the layer bounds. Verify: a real web page renders in the surf
  tile.
- **Step 4 — input routing.** Translate pointer/keyboard from the `SurfaceTile`
  widget (it already takes pointer events) into the producer's `send_mouse_input`
  / `send_keyboard_input`. Verify: links click, scrolling works.
- **Step 5 — lifecycle polish.** Resize → `producer.resize`; offset tracking;
  retire on tile close; multi-tile via the `WorkbenchTiling` widget (verso P1).

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
