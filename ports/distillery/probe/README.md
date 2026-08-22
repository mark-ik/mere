# Distillery browser model probe

This is D2's development surface, homed with the model-works port without
turning the probe into product chrome. It runs a real MiniLM artifact through
the Eidetic `ModelLibrary`, Muniment IndexedDB, ESP's BERT loader, and Burn
WGPU inside a dedicated browser worker.

The probe is a standalone Cargo workspace. This keeps an evidence-only browser
surface from widening Mere's ordinary product graph and lets it build while
unrelated headed ports move between Genet host generations.

From the Mere root:

```powershell
cargo binstall wasm-bindgen-cli@0.2.122 --root C:\path\to\wasm-bindgen-0.2.122
ports/distillery/probe/run-probe.ps1 `
  -WasmBindgen C:\path\to\wasm-bindgen-0.2.122\bin\wasm-bindgen.exe
```

Then open the printed URL in a headed Chromium browser and select **Run cold,
cancel, and warm**. `window.distilleryModelProbe.runSuite()` is the stable
automation entry point; `window.distilleryModelProbe.receipt()` returns the
machine-readable result. Generated wasm and model artifacts stay out of Git;
selected dated receipts may be checked in when they substantiate a boundary.

The first configured row is the checked-out
`models/all-MiniLM-L6-v2` artifact. The server exposes the Mere root so the
page can fetch those local files without copying 90 MB into the probe tree.

## Claim boundary

The 2026-08-22 MiniLM row now passes its numerical gate. Cold and warm workers
produce the same finite, unit-norm 384-float embedding, within
`8.940697e-8` of ESP's committed native fixture. The stored 90,868,376-byte
weight artifact reopens with matching integrity, worker termination emits no
late message in the 300 ms quiet window, and all WebGPU error scopes are empty.
See the
[recovered MiniLM receipt](receipts/2026-08-22_minilm_after_binary_alias_patch.json).

The root defect is a same-allocation binary launch in Burn/CubeCL. The published
`burn-cubecl 0.22.0-pre.2` path binds one allocation twice when evaluating a
graph such as `x.clone() * x`. Chromium executes that shape without a
validation error but returns stale storage. Scalar operations and two
independently uploaded operands pass. Burn LayerNorm uses the shared-input shape
for its variance and therefore returned the input unchanged.

Mere carries a narrow `burn-cubecl` backport: detect equal logical allocation
and view identity, bind the storage once, alias the second logical input to the
first binding, and write to a distinct output. The
[model-free receipt](repros/burn_browser_embedding/receipts/2026-08-22_binary_alias_iab.json)
records the unpatched failure, two rejected fix hypotheses, and the passing
backport. The existing Cubek extrema materialization patch remains independently
required.

Frame p95 stayed below the configured 33.4 ms bound in idle, cold,
cancellation, and warm phases. There were seven isolated over-bound intervals:
three cold, two during cancellation, and two warm; the cold maximum was 218.2
ms. The receipt reports those spikes rather than treating p95 as a complete UI
smoothness claim.

This row proves artifact/copy-ladder viability, worker-owned IndexedDB, WGPU
model construction, numerical execution, worker termination, message cutoff,
and warm reopen for one MiniLM embedding artifact. It does not prove decoder
streaming, cooperative ESP cancellation, GPU-memory release, an upper model-size
ceiling, or a product default.

D2c can now open. More models are useful only as a configured size and format
sweep with the same cold/warm, fixture, frame, and cancellation receipts.
Trainers remain outside this ceiling probe.
