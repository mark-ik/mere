# Web Surface Contract Assessment

**Status (2026-09-04):** assessment complete; migration started separately
from the triplet release gate.

## Verdict

Extracting a neutral contract is useful, but not before the contract is made
fully honest about asynchronous browser operations and resource ownership. The
right order is:

1. keep the current publish train unblocked;
2. repair capability honesty and sync/async mismatches in the existing crates;
3. extract a small `surface-engine-api` crate only after the event and request
   model is stable enough to be a public semver surface.

The assessment should not block `grafting` 0.6.0, `scrying` 0.7.0, or
`welding` 0.14.0. It should block any stronger public claim that these crates
already form a complete browser replacement platform.

## Architectural ruling

There are three distinct layers, and combining them would recreate the present
translation problem in a public crate:

1. **Native image custody and import.** Graft owns OS/GPU resource lifetime,
   synchronization, and adoption by the host's wgpu device. Its owned transport
   types eventually need a wgpu-free package/module boundary so a neutral
   surface protocol can use them without selecting the host's wgpu major.
2. **One browser surface protocol.** A small neutral contract owns
   non-blocking frame acquisition, input/focus, correlated commands, one ordered
   event stream, settings outcomes, and runtime capability evidence.
3. **Host orchestration.** Inker owns engine registration and choice, profile
   binding, fallback, document/graph integration, and application routing.

Only layer 2 is an extraction candidate. Graft stays below it, including
authority over any later transport-only package factored from its workspace.
Inker stays above it. This is also the dependency direction: the neutral types
must not depend on Mere, Inker registries, or a specific wgpu version.

## What already looks like a neutral contract

Mere/Inker has the closest thing to the shared browser-surface shape:

- `mere/crates/inker/inker/src/surface_engine.rs:306` defines `SurfaceFrame` as
  raw native texture handle plus sync, size, format, and resource epoch. It is
  deliberately wgpu-free; the host imports on its own device.
- `mere/crates/inker/inker/src/surface_engine.rs:698` defines
  `WebSurfaceCapabilities` with frame transport, cookies, script, pointer,
  document controls, PDF, downloads, devtools, popups, drag/drop, IME,
  accessibility, and degradation reasons.
- `mere/crates/inker/inker/src/surface_engine.rs:870` documents
  `SurfaceProducer` as not `Send`, matching STA-bound WebView2 and
  main-thread-only WKWebView reality.
- `mere/crates/inker/inker/src/surface_engine.rs:942` defines `WebSurface` as
  the web-specific control plane layered over raw surface transport, with one
  ordered `poll_web_event` stream.

That is enough shape to avoid three unrelated browser-surface APIs if the
details are corrected.

## What must not be extracted

These pieces are Mere/Inker host policy, not neutral browser-surface API:

- `SurfaceEngineRegistry`, route decisions, and engine IDs.
- `EngineProfileBinding` as a persona/graph storage decision.
- fallback routing between static documents, Nematic, Scry, Graft, and Weld.
- document graph ingestion, projection, content custody, and app-level
  navigation history.

The neutral contract should describe how a host drives one browser surface and
imports its frame. It should not decide which engine the host chooses or how a
Mere graph stores the result.

## Mismatches to fix first

**Synchronous browser commands.** `WebSurface` currently has synchronous
`get_cookies_for_url` and `execute_script_with_result` methods at
`mere/crates/inker/inker/src/surface_engine.rs:1003` and `:1035`. That does not
fit CEF or WebKit well. Weld already exposes `request_script_result` /
`poll_script_result` and `request_cookies` / `poll_cookies` in
`welding/src/surface.rs:930` and `:958`; the neutral contract should follow
that request-id plus completion-event shape.

**Event order.** Inker's doc says filtered convenience pollers can discard
intervening events and are not part of the contract, but the Scry and Graft
adapters still synthesize the ordered stream by polling navigation before web
messages. A public contract needs one actual queue, not ordered-by-adapter
guesswork.

**Capability defaults.** `WebSurface::capabilities` currently defaults to
`WebSurfaceCapabilities::default()`. That is useful for tests but dangerous as
a public implementor path. Public producers should be required to return a
complete capability struct, with unsupported fields carrying explicit reasons.

**Dropped frame modes.** `scrying-engine` drops CPU RGBA, PNG snapshot, and
overlay-only frames at `mere/crates/inker/engines/scrying-engine/src/translation.rs:63`.
That is acceptable for today's composited-texture route, but it is not a
complete neutral surface contract until CPU and overlay outputs have named host
behavior.

**Borrowed versus transferred handles.** Inker already has
`FrameHandleOwnership`, but Graft's current safe Vulkan descriptor still carries
copyable consumed fds. The neutral contract must depend on Graft's corrected
owned/raw split, or it will preserve the bug at a higher layer.

**Feature-conditioned capability truth.** Scry's WPE capability struct says
unsupported while describing a working feature-enabled WPE path. Weld's static
probe must account for `cef-runtime`. A neutral contract that copies those
answers would become a shared lie.

## Recommended public shape

A future neutral surface-contract crate should be wgpu-free and
producer-neutral. The crate name remains open; `surface-engine-api` below is
descriptive, not a confirmed package name.
It should contain:

- `SurfaceFrame` as an owned transport frame from Graft's wgpu-free boundary,
  plus an explicit CPU frame and an explicit native-overlay outcome. It should
  not reproduce Inker's integer handles plus `Borrowed`/`Transferred` tag as
  its safe front door.
- `SurfaceProducer` as a single-owner, non-`Send` trait with non-blocking frame
  acquisition.
- `WebSurface` as an extension trait for navigation, focus, input, settings,
  cookies, script, capture, find, permissions, authentication, downloads, and
  devtools.
- `WebSurfaceEvent` as the single ordered event stream, including correlated
  completions for async commands.
- `WebSurfaceCapabilities` with no guessed defaults in production
  implementations.
- Correlation IDs minted by the caller and carried on both accepted commands
  and their completion/error events. Acceptance failure returns synchronously;
  accepted work settles exactly once in event order.
- Settings application results that say which fields were applied, rejected, or
  degraded. A blob-shaped `apply_settings` result cannot otherwise keep
  capability truth aligned with actual instance state.

It should not expose `wgpu::Texture`. Graft remains the native import crate;
Scry, Weld, and Servo adapters produce native frame descriptors and the host
imports them on its own `wgpu` device.

Today `grafting` aliases a feature-selected wgpu/wgpu-hal pair throughout its
public API. Depending on that crate directly would therefore make the neutral
contract choose the host's wgpu major. Before extraction, factor the owned
native-frame and synchronization vocabulary into a wgpu-free boundary owned by
the Graft repository, then let both `grafting` and the neutral contract depend
on it. Do not create a second, subtly different raw-handle vocabulary in Mere.

## Design constraints before extraction

- A public capability report is an instance observation, not an operating-system
  guess. Compile-time availability, construction success, selected transport,
  and runtime-observed acceleration are distinct facts.
- The default trait implementation for a supported operation must not block or
  fabricate support. Test stubs can use helpers, but production implementors
  must return a complete report.
- One queue owns web event order. Adapters may translate event values but may not
  create ordering by polling several backend-specific queues in priority order.
- A frame is either immediately available or it is not. Waiting for first paint
  is an explicit, bounded host operation outside the ordinary render-loop poll.
- Resource ownership must be structural. An integer plus a
  `Borrowed`/`Transferred` tag is useful at an FFI edge, but is not the safe
  public custody model for a descriptor that can be consumed or leaked.
- CPU snapshots and native child overlays need explicit host outcomes. If the
  v1 contract does not carry them, capabilities must reject them before a host
  begins acquiring frames.

## Migration path

1. In Mere/Inker, change result-bearing script and cookie reads to
   request/correlation/event APIs while keeping temporary compatibility helpers
   behind adapter-local blocking wrappers.
2. In Scry and Weld, expose complete capability structs that map one-to-one
   into Inker without default guessing.
3. In Graft, land the owned/raw resource API so all higher-level contracts can
   name transferred versus borrowed handles correctly.
4. Factor Graft's owned frame/sync vocabulary behind a wgpu-free package or
   module boundary without changing its ownership authority.
5. Extract the proven browser-control types and traits into the neutral crate;
   do not extract Inker's current file wholesale.
6. Make `scrying-engine`, `weld-engine`, and `graft-engine` depend on the new
   API crate instead of re-owning the bridge vocabulary.
7. Only after one external consumer uses Scry and Weld through the extracted
   contract should Scry/Weld/Graft consider implementing it directly.

## Done conditions

- `cargo test -p inker -p scrying-engine -p weld-engine -p graft-engine` passes
  with the async event contract.
- Adapter tests prove a script-result completion and a cookie-read completion
  cannot overtake earlier navigation or page-message events.
- Capability tests fail if any backend inherits a default capability answer.
- A fresh consumer can drive at least two engines through the extracted crate
  without depending on Mere.
- The DX12, Metal, and Vulkan release consumer still imports browser frames
  through Graft after the extraction.

## Migration progress

- 2026-09-04: Inker added caller-minted `WebRequestId` values,
  `request_script_result` / `request_cookies_for_url` acceptance methods, and
  correlated `ScriptCompleted` / `CookiesCompleted` variants on the single
  `WebSurfaceEvent` queue. A focused ordering test proves completions remain
  behind an earlier navigation event and retain the caller's ids.
- 2026-09-04: The blocking script/cookie methods are gone from Inker and its
  Graft, Scry, and Weld adapter traits. Pelt's only caller was a WebView
  readiness probe; its fixture now posts `pelt.surface.ready` through the
  page-to-host message channel and Pelt drains that from the ordered event
  stream. `scrying-engine` still downgrades correlated script-result and cookie
  reads until Scry's platform callbacks can settle those requests natively.
- Pelt's Windows consumer now recovers Scry's owned native-frame payload and
  imports it with Scry's host-device importer and the same shared fence
  synchronizer given to WebView2. The mixed headed receipt passed at 1280x800
  and 960x640 with three owned imports, three fence waits, host composition,
  and the page-message readiness event (artifact digest `85d1b0ba8a86778f`).
- Focused tests pass for Inker, Graft/Scry/Weld adapters, and Pelt core. Direct
  Weld callback binding, Scry callback migration, and extraction remain open.

## Open decisions

- The public crate name. `surface-engine-api` and `web-surface-api` are working
  descriptions only.
- Whether synchronous compatibility helpers live in adapter crates only or in
  the API crate as explicitly blocking convenience functions.
- Whether CPU snapshot and native child overlay outputs are first-class v1 frame
  transports or deliberately deferred until a consumer needs them.
