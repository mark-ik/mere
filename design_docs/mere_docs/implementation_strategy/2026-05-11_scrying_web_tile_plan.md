# `scrying.web` tile integration plan

**Date**: 2026-05-11
**Status**: Implementation plan
**Scope**: Wire the `scrying.web` engine end-to-end through mere-host
— from inker route decision through tile lifecycle to gpui-composited
wgpu texture. Follows the [engine-peers + scrying-as-library brief](../research/2026-05-11_engine_peers_and_scrying_library_brief.md);
the brief decided the architecture, this plan decomposes the code.

**Related**:

- [`../research/2026-05-11_engine_peers_and_scrying_library_brief.md`](../research/2026-05-11_engine_peers_and_scrying_library_brief.md)
  — the architectural decision this plan implements.
- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md)
  — multiplexer framing; §5.4 (engine profile / UDF) is what `scrying.web`
  binds against for cookie continuity.
- `repos/scrying/design_docs/2026-05-11_windows_webview2_target.md`
  — the Windows producer's audited capability state (most of W1–W5 ✅).
- `repos/scrying/scrying/README.md` — producer trait surface.

---

## 1. The shape mismatch

mere-host's current tile dispatch is **document-engine only**:

```rust
// inker
pub trait Engine {
    fn render(&self, input: &EngineInput) -> Result<EngineDocument, EngineError>;
}

// mere-host/src/loader.rs
pub fn load(...) -> Result<EngineDocument, EngineError> {
    let raw = fetch(address)?;
    let decision = policy.route_filtered(&request, ...);
    let input = EngineInput::new(address, raw);
    registry.dispatch(&decision, &input)   // → EngineDocument
}

// workbench/verso/tile-state/src/tiles.rs
pub struct TileState {
    pub history: Vec<HistoryEntry>,
    pub history_cursor: usize,
    pub documents: HashMap<String, EngineDocument>,
}
```

Every tile is a viewer for an `EngineDocument`. Scrying tiles don't
fit that shape:

- **No fetch**: scrying's underlying WebView (`WebView2` /
  `WKWebView` / `WebKitGTK`) does its own HTTP. Mere-host's
  `loader::fetch` (which only handles `file://` and `mere://` today
  anyway) is bypassed entirely.
- **No `EngineDocument`**: scrying produces a `WryWebSurfaceProducer`
  instance + a frame stream + a wgpu texture per frame. Documents
  are nowhere in the path.
- **Long-lived producer**: scrying's producer owns the live WebView
  control. It outlives any single "fetch" — navigation, reload,
  back/forward all happen on the same producer. Tile lifecycle ≈
  producer lifecycle.
- **Input flow inverted**: document tiles get input through the
  host's normal event routing (clicks → hit-test → link click).
  Scrying tiles forward input *into* the producer via
  `send_mouse_input` / `send_pointer_input` / `send_keyboard_input`
  (the latter via CDP, per the Windows target doc).

The engine trait can't bridge this without losing its meaning. We
need a **second dispatch shape** for surface-producing engines.

## 2. Architecture decision: parallel dispatch

Two options were considered:

**Option A — unified `Engine` trait with output enum.**

```rust
pub enum EngineOutput {
    Document(EngineDocument),
    Surface(Box<dyn SurfaceProducer>),
}
pub trait Engine {
    fn render(&self, input: &EngineInput) -> Result<EngineOutput, EngineError>;
}
```

Pros: one trait, one registry, one dispatch path.
Cons: every existing engine impl (12 nematic engines + serval +
graphshell internal) has to handle the enum even though they only
ever return `Document`. The trait surface grows for downstream
consumers (host needs to match on the enum at every call site).
Bigger churn, leaks the surface-engine concept into the document path.

**Option B — separate `SurfaceEngine` trait + parallel dispatch.**

```rust
// inker::engine (unchanged)
pub trait Engine {
    fn render(&self, input: &EngineInput) -> Result<EngineDocument, EngineError>;
}

// inker::surface_engine (new)
pub trait SurfaceEngine {
    fn spawn(&self, request: &SurfaceSpawnRequest) -> Result<Box<dyn SurfaceProducer>, EngineError>;
}
```

The router still produces one `EngineRouteDecision`; the host
decides which registry to dispatch through based on whether the
chosen engine ID is registered as document-engine or surface-engine.
Both registries coexist; the routing rule and the engine kind don't
have to align — `scrying.web` is just an engine ID that resolves to
a surface registry, while `nematic.markdown` resolves to the document
registry.

Pros: zero churn to existing engines. Surface concerns stay in their
own contract surface. Host's dispatch path is a small match on
"which registry has this engine."
Cons: two registries to maintain. Slightly more inker surface.

**Decision: Option B.** The pro of zero churn to 13+ existing engine
impls outweighs the con of one extra registry. It's also more
honest about what's happening — document engines and surface engines
are doing structurally different work, and treating them the same is
the misleading framing.

The `SurfaceProducer` trait is **not** scrying-specific — it's
inker's surface-side equivalent of `Engine`. The trait shape mirrors
scrying's `WryWebSurfaceProducer` closely (frame acquisition,
navigation, history, input, settings, snapshots) so that the
scrying impl is a thin pass-through. Future surface-producing
engines (e.g., a hypothetical `wry.web` overlay engine, or a future
custom-renderer engine) implement the same trait differently.

## 3. Crate placement

```text
inker (new module: surface_engine)
  - SurfaceEngine trait
  - SurfaceProducer trait
  - SurfaceSpawnRequest / SurfaceFrame / surface event vocabulary
  - SurfaceEngineRegistry (parallel to EngineRegistry)

workbench/verso/tile-state (new crate: verso-tile-state)
  - SurfaceTileState (analog of TileState but holds a producer +
    last-acquired frame, not a document cache)
  - lifecycle helpers: spawn / navigate / step (per-frame poll) /
    teardown

graphshell/shell/session-runtime
  - session manifest, engine profile paths, view-intent sidecars, and
    worker declarations
  - re-exports tile state for compatibility but does not own it

mere-host (product binary)
  - host_navigation, panes, etc.: new dispatch branch on whether
    the chosen engine is in the document or surface registry
  - rendering: new gpui-side path that imports the producer's wgpu
    texture into a gpui-renderable surface

scrying-engine (workbench/verso crate)
  - implements `inker::SurfaceEngine` for the scrying-driven path
  - depends on scrying + inker
  - host pulls this in to register the engine

graphshell/shell/system/control-plane
  - owns the typed action bus and gates; surface navigation commands
    route through it rather than through tile state directly
```

2026-05-19 topology correction: do **not** put the scrying surface
implementation or tile state back into session runtime. The runtime
owns manifests and session sidecars. Tile lifecycle belongs under
`workbench/verso/tile-state`; the scrying surface engine implementation
belongs under `workbench/verso/scrying-engine` once it becomes more than
a registration shim.

## 4. The `SurfaceProducer` trait shape

Adapted from scrying's `WryWebSurfaceProducer` (renaming to drop the
misleading `Wry` prefix; scrying's own types may also drift this
direction over time, but inker doesn't need to wait):

```rust
// illustrative — exact shape lands with implementation
pub trait SurfaceProducer: Send {
    // Layout
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError>;
    fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError>;

    // Frame acquisition
    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError>;

    // Navigation
    fn navigate_to_url(&mut self, url: &str) -> Result<(), SurfaceError>;
    fn navigate_to_string(&mut self, html: &str) -> Result<(), SurfaceError>;
    fn reload(&mut self) -> Result<(), SurfaceError>;
    fn stop(&mut self) -> Result<(), SurfaceError>;
    fn go_back(&mut self) -> Result<(), SurfaceError>;
    fn go_forward(&mut self) -> Result<(), SurfaceError>;
    fn can_go_back(&self) -> bool;
    fn can_go_forward(&self) -> bool;

    // Input
    fn send_mouse_input(&mut self, ev: MouseEvent) -> Result<(), SurfaceError>;
    fn send_pointer_input(&mut self, ev: PointerEvent) -> Result<(), SurfaceError>;
    fn send_keyboard_input(&mut self, ev: KeyboardEvent) -> Result<(), SurfaceError>;
    fn move_focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError>;

    // Events
    fn poll_navigation_event(&mut self) -> Option<NavigationEvent>;
    fn poll_cursor_shape(&mut self) -> Option<CursorShape>;
    fn poll_web_message(&mut self) -> Option<WebMessage>;

    // Settings
    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError>;

    // Snapshot
    fn capture_snapshot_png(&mut self) -> Result<Vec<u8>, SurfaceError>;
}
```

The `SurfaceFrame` carries a wgpu-importable native texture handle
(matching scrying's `NativeFrame::Dx12SharedTexture` shape) plus
sync metadata (the explicit D3D12 fence from scrying's
`SyncMechanism::ExplicitFence`).

inker doesn't need every method scrying's trait exposes — just the
ones a tile lifecycle interacts with. Downloads, auth, permissions,
new-window/popup interception, virtual-host routing, find-in-page,
PDF, print, context menus, and drag-and-drop are *also* shipped on
scrying's Windows producer (per the audit doc), but they don't
*all* need to be on `SurfaceProducer` v1 — they can ship as
extension traits or as concrete producer-specific methods that the
host opts into. Trait pollution is the failure mode; keep `v1`
minimal.

## 5. UDF binding

Per the engine-peers brief: `serval.web` and `scrying.web` bind the
same persona-scoped UDF. Implementation:

```rust
pub struct SurfaceSpawnRequest {
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub engine_profile: EngineProfileBinding,  // persona/session/graph
    pub fence_handle: Option<D3d12SharedHandle>,
}
```

`EngineProfileBinding` resolves to a UDF path the producer points
at on construction. scrying's `WebView2CompositionConfig::user_data_dir`
field is the Windows binding point; macOS WKWebView binds via
`WKWebsiteDataStore`. The host passes the resolved UDF path; the
scrying impl plumbs it to the right producer config.

## 6. Slice sequence

### Slice 1 — `SurfaceEngine` + `SurfaceProducer` traits in inker

- New module `inker::surface_engine` with trait definitions,
  request/response types, and `SurfaceEngineRegistry`.
- No impls yet; just the contract.
- Tests: the registry register/lookup/contains shape; no live
  producer.
- ~200 LOC, no new deps. Pure inker work.

### Slice 2 — `scrying-engine` impl in `workbench/verso`

- Add `scrying = { path = "../../scrying/scrying" }` to
  the verso scrying engine crate.
- New module/crate implementing
  `inker::SurfaceEngine` and `SurfaceProducer`.
- Windows path first (it's the most mature per the audit doc).
  macOS path follows; Linux stays skeletal until scrying's Linux
  producer matures.
- UDF binding: pass the persona-scoped UDF path through to scrying's
  `WebView2CompositionConfig::user_data_dir` (Windows) /
  `WkWebViewConfig`'s data-store binding (macOS).
- Tests: lifecycle smoke — spawn against a known URL, acquire a
  frame, teardown. Maps to scrying's `--scripted` /
  `--browser-test` already-validated paths.
- ~300 LOC, drops the scrying dep into the verso scrying engine crate.

### Slice 3 — Surface tile state + lifecycle in `workbench/verso/tile-state`

- `verso-tile-state::surface_tile` with `SurfaceTileState`
  analog of `TileState`. Holds a `Box<dyn SurfaceProducer>` plus the
  last-acquired `SurfaceFrame`, plus the within-tile navigation
  history (still relevant for back/forward — the producer has its
  own history but mere's tile-strip switcher needs to know URLs
  too).
- Lifecycle helpers: `spawn`, `navigate`, `step` (called per host
  tick to poll for new frames + events), `teardown`.
- Surface-tile vs document-tile distinction made at the `TileManager`
  level — tiles get one tile-state flavor or the other based on the
  engine kind chosen for the anchor URL.
- ~200 LOC.

### Slice 4 — gpui rendering path for `SurfaceFrame`

- gpui needs to composite an external wgpu texture into its render
  tree. gpui (Glass-HQ fork) likely already has external-texture
  support since serval-shaped tiles will need the same thing.
  Verify in the host's existing code; reuse if so.
- Per-frame: poll the producer for a new frame; if present, hand
  the texture to gpui for the surface tile's rect; pass through the
  fence-sync handle.
- ~100 LOC if gpui has external-texture composition; more if a new
  gpui surface type is needed (out of scope for this plan).

### Slice 5 — Dispatch routing in `mere-host`

- Host gains a small dispatcher in `loader.rs` (or a sibling
  `surface_loader.rs`): given an `EngineRouteDecision`, choose
  document path vs surface path based on whether the engine ID is
  registered as document or surface.
- Document path stays exactly as it is.
- Surface path constructs a `SurfaceSpawnRequest` from the address +
  resolved UDF binding, dispatches through `SurfaceEngineRegistry`,
  hands the producer to the new surface-tile state.
- Inker rule for `scrying.web` is opt-in (no default-policy entry),
  so the routing decision only resolves to `scrying.web` when a
  user pins it or a per-host override does.
- ~150 LOC.

### Slice 6 — Input wiring

- gpui mouse/keyboard events hitting a surface tile's rect route to
  `SurfaceProducer::send_mouse_input` etc. — instead of the orrery /
  workbench / document hit-test path.
- Windows: keyboard goes through scrying's CDP bridge per the
  audit doc. Mere-host doesn't need to know that; just calls
  `send_keyboard_input` and lets scrying handle the bridge.
- IME bridge (Windows): scrying emits `TextInputFocused` /
  `TextInputChanged` / `TextInputBlurred` events; mere-host
  consumes them to set winit's IME candidate area. The audit doc
  describes the existing demo-win bridge — adapt for gpui.
- ~150 LOC for first cut (mouse + keyboard); IME bridge can defer
  to slice 7.

### Slice 7 — Navigation surface

- Omnibar submit on a surface-tile-pinned URL routes through
  `SurfaceProducer::navigate_to_url`.
- Back/forward operate on the producer's history (`go_back`,
  `go_forward`, `can_go_back`, `can_go_forward`).
- Mere-host's tile-history is still recorded for the tile-strip
  switcher, but driven by `NavigationEvent::Completed` from the
  producer.
- Reload calls `SurfaceProducer::reload`; mere's "reload" gesture
  just funnels into the right call.
- ~100 LOC.

### Slice 8 — End-to-end smoke

- Add a known-Servo-breaking URL to a manual-pin path.
- Verify: pinning to `scrying.web`, the tile renders, scrolls,
  accepts clicks, accepts text input, navigates, and cookies
  persist across producer recreations (binding the same UDF).

Total: ~1000–1500 LOC across 8 slices. Larger than the brief's
"low hundreds of LOC" estimate because the brief assumed a unified
trait; the parallel-trait choice is more code but lower-risk.

## 7. What this plan doesn't decide

- **`wry.web` (overlay-based)**: separate engine, separate impl.
  Lives or doesn't on its own merits. Doesn't share code with
  `scrying.web`.
- **Auto-fallback heuristic** (serval failure → propose
  `scrying.web`): the engine-peers brief flagged it as a follow-up.
  Lands after manual pin works.
- **Cookie/UDF persona resolution**: this plan assumes a working
  `EngineProfileBinding → UDF path` resolver. That's a host-config
  surface in the multiplexer framing brief §5.4; if it's not wired
  yet, this plan's slice 2 needs to point at a fallback path.
- **`TileSnapshot` for engine-flip continuity**: deferred to v2 per
  the engine-peers brief. v1 of `scrying.web` is full-reload on
  engine flip.
- **`wry.web` overlay tile**: structurally analogous module, but
  the overlay composition model is different enough that it warrants
  its own small plan when its time comes.

## 8. Risks / pitfalls

- **gpui external-texture support**: if gpui doesn't yet composite
  external wgpu textures, slice 4 grows substantially. Worth
  verifying before slice 2 lands so the surface-producer trait
  surface doesn't assume something gpui can't consume.
- **Fence sync**: scrying's Windows producer supports explicit D3D12
  fence sync (`with_fence_shared_handle`); the host needs to mint
  the fence and hand the share-handle to scrying at spawn time.
  Missing this means falling back to the barrier/cache path, which
  works but introduces extra latency.
- **`SurfaceProducer` v1 trait churn**: scrying's API is rich
  (downloads, auth, permissions, etc.). Keep v1 minimal; add to
  the trait only as the host needs the methods. Premature trait
  expansion is the failure mode.
- **CompositionController vs Window-to-Visual**: scrying's Windows
  target accepts only pure CompositionController as the no-overlay
  path. The IME bridge requires the CompositionController shape.
  Mere-host's slice 4 should not assume Window-to-Visual — track
  whatever scrying ships as the accepted path.
- **macOS parity**: the audit doc focuses on Windows. macOS slice
  may discover gaps in scrying's WKWebView producer (per the parity
  checklist mentioned in the doc). Don't block Windows on macOS;
  ship Windows first.

## 9. Recommended landing sequence

1. **Slice 1** alone first — `SurfaceEngine` / `SurfaceProducer`
   traits in inker. Low-risk; doesn't depend on anything else;
   establishes the contract Mark can sanity-check before deeper
   work.
2. **Slice 2** — scrying engine impl. Pulls scrying dep into
   `workbench/verso/scrying-engine`; concrete Windows-first impl.
3. **Slices 3–6** together as a working tile. Each individually is
   ~100–200 LOC but they have to land together for any tile to
   render.
4. **Slice 7** (navigation surface) after the tile renders.
5. **Slice 8** is the validation, not new code.

Each slice should land with its own commit and tests where
feasible. Slices 4 (gpui rendering) and 6 (input wiring) are the
ones most likely to need iterative shaping — bound them with a
manual smoke before claiming done.
