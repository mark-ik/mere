# Surface-Engine Contract Fold Plan

Fold every surface engine (scry / weld / graft, and future ones) onto the neutral
`inker` `SurfaceEngine` registry path, eliminating scry's concrete-producer bypass
— by **enriching the contract**, not special-casing scry. Builds on the
`resource_epoch` contract addition (mere `0bee036`) and the grafting interop
convergence (wgpu-graft `953da76`; welding/scrying DX12 delegation `1f29bb6` /
`44c785a`). Related: [engine_picker_and_pluggability_plan](2026-06-15_engine_picker_and_pluggability_plan.md)
(compile gating + activation), [scrying_tile_plan](../../mere_docs/implementation_strategy/2026-06-10_scrying_tile_plan.md)
(the `ScryingHost`), and wgpu-scry `design_docs/2026-06-28_improvement_backlog.md`.

## Findings (capability audit, 2026-06-28)

A 7-agent audit (workflow `washd2vlp`, adversarially verified) settled the question
*"are scry's concrete needs general/foldable or genuinely scry-specific?"* — **a
clean full fold is possible; the hacks are not scry's durable future.** scry only
*looked* special; the appearance decomposes into three buckets:

1. **General web capabilities missing from the contract** → a new `WebSurface`
   sub-trait. `set_cookie` and `execute_script`-with-result are absent from *both*
   `inker::SurfaceProducer` **and** scry's own `WebSurfaceProducer` trait; they exist
   only as inherent methods on the concrete WebView2 producer (`cookies.rs:35`,
   `navigation.rs:178`). That is the actual reason meerkat holds the concrete type and
   bypasses the registry (`windows_pool.rs:29-57` `ProducerSurface`). CEF has a cookie
   manager + `execute_java_script`; Servo has a net cookie jar + JS eval. General.
2. **General GPU-interop concerns** → grafting (the shared core). The fence
   synchronizer is *already* a public grafting export (`InteropSynchronizer` /
   `SyncMechanism::ExplicitFence` / `Dx12FenceSynchronizer`); scry runs a byte-for-byte
   vendored fork, and inker's contract already carries it
   (`SurfaceSpawnRequest.fence_handle`, `SurfaceFrame.sync::D3d12Fence`). The per-frame
   cache-flush (1×1 `copy_texture_to_buffer` barrier) is **not** scry-specific — weld
   hand-rolls the *identical* barrier (`welding/native_frame/mod.rs:543-582`); grafting
   owns zero copies. Both belong in grafting.
3. **One genuine scry-WGC implementation hack** → internalized in the adapter.
   `force_restart_capture` + the empty-poll stall heuristic exist only because scry
   pulls frames via Windows Graphics Capture from an off-window host (a WGC session can
   die, returning `Ok(None)` forever). CEF pushes via `OnAcceleratedPaint`; Servo
   presents through a surfman swapchain — neither stalls. macOS ScreenCaptureKit has the
   same family of quirk and scry already internalizes it. It hides behind the existing
   `acquire_frame -> Ok(None)` semantics; it must never enter a neutral trait.

Adversarial catch worth recording: scry's *trait* `acquire_frame` is **blocking (2s)
and non-`Option`** — it can't emit `Ok(None)` — while the live path uses the concrete
non-blocking `try_acquire_frame`. So today's `ScryingProducer` adapter, wired to the
blocking method, would stall the render loop and never report "no new frame." The
verifier called this "the real, present reason the bypass exists, more than the
cookie/script gap." Fix: the scry adapter calls the concrete non-blocking acquire
internally and owns the empty-poll loop. Adapter-fill, not a fundamental blocker.

## Contract design

**Trait shape — one sub-trait, plus a capability descriptor, not a hierarchy:**

- **Base `SurfaceProducer`** = frame transport + input + lifecycle (`acquire_frame` /
  `resource_epoch` / `sync`, resize, `set_offset`, mouse/pointer/keyboard, `move_focus`,
  `poll_cursor_shape`, `apply_settings`, `capture_snapshot_png`). This is exactly what
  the generic host pool drives and composites — engine-agnostic. A future non-web GPU
  surface (video / remote-desktop / native embed) implements only this.
- **`WebSurface: SurfaceProducer`** = the web *control* plane: navigate / history /
  reload / stop, `set_cookie`, `execute_script`-with-result, `poll_navigation_event`,
  `poll_web_message`, settings. Every web engine has all of these. **Move
  navigate/history/nav-events/web-messages off the base onto `WebSurface`** so the base
  is genuinely engine-neutral (the pool needs only the base; the omnibar and the flip
  use `WebSurface`). This is the pool-vs-control split.
- **A runtime capability descriptor** for the optional / varies-by-backend web features
  (find-in-page, PDF, downloads, drag/drop, IME observability). These do **not** become
  more sub-traits: their support varies per *backend* (WebKitGTK 6.0 ≠ 4.1 within scry),
  which a compile-time per-type trait can't model. scry already pioneered this
  (`WebSurfaceCapabilities`; the parity-matrix is its human form). Ride a
  `capabilities()` descriptor + `Unsupported` results.

So the only traits are the base + `WebSurface`; the only thing that would add another
trait is a genuinely non-web GPU surface class (the base already accommodates it).

**`inker::Cookie`** — full RFC-6265bis record (name/value/domain/path/expires/secure/
http_only/same_site/partitioned) so the flip boundary is lossless. `verso_api::Cookie`
already models this; today the scry shim drops `SameSite`/`Partitioned` because
`scrying::Cookie` lacks those fields (`windows_pool.rs:38-44`).

**grafting (shared interop)** owns sync (already) + the cache-flush (hoist). A
`resource_epoch`-keyed `EpochCachedImporter` imports-once on epoch change, runs one
cache-flush per frame, and delegates fence waits to the bound `InteropSynchronizer`,
so one generic host pool serves scry/weld/graft with zero per-engine reimplementation.
Keep it pure wgpu + `inker::SurfaceFrame` neutral fields — no mere types (wgpu-graft is
a standalone public lib).

**scry adapter** internalizes its WGC quirks: consume the non-blocking
`try_acquire_frame` + own the empty-poll/restart loop behind `Ok(None)`; populate
`SurfaceFrame.sync` from the fence value (the `weld-engine` adapter already plumbs
`sync: f.sync` — it's the template); thread `SurfaceSpawnRequest.fence_handle` through
the factory.

Result: `windows_pool` drives scry entirely through `SurfaceProducer` + `WebSurface`,
importing via grafting. The concrete bypass, the `ProducerSurface` shim, and the
`inner_mut` escape hatch all disappear; the verso flip (already engine-neutral —
`verso-scry` depends only on `verso-api` and tests against a mock) drives weld/graft
identically by renaming `ScrySurface` → a generic `SecondaryForward` over `WebSurface`.
The only genuinely scry-specific residue is "how this capture transport recovers
liveness," hidden inside the adapter.

## Plan (phases)

1. **inker contract** — add `inker::Cookie` + the `WebSurface` sub-trait; move
   navigate/history/nav-events/web-messages off the base onto `WebSurface`; leave the
   base as frame-transport + input + lifecycle.
2. **adapters** — impl `WebSurface` on the scry/weld/graft adapters (lift the concrete
   `set_cookie`/`execute_script_with_result` up through the adapter); weld gains a
   cookie-manager `set_cookie` + a result-returning script; graft surfaces Servo's
   jar/eval when wired.
3. **scry adapter interop fills** — `translation.rs` `sync: None` →
   `D3d12Fence{handle,value}`; factory honors `fence_handle`; non-blocking acquire +
   internalized stall recovery.
4. **grafting** — hoist the cache-flush into the import/consumer-ready path; build the
   `resource_epoch`-keyed `EpochCachedImporter`. (Touches wgpu-graft; keep mere-free.)
5. **meerkat host pool** — generic epoch-cached pool over `Box<dyn SurfaceProducer>` (+
   `WebSurface` for control); rewire `window_view`/`render`; collapse `ProducerSurface`
   and the flip onto `WebSurface`; delete `windows_pool`'s bespoke import/flush/recovery.

Steps 1-3 are low-risk adapter fills, cargo-check-verifiable on Windows, and unblock the
registry path. Steps 4-5 remove the bypass entirely.

## Risks / gates

1. **Sub-trait dispatch** — the registry returns `Box<dyn SurfaceProducer>`; reaching
   `WebSurface` needs an `as_web_surface(&mut self) -> Option<&mut dyn WebSurface>`
   accessor on the base (query capability once), not a per-call downcast (which would
   reintroduce a softer bypass).
2. **Cookie lossiness** — `scrying::Cookie` lacks `SameSite`/`Partitioned`; a lossless
   `inker::Cookie` fold needs those added upstream in wgpu-scry. *(Cleared: editing scry
   is authorized.)*
3. **Fence path is scry's un-exercised path** — the live path rides keyed-mutex +
   cache-flush; flipping the neutral importer to consume `SurfaceFrame.sync` exercises
   code that is not the live path. Steps 4-5 need a **headed scry-shots smoke driven on
   the producer's UI thread** (WebView2 is COM/HWND-affine), not just a green build —
   the off-window tile must not composite blank.
4. **`execute_script` signature** — scry/CEF differ (with-result vs CEF's current void);
   settle on result-returning with documented best-effort.
5. **grafting stays mere-free** — the `EpochCachedImporter` must be pure wgpu +
   `inker::SurfaceFrame` neutral fields.
6. **graft is demo-stage** for cookies/script/nav (grafting is GPU-interop-only); the
   `WebSurface` sub-trait sits unimplemented for graft until Servo's jar/eval/nav are
   surfaced — fine, but the contract must not assume graft satisfies it yet.

## Progress

- **2026-06-28** — Capability audit complete (workflow `washd2vlp`, 7 agents, 645K
  tokens, adversarially verified): full fold possible, taxonomy + contract design above.
  Prereqs already landed this session: `resource_epoch` on `SurfaceFrame` + adapters
  folded on (`0bee036`); grafting GL-gate (`953da76`) + welding/scrying DX12 delegation
  (`1f29bb6` / `44c785a`) pushed. Sub-trait decision settled with Mark (one `WebSurface`
  + capability descriptor; base = frame transport). Repo-edit + cross-repo gates cleared.
- **2026-06-29** — **Phases 1-4 landed + verified** (executed in the concurrent
  workstream, not this thread). The clean-split design shipped, not the minimal fold: base
  `SurfaceProducer` (frames/input/lifecycle) + `WebSurface: SurfaceProducer` with an
  `as_web_surface` upcast, a full `WebSurfaceCapabilities` descriptor (find/pdf/downloads/
  drag/ime tiers), and a `WebSurfaceEvent` enum; nav/history/cookies/script moved onto
  `WebSurface`. All three adapters (graft/weld/scrying-engine) impl `WebSurface`. scry's own
  `WebSurfaceProducer` trait grew cookie get/set across every backend (wgpu-scry `27943f5`).
  grafting grew the `EpochCachedImporter` (import-once by `resource_epoch`, per-frame
  cache-flush *submitted*, defensive on reused-without-cache) + `close_shared_handle`
  (wgpu-graft `0163d03`). scrying-engine fills: `translation.rs` populates `SurfaceFrame.sync`
  as `D3d12Fence` on the explicit-fence path; the empty-poll stall recovery (600->4800) is
  internalized inside `ScryingProducer::acquire_frame` behind the neutral `Ok(None)`. Verified
  green: inker + 3 adapters (pulling scry) + grafting all compile; the flush submits.
  **Remaining: phase 5 only** — the meerkat generic pool over `Box<dyn SurfaceProducer>`
  (+ `as_web_surface` queried once per drive for the control plane), the render rewire
  replacing `ScryingHost`/`windows_pool` and driving import through grafting's
  `EpochCachedImporter`, and collapsing the flip's `ProducerSurface` onto `WebSurface`. Gated
  on the live roster/gloss render churn settling (`window_view`/`render/*` dirty) and needs
  the headed scry-shots smoke on the producer's UI thread (the fence path is scry's
  un-exercised path; a green build won't prove the off-window tile composites non-blank).
- **2026-07-04 — Phase 5 reconciled (landed in the concurrent workstream) + the headed
  smoke run.** Code-verified: `windows_pool.rs` holds `producer: Box<dyn SurfaceProducer>`,
  spawns via `inker::SurfaceEngineRegistry`, imports through grafting's
  `EpochCachedImporter` + `Dx12FenceSynchronizer`, and reaches the control plane via
  `as_web_surface()` per drive; the flip's `ProducerSurface` is now a thin shim over
  `&mut dyn WebSurface` (the generic forward, keeping the old name); the only concrete
  `scrying::` types left sit in `factory.rs` (the legitimate construction boundary), and
  `inner_mut` is gone (commits `71c1e5a`, `19eed3b`). **Headed fence smoke (gate #3): PASS.**
  Fresh binary, live session: `>compat_view` on an example.com node spawned the WebView2
  surface through the registry (capabilities line logged: `scrying.webview2`,
  `transport=ImportedTexture`, Dx12SharedTexture + fence delegation), the off-window tile
  composited **non-blank** beside a serval tile, and liveness was proven dynamically: a
  select-all forwarded through the CDP input path re-rendered the page (selection
  highlight visible in capture `C:\t\smoke8-wiki.png`), `WebSurfaceEvent` polling observed
  the accelerator (`meerkat.surface.event` diagnostics), and **zero stall-restarts** were
  logged across the session. Two crashes found and fixed on the way (the in-flight
  partition classifier + batch diagnostics walked read accessors on dead batch NodeIds;
  both now gate on the engine's never-panicking `is_live`: `serval_render.rs`
  `node_under_root`, `pane_session.rs` `describe_node_brief`). Plan complete; the
  remaining polish is the `SecondaryForward` rename if wanted.
