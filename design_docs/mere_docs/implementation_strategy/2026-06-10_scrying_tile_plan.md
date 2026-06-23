# Scrying Tile Plan (flip P4 / integration S6)

**Date**: 2026-06-10
**Status**: X1+ shipped 2026-06-15 (meerkat `0adca6e` single-tile, `06b6ac7`
multi-tile) via the ad-hoc `compat_pins` path; folding it into `inker::routing`
is the inker-picker plan's Phase 0. *(Status corrected 2026-06-23; the body
below predates the implementation.)*
**Scope**: Land external web content in meerkat: a node routed to `scrying.web`
renders through the system WebView (WebView2 first), its GPU frames imported
into the host's wgpu device and composited at the tile/card rect via
netrender's external-texture pass. This is the integration plan's S6 and the
archived flip plan's P4 — the one flip phase that never shipped.
**Related**: [integration plan](2026-06-02_modular_integration_plan.md) S6
(this doc is its detailed elaboration);
[verso charter](../../verso_docs/technical_architecture/2026-06-10_compatibility_view_charter.md)
(sequencing step 1: this plan lands first, no verso needed; the flip carriers
come later);
the archived Masonry-era
[scrying integration plan](../../archive_docs/2026-06-09_pivot_superseded/2026-05-27_scrying_integration_plan.md)
(prior art; its host-factory sketch became the shipped `ProducerFactory` seam).

---

## What already exists (verified 2026-06-10; the gap is host wiring only)

- **`scrying` (repos/wgpu-scry)** — mature standalone lib: platform producers
  for WebView2 composition (Windows), WKWebView (macOS), WebKitGTK/webkit6 and
  WPE (Linux), each with capture, input, IME, cookies, navigation, downloads,
  script messaging; `native_frame` does zero-copy GPU import
  (`WebSurfaceFrame::Native(NativeFrame::Dx12SharedTexture)` on Windows via
  DXGI shared handle, dmabuf on Linux, IOSurface on macOS) with keyed-mutex /
  implicit sync today and an explicit-fence path scaffolded (README "Future
  work: explicit fence sync").
- **`scrying-engine` (crates/inker/engines/scrying-engine)** — complete and
  test-covered: `ScryingTileEngine: inker::SurfaceEngine` registered as
  `scrying.web`, `ScryingProducer` adapting `scrying::WebSurfaceProducer` onto
  `inker::SurfaceProducer` (resize, offset, acquire_frame, navigate, back/
  forward, mouse/pointer/keyboard input, focus, nav/cursor/web-message polling,
  settings, PNG snapshot), and the `ProducerFactory` host hook naming exactly
  the resources the host must supply: parent HWND / NSView, the wgpu device
  (or fence share-handle), per-platform composition plumbing
  (engine.rs:18-37). `SurfaceSpawnRequest` already carries
  `EngineProfileBinding { user_data_dir }` + optional `fence_handle`.
- **inker routing** — `ENGINE_SCRYING_WEB = "scrying.web"`, opt-in per tile via
  `EngineRouteRequest::pinned_engine` (routing.rs:11-24);
  `SurfaceContractMode::CompositedTexture`. graph-kernel's `Node` already
  carries the per-node compatibility-mode toggle field (node.rs:120) — the
  graph-truth hook for the pin.
- **netrender** — `compose_external_texture` / `ExternalTexturePlacement` with
  `scene_op_boundary` ordering, shipped and exercised by meerkat's ~12 actor-
  texture call sites in render.rs.
- **Zero consumers**: nothing constructs `ScryingTileEngine` or implements
  `ProducerFactory` anywhere in mere. That is the entire gap.

## The architectural asymmetry (why this is not "one more content actor")

Constellation content actors render `netrender::Scene`s off the UI thread;
the host rasterizes scene → texture, cached by `scene_version`
(constellation.rs). A scrying tile inverts every part of that:

- the WebView renders *itself*; there is no Scene and no rasterize step — the
  producer yields a GPU texture the host imports;
- the producer is **UI-thread-resident** (WebView2's composition controller is
  COM/HWND-bound; producer methods are sync), so scrying activations live on
  the UI thread and are driven in the frame loop, not on pool workers;
- input is **forwarded into** the producer (send_mouse/keyboard, move_focus),
  not dispatched in serval — the tile is a black box with its own cursor and
  focus;
- frames arrive on the WebView's own schedule; v1 acquires per redraw while
  the tile is visible (continuous redraw), with a frame-arrival wake as a
  refinement.

So: a small `ScryingHost` (new meerkat module, mind the 600-LOC ceiling)
parallel to — not inside — the Constellation. Same vocabulary (needed-set
reconcile, warm tabs, cap, reap, respawn-on-fault), different storage:
`HashMap<GraphMemberId, ScryingActivation { producer, imported_texture,
last_frame_meta, .. }>`. WebView memory cost is far above an actor's, so it
gets its own, smaller, configurable cap (separate from `DEFAULT_TAB_CAP`).

## Phases (done-conditions, not dates)

### X1 — One live tile on Windows

Implement `ProducerFactory` for Windows in meerkat (parent HWND from winit's
`RawWindowHandle`, device/queue from the `SurfaceHost`, `user_data_dir` under
the session dir per the engine-profile-boundary plan), register
`ScryingTileEngine` in the surface-engine registry, and for a node pinned to
`scrying.web`: spawn on focus, `acquire_frame` per redraw, import the
`Dx12SharedTexture` frame into wgpu (scrying's `native_frame` +
`DxgiSharedHandleBridge`), composite at the focused card rect via
`compose_external_texture`. Resize follows the card rect. **Done when** a
real HTTPS site renders live inside a meerkat card on the Windows laptop.

### X2 — Input, navigation, chrome integration

Forward mouse/pointer/keyboard/wheel to the producer when the pointer is over
the scrying rect (the host already routes per-region; this is one more region
with a real consumer). Omnibar drives `load_url` (the non-blocking
concrete-producer path via `inner_mut()`, per the producer.rs blocking
caveat — not the 15s-timeout trait method on the UI thread); back/forward
buttons map to `go_back`/`go_forward` + `can_go_*`; `poll_navigation_event`
updates the omnibar + node lineage; `poll_cursor_shape` sets the winit
cursor; focus hand-off via `move_focus` on click/Tab into the tile. **Done
when** you can log into a real site in a scrying tile using only meerkat
chrome.

### X3 — Lifecycle + the pin surface

`ScryingHost` reconcile semantics aligned with the constellation (warm on
blur, cap + LRU eviction, reap on close, fault respawn with storm cap); the
per-node engine pin exposed in UI (the node.rs compatibility toggle +
`pinned_engine` route override; per the configurability rule this is a
visible per-node setting, with the auto-fallback rule from routing.rs:19-21
as a later refinement). Tile teardown verified leak-free (producer drop +
imported texture release). **Done when** pin/unpin per node works from the
inspector or card chrome and a closed tile releases its WebView.

### X4 — Other platforms

macOS (WKWebView producer) and Linux (webkit6 or WPE, per the wgpu-scry
parity matrix) factories behind `cfg(target_os)`. All four test machines can
validate locally. **Done when** the X1 done-condition passes on iMac and one
Linux box.

### Later (not this plan)

- The verso flip carriers (serval → scrying with state carry) — charter step
  3; this plan deliberately lands the scrying lane *without* verso.
- **The external-texture element view in xilem-serval** (the audit's "one
  missing primitive"): tile content, cards, and scrying textures placeable as
  DOM children at DOM-computed rects, retiring meerkat's hand-summed rect
  compositing. The *rendering* version is one medium PR (serval already
  lowers `DrawExternalTexture`; netrender's compose pass works). The
  *interactive* version — what this lane wants under the orrery camera —
  additionally needs transform-aware hit-testing and the pointer propagation
  cell, both tracked in the
  [host cheap-path plan](../../archive_docs/2026-06-15_completed_plans/2026-06-10_host_cheap_path_plan.md) C6 *(archived)*. Not one PR.
- Explicit fence sync (the scrying README's ~150-250-line wgpu-hal path) if
  keyed-mutex/implicit sync shows artifacts under load.
- Frame-arrival wake instead of acquire-per-redraw (battery/perf refinement).
- `content_generation` population for netrender's tile-cache keying once a
  sampling-source use appears (paint_list_api items.rs:332-339).

## Constraints and notes

- **Blocking nav caveat**: `SurfaceProducer::navigate_to_url` is
  blocking-with-timeout and panics from event-loop callbacks on macOS
  (producer.rs:9-14). The host must use the concrete producer's non-blocking
  `load_url` via `inner_mut()`. X2 owns this.
- **Session-substrate split is by design**: the WebView brings its own
  network stack, cookie jar (`user_data_dir` profile), and cache; netfetcher/
  eidetic are not in the loop for scrying tiles. The verso charter's §5
  ceiling documents this; no sync between the worlds in this plan.
- **Stale pointer to fix in passing**: scrying-engine's lib.rs cites
  `2026-05-11_scrying_web_tile_plan.md` (archived, gpui-era); repoint to this
  plan when X1 lands.
- The compose path's ordered-interleave cost (full tail re-render per
  boundary crossing, netrender renderer/mod.rs) is acceptable at v1 tile
  counts; default topmost-overlay ordering is fine for the focused card.

## Findings

- 2026-06-10 scoping pass: all three layers below the host are built and
  tested (scrying producers, the engine/adapter/factory seam, the netrender
  compose pass). The audit's "P4 unbuilt" is precisely a host-wiring gap, the
  same demo-host-vs-product-host shape as IME.
- **The engine-registry seam cannot carry the WebView2 frame protocol.** The
  type-erased `inker::SurfaceProducer` lane lossy-maps frames to a raw handle
  and drops `resource_is_new`/`shared_handle` — but the producer's contract is
  handle-handoff (import once on a fresh allocation, close the NT handle, then
  keep sampling the same imported texture while `CopyResource` overwrites it).
  So the host pool binds `PlatformWebSurfaceProducer` concretely and uses
  `try_acquire_frame` (non-blocking). X2/X3 should either extend
  `inker::SurfaceFrame` to express the handoff or accept that frame transport
  is host-concrete per platform while the inker seam carries nav/input.

## Progress

- **2026-06-10** — Plan created from the post-audit scoping pass (scrying-engine,
  constellation, wgpu-scry README, integration plan S6). No code yet. X1 is
  the entry point.

- **2026-06-10** — **X1 implemented (code-complete; on-screen check pending).**
  - New `meerkat/src/scrying_host.rs`: `ScryingHost` (session-local compat
    pins + the Windows pool). Windows pool spawns a
    `WebView2CompositionProducer` per pinned member (HWND from the winit
    window, profile at `<session_dir>/scrying/profile`), drives
    resize / non-blocking `load_url` / `try_acquire_frame` per redraw, imports
    fresh allocations via `WgpuTextureImporter` (closing the handed-off NT
    handle), and keeps sampling the imported texture on reused frames. Spawn
    failures are recorded once (no respawn storm); unpin / multi-graph switch
    reap.
  - `serval-winit-host` gained a `queue()` accessor beside `device()`.
  - New palette command `Compatibility view (system WebView, focused node)`
    (host action). Pinning opens the live card and reaps the node's content
    actor; the render path routes a pinned member's live card to
    `scrying.drive` + a `compose_external_texture` at the card rect (no UV
    window — the WebView scrolls itself), with continuous redraw while
    visible. The pin is session-local host state for X1; the durable
    `node.compat_mode` field takes over in X3.
  - meerkat deps: `[target.'cfg(windows)']` `scrying` (path) + `dpi`.
  - Verified: `cargo build -p meerkat` links clean; meerkat tests 44 + 63
    green (palette row count updated). The X1 done-condition (a real HTTPS
    site live in a card) needs the on-screen run: focus a node with an
    `https://` URL, palette → "Compatibility view", expect the WebView frame
    in the live card. First-frame latency includes the WebView2 environment
    boot.
  - Known X1 edges, deliberately deferred: the card's close (X) button leaves
    the WebView warm (reaped only on unpin/graph-switch; X3 lifecycle); no
    input forwarding yet (X2); `last_error` is recorded but the card shows no
    error placeholder yet (X2); capture continues while the card is hidden
    (frame-arrival wake is a Later item).

- **2026-06-10** — **X2 input core implemented (code-complete; on-screen check
  pending).** The tile now takes mouse, wheel, and keyboard.
  - `ScryingHost` gained host-neutral `forward_mouse` / `forward_wheel` /
    `forward_key` / `focus_tile` (+ `MouseBtn` / `MousePress` / `KeyMods`); every
    scrying input type (`MouseInput`, `KeyboardInput`, `KeyModifierFlags`,
    `FocusReason`, the button/kind enums) stays sealed inside the `cfg(windows)`
    pool, so the cross-platform host never names them. The pool builds the scrying
    events and calls the concrete producer's `send_mouse_input` /
    `send_keyboard_input` / `move_focus`.
  - Render records the focused tile's window rect in `App.scrying_rect`;
    `scrying_at(x, y)` returns the member + **tile-local** coords. Routing: a
    press/release on the tile forwards the button and hands the tile the keyboard
    (`scrying_input_focus`); cursor-move over it forwards a move and does **not**
    pan the orrery; a wheel forwards 120-per-notch; while the tile holds focus,
    keys forward to its WebView (winit→Win32 VK map + `event.text`), and
    **Escape** releases focus to the chrome. Unpin / multi-graph switch clear the
    focus + rect.
  - Builds clean on Windows (the `cfg(windows)` producer calls type-check against
    the real crate); meerkat 44 lib + 63 bin green. **Done-condition pending the
    on-screen run**: with X1's live tile up, you should be able to click fields,
    type, and scroll inside the WebView card.
  - Remaining X2 (chrome integration, not wired): omnibar `load_url` via the
    concrete producer's non-blocking `inner_mut()`; back/forward + `can_go_*`;
    `poll_navigation_event` → omnibar + node lineage; `poll_cursor_shape` → winit
    cursor; Tab-into-tile focus (mouse focus hand-off is done).

- **2026-06-11** — **X1/X2 verified on-screen (Windows laptop); three fixes + the
  display model corrected.** Load, navigate, scroll, click, and keyboard input all
  work in a live tile (typed into Google AI Mode through the embedded WebView and
  got a reply), and deselect dismisses it. Fixes, in order found:
  - **DispatcherQueue (scrying repo).** First spawn panicked: `Compositor::new` needs
    a `DispatcherQueue` on the UI thread, which winit does not create.
    `CompositionRoot::new` now ensures one (idempotent; honors a consumer's own),
    retained thread-locally. Consumers need no setup. (`scrying` commit `3421982`.)
  - **Display model: capture → visual hosting.** The X1 capture-into-texture
    composite never delivered visible pixels; what showed was the producer's own
    HWND-parented composition visual, at the window origin (offset 0,0), on top of
    the swapchain. Parking it off-screen blanked it (DWM culls an off-screen visual,
    so capture dies). So meerkat switched to the demo's model: **position the WebView
    visual at the card's screen origin each frame** (origin threaded through
    `ScryingHost::drive`). The visual displays directly; the texture composite is now
    vestigial.
  - **Dismiss.** `ScryingHost::hide_all()` parks every tile's visual off-screen at the
    top of each frame; the focused card re-shows its own by positioning it. A
    deselected / unpinned tile stops displaying instead of freezing in place.
  - **Open refinements (not yet done), all from this session:**
    - **One tile at a time per window.** A second pinned node fails to spawn with
      `DCOMPOSITION_ERROR_WINDOW_ALREADY_COMPOSED` — Windows allows one
      `DesktopWindowTarget` per HWND, and meerkat builds a fresh `CompositionRoot`
      per producer. Multi-tile needs scrying's `new_attached` (share one root); X1's
      "focused card only" scope can instead reap-on-new-pin.
    - **The × is occluded.** The host's close button paints in the swapchain, under
      the visual (DWM composites the visual above), so it can't be clicked. Needs a
      close control *outside* the WebView rect (a tab handle above the card) or
      reap-on-deselect. Deselect already dismisses (hide); the × is only for closing
      without deselecting.
    - **Scrollbars** want overlay / auto-hide (a WebView2 setting in scrying).
  - meerkat 44 lib + 65 bin green throughout.
