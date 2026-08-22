# Distillery browser model probe

This is D2's development surface, homed with the model-works port without
turning the probe into product chrome. It runs a pinned real-model matrix
through the Eidetic `ModelLibrary`, Muniment IndexedDB, ESP's BERT loader, and
Burn WGPU inside dedicated browser workers.

The probe is a standalone Cargo workspace. This keeps an evidence-only browser
surface from widening Mere's ordinary product graph and lets it build while
unrelated headed ports move between Genet host generations.

From the Mere root:

```powershell
cargo binstall wasm-bindgen-cli@0.2.122 --root C:\path\to\wasm-bindgen-0.2.122
ports/distillery/probe/fetch-model-matrix.ps1
ports/distillery/probe/run-probe.ps1 `
  -WasmBindgen C:\path\to\wasm-bindgen-0.2.122\bin\wasm-bindgen.exe
```

The fetch command verifies every configured byte count and SHA-256 before
installing the ignored local artifacts. The standalone lockfile pins selected
versions, but `run-probe.ps1` cannot use Cargo's `--locked` flag: inherited
path-workspace patch tables make Cargo reorder semantically identical
`patch.unused` entries. The checked-in package selections and exact
wasm-bindgen CLI pin remain the reproducibility boundary.

Open the printed URL in a headed Chromium browser and select **Run configured
matrix**. `window.distilleryModelProbe.runSuite(modelId)` runs one configured
row, `runMatrix()` runs in ascending artifact size, and `receipt()` returns the
machine-readable result. Generated wasm and model artifacts stay out of Git;
selected dated decision receipts are checked in when they substantiate a
boundary.

## Claim boundary

The 2026-08-22 D2c embedding matrix passes all four configured rows in a clean
headed Chromium 151 build: [BGE Micro v2](https://huggingface.co/TaylorAI/bge-micro-v2),
MiniLM-L6-v2, [E5-small-v2](https://huggingface.co/intfloat/e5-small-v2), and
[E5-base-v2](https://huggingface.co/intfloat/e5-base-v2). Their weight
artifacts range from 34,785,664-byte F16 through 437,955,512-byte F32. Every
cold and warm output is finite, unit norm, stable within and across workers,
and within `1.416e-7` of its independent PyTorch/Transformers reference.
IndexedDB integrity reopen, termination at `executing`, the 300 ms quiet
window, and WebGPU error scopes pass for every row. See the
[browser matrix receipt](receipts/2026-08-22_d2c_browser_matrix.json) and
[native control](receipts/2026-08-22_d2c_native_matrix.json).

BGE's published F16 artifact forced one narrow ESP extension: the safetensors
loader now accepts F16 weights and promotes them to Burn f32 tensors. Other
published dtypes still fail explicitly. This is loader-format support, not a
new model adapter.

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

Frame p95 stayed below the configured 33.4 ms bound in every idle, cold,
cancellation, and warm phase. The clean matrix still recorded 41 isolated
over-bound intervals; the largest was 175.8 ms during E5-base warm reopen. The
receipt keeps those spikes visible rather than treating p95 as a complete UI
smoothness claim.

This matrix proves the eager five-copy artifact ladder, worker-owned IndexedDB,
F16/F32 BERT construction, numerical BrowserWebGpu execution, worker
termination, message cutoff, and warm reopen through the 438 MB embedding row.
The browser denied persistent-storage promotion, so the stored rows remained
best effort even though same-origin warm reopen passed.

This completes D2c's configured embedding phase. The upper embedding boundary
is unmeasured above E5-base. Decoder streaming, first-token and token-throughput
bounds, cooperative ESP cancellation, GPU-memory release, and a product default
remain open. Trainers remain outside this ceiling probe.
