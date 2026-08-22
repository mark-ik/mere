# Burn browser embedding reproducer

This is the embedding-only extraction of Distillery's MiniLM BrowserWebGpu
failure. It removes the model artifact, tokenizer, storage, ESP BERT graph,
pooling and normalization. Deterministic Burn graph controls run in a dedicated
worker:

- a `4 × 3` table with four mixed indices;
- a `16 × 384` table with eight mixed indices; and
- a `2 × 384` table with eight zero indices;
- a `16 × 384` table retained across 100 subsequent 589,824-byte tensor
  uploads, matching the loader's bulk-write lifecycle; and
- the real MiniLM `30,522 × 384` word-table geometry retained across another
  44 MB of uploads, matching the model's aggregate upload size and order; and
- that model-sized row retained while 128 dependent operations are queued
  before readback, testing output-handle lifetime inside a larger graph; and
- the word, position, and token-type lookups queued together and consumed by
  one sum before any readback, matching BERT's embedding subgraph; and
- the model-sized lookup through Burn's `Embedding` and `Param` wrappers rather
  than the underlying tensor operation; and
- the word lookup after 100 retained 384-float uploads, stressing the small
  suballocation pool used by BERT biases and LayerNorm parameters; and
- an `8 × 384` LayerNorm with host-checked row means and variances.

Each case awaits an integer input round trip, computes the expected rows on the
host, and records output bits, non-finite values, and the first mismatch. WebGPU
validation errors are captured separately.

From this directory, with wasm-bindgen CLI 0.2.122 installed:

```powershell
.\run-repro.ps1 -WasmBindgen C:\path\to\wasm-bindgen.exe
```

Open the printed URL in headed Chromium and choose **Run embedding cases**.
For automation, call `window.burnEmbeddingRepro.run()` and inspect both
`result` and `gpu_errors`.
