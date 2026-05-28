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

`scrying`'s `WebSurfaceProducer` (platform-aliased `PlatformWebSurfaceProducer`
= `WebView2CompositionProducer` on Windows, `WkWebViewProducer` on macOS,
`WebKitGtk`/`Wpe` on Linux) yields a `WebSurfaceFrame`:

- **`CpuRgba { pixels: image::RgbaImage, .. }`** — CPU pixels. Upload via
  `queue.write_texture` (the stub's exact machinery), then
  `copy_texture_to_texture` into the target. **No GPU interop; works on any
  wgpu backend** (we're on Vulkan). **Tier 1.**
- **`Native(NativeFrame)`** → `scrying::import_native_frame(..) ->
  ImportedTexture { texture: wgpu::Texture }` — scrying imports the platform
  texture into *our* device; we `copy_texture_to_texture` from it. Zero-copy,
  but gated by `WebSurfaceCapabilities::imported_texture`, which depends on the
  host backend pairing (likely `Unsupported` on Vulkan↔D3D12). **Tier 2.**

Setup is clean: `HostWgpuContext::new(device, queue)` +
`WebSurfaceCapabilities::probe(Some(&host))`.

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
3. **Frame pump, event-loop-safe.** `WebSurfaceProducer::acquire_frame` is
   **blocking** with an explicit re-entrancy hazard; the concrete producers
   expose a **non-blocking** fast-path. WebView2 is COM/STA — the producer must
   live and be pumped on the **main (event-loop) thread**. So: poll non-blocking
   on the main thread, stash the latest frame in the registry, `request_redraw`
   to keep frames flowing; the compositor uploads the latest.
4. **Platform variance.** WebView2 (Windows) first; the `PlatformWebSurfaceProducer`
   alias + `capabilities.probe` abstract the rest. macOS/Linux are follow-ups.

## Sequencing (each step runtime-verified; none verifies headless)

- **Step 1 — window-handle seam.** Add `pub window: &winit::window::Window` to
  `ExternalCompositeCtx` (masonry `render()` sets it from `window.handle()`). In
  mere-app's compositor closure, read `ctx.window.window_handle()` (via
  `raw-window-handle`) to get the Win32 HWND. Verify: log the captured HWND.
- **Step 2 — producer holder + create + navigate.** A `SurfaceProducers` holder
  (`TileId → PlatformWebSurfaceProducer`) in `AppState`. On showing the surf
  tile (or a URL), lazily create the producer with the HWND + `HostWgpuContext`,
  `navigate` to a real URL. Verify: producer constructs, navigation fires (log).
- **Step 3 — CpuRgba pump + composite.** Poll the producer non-blocking each
  frame (main thread), stash the latest `CpuRgba` in the registry (value type
  `[u8;4]` → `Arc<RgbaImage>` / a frame buffer), `request_redraw`; the compositor
  uploads the pixels into the layer bounds. Verify: a real web page renders in
  the surf tile.
- **Step 4 — input routing.** Translate pointer/keyboard from the `SurfaceTile`
  widget into the producer (`inker`'s input vocabulary already exists). Verify:
  links click, scrolling works.
- **Step 5 — Tier 2 zero-copy** (optional, perf): if `imported_texture` is
  `Supported`, switch to `Native` → `import_native_frame` →
  `copy_texture_to_texture`. Verify: no CPU readback, same image.

## Boundaries

- `inker`'s `SurfaceProducer` trait emits `NativeTextureHandle` (platform
  handles) — the host would do the cross-API import itself. We **bypass** that
  for the host path and use scrying's `WebSurfaceProducer` directly, which
  imports into our device (Tier 2) or hands CPU pixels (Tier 1). inker's routing
  (which engine backs a tile) still decides *that a tile is scrying-backed*; the
  realization is host-side.
- One producer per `TileId`; the `SurfaceRegistry`'s value grows from `[u8;4]`
  to a frame handle. The `WorkbenchTiling` widget (verso P1) will host these
  tiles once it lands; for now the single `MainView::Surface` tile drives it.
- No DOM bridge here — that's the [scrying DOM-bridge brief](../../../../serval/docs/2026-05-26_scrying_dom_bridge.md),
  orthogonal, riding the same producer.
