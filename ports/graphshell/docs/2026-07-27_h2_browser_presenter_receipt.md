# Graphshell H2 Browser Presenter Receipt

**Plan:** [Graphshell reference host H2](../../../design_docs/mere_docs/implementation_strategy/2026-07-27_graphshell_reference_host_plan.md#h2-present-the-host-in-a-browser)  
**Executed:** 2026-07-28

## Result

H2 is complete. Graphshell now has a headed browser host built from the H1
local Mere fixture and remote projection:

- Mere Canvas owns graph frames, selection, pan, zoom, and node drag;
- Cambium composes Graphshell chrome through Genet's neutral DOM and layout;
- NetRender draws content and alpha-composited chrome into WebGPU textures;
- an asynchronous `HTMLCanvasElement` presenter owns the browser surface;
- semantic HTML mirrors every control and exposes keyboard input and status;
- wide and narrow layouts share the same host state and action vocabulary.

`graphshell-web` is a separate workspace package. Browser-only dependencies do
not enter the portable `graphshell` web profile. Canvas2D was not used.

## Headed browser wall

| Browser | Viewport | Pan/zoom | Select/drag | Detail/action | Session switch |
| --- | ---: | --- | --- | --- | --- |
| Chromium 150.0.7871.187 | 1280×800 | pass | pass | pass | pass |
| Chromium 150.0.7871.187 | 600×800 | pass | pass | pass | pass |
| Firefox 151.0 | 1280×800 | pass | pass | pass | pass |
| Firefox 151.0 | 600×800 | pass | pass | pass | pass |

Each run began from a fresh H1 fixture. It selected the HTTPS object, dragged
it to a different persistent position, changed the camera through zoom and pan,
opened detail, invoked the advertised `graphshell.inspect` action, observed one
accepted invocation, mounted the remote session, and restored the local
session. The final semantic tree exposes the local session, graph controls,
object detail, accepted action status, and viewport.

The committed evidence is:

- [machine-readable interaction and semantic-tree receipt](receipts/h2_browser_receipts.json);
- [Chromium wide](receipts/h2_chromium_wide.png);
- [Chromium narrow](receipts/h2_chromium_narrow.png);
- [Firefox wide](receipts/h2_firefox_wide.png);
- [Firefox narrow](receipts/h2_firefox_narrow.png).

## Runtime defects found by the headed proof

The WASM compile alone had hidden two runtime defects.

First, Mere's graph clock called `std::time::SystemTime::now()`, which panics on
`wasm32-unknown-unknown`. Persisted wall-clock stamps now use `web_time`, which
re-exports the native clock on desktop and uses `Date.now()` in browser WASM.

Second, a dragged node under an active analytic arrangement snapped back on the
next frame because the arrangement reapplied its original slot. Dragging now
updates that active slot before the overlay runs again. A focused native Canvas
test proves the new contract.

## Verification

The browser package compiled and linked for WASM:

```powershell
$env:CARGO_TARGET_DIR = 'target-plan-graphshell-web'
cargo check -p graphshell-web --offline --target wasm32-unknown-unknown
cargo build -p graphshell-web --offline --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir ports/graphshell/web/pkg `
  target-plan-graphshell-web/wasm32-unknown-unknown/debug/graphshell_web.wasm
```

The focused Canvas regression passed:

```powershell
$env:CARGO_TARGET_DIR = 'target-plan-graphshell-native'
cargo test -p mere-canvas dragging_a_node_updates_its_active_strategy_slot --lib --offline
```

Result: 1 passed, 0 failed, 148 filtered out.

Both browser runs were headed. Chromium used the installed Chrome binary.
Firefox used Playwright's matching headed Firefox build with WebGPU enabled.
The in-app Browser was also used to inspect the live Chromium semantic surface
and visual frame before the cross-browser wall.

This checkout uses the repository's ignored local Cargo patches. The commands
prove the live patched checkout and do not claim clean patch-free dependency
resolution. Existing dependency warnings remain outside this receipt.

## Acceptance boundary

H2 proves the useful browser presentation and interaction loop. It does not
prove browser persistence, OPFS worker ownership, extension packaging, native
installers, system application launch, or live Personae vault authority. Those
remain later host slices.
