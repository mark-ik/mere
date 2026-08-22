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

The 2026-08-22 row proves the artifact/copy ladder, worker-capable IndexedDB,
WGPU model construction, worker termination, message cutoff, and a fresh
worker's warm reopen. It records browser frame intervals and every unavailable
memory fact as `unknown`.

It does not prove a correct WGPU embedding. The patched reduction is accepted
without GPU validation errors, but the output still fails both unit norm and
ESP's committed MiniLM fixture. Its first eight float bit patterns are exactly
the input token ids. The page therefore ends in `limited`. See the checked-in
[`patched receipt`](receipts/2026-08-22_minilm_after_cubek_patch.json) and the
[`staged trace`](receipts/2026-08-22_minilm_stage_trace.json). The staged trace
is sensitive to awaited readback barriers: a shorter trace first produces NaNs
after embedding row zero, while a trace with more barriers keeps embeddings and
the encoder finite before pooling produces NaNs. That moves the boundary from a
specific BERT operation to BrowserWebGpu graph, task, or buffer lifetime.

The model-free
[`embedding control`](repros/burn_browser_embedding/README.md) covers MiniLM's
word-table geometry, model-sized upload pressure, queued consumers, `Param`,
grouped word/position/token-type lookups, and the three-way sum. Its eleven
headed cases pass with empty GPU error scopes. A BERT-width LayerNorm case has
also been added and compiled; its headed receipt remains open.

See also the earlier
[`interpretation`](../../../design_docs/mere_docs/testing/2026-08-21_browser_model_ceiling_receipt.md).

The reduction failure now has a four-case standalone
[`reproducer`](repros/cubek_browser_extrema/README.md) and a validated
runtime-materialization patch. All four cases pass in headed Chromium with
empty GPU error scopes; the
[`reduction receipt`](repros/cubek_browser_extrema/receipts/2026-08-22_patched_iab.json)
records the result. The remaining failure is downstream or independent of the
extrema identity shader. Another model or a trainer would add variables before
this lower execution-lifetime boundary is fixed. The useful next harness is a
small graph whose result changes when an awaited readback barrier is inserted.

It does not prove decoder streaming, cooperative ESP cancellation, GPU-memory
release, a model-size ceiling, or a product default. Those require the decoder
artifact and configurable size sweep retained by the D2 plan.
