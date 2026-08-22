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

The 2026-08-21 row proves the artifact/copy ladder, worker-capable IndexedDB,
WGPU model construction, worker termination, message cutoff, and a fresh
worker's warm reopen. It records browser frame intervals and every unavailable
memory fact as `unknown`.

It does not prove a correct WGPU embedding. CubeCL's max-reduction WGSL is
rejected by BrowserWebGpu, and the repeatable output fails both unit norm and
ESP's committed MiniLM fixture. The page therefore ends in `limited`. See the
checked-in [`receipt`](receipts/2026-08-21_minilm_browser_ceiling.json) and the
[`interpretation`](../../../design_docs/mere_docs/testing/2026-08-21_browser_model_ceiling_receipt.md).

The reduction failure now has a four-case standalone
[`reproducer`](repros/cubek_browser_extrema/README.md) and a candidate
runtime-materialization patch. The main probe is wired to that candidate for
the next headed run. The checked-in MiniLM receipt remains the authority until
that run passes the numerical fixture; a patched wasm build is not a receipt.

It does not prove decoder streaming, cooperative ESP cancellation, GPU-memory
release, a model-size ceiling, or a product default. Those require the decoder
artifact and configurable size sweep retained by the D2 plan.
