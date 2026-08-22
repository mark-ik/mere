# Browser Model Ceiling Probe Plan

**Date**: 2026-08-09

**Status**: D2a, the MiniLM D2b embedding row, and D2c's configured embedding
matrix are complete. Headed Chromium passes four cold/cancel/warm rows from a
34.8 MB F16 BGE artifact through a 438.0 MB F32 E5-base artifact. The upper
embedding boundary is unmeasured above that matrix. Decoder streaming,
cooperative ESP cancellation, and GPU-memory release remain unmeasured. This
track remains independent of the personal mesh and Burn Remote.

**Related**:
[`2026-07-05_inference_provider_plan.md`](2026-07-05_inference_provider_plan.md),
[`../research/2026-06-24_local_models_harness_brief.md`](../research/2026-06-24_local_models_harness_brief.md),
[`2026-08-08_esp_consolidation_plan.md`](2026-08-08_esp_consolidation_plan.md),
[`../../../crates/intel/esp/design_docs/2026-08-09_feature_target_matrix.md`](../../../crates/intel/esp/design_docs/2026-08-09_feature_target_matrix.md),
[`../../../ports/graphshell/docs/2026-08-06_browser_storage_persistence_receipt.md`](../../../ports/graphshell/docs/2026-08-06_browser_storage_persistence_receipt.md)

D2 answers an empirical question: what model artifact and execution sizes can
Mere use responsibly inside a browser tab? A successful wasm compile is the
starting condition, not the answer. The probe measures the whole browser path:
artifact acquisition, storage, integrity, loading, execution, cancellation,
render contention, and restart.

---

## 1. Current evidence and missing proof

Established now:

- ESP's default and model feature combinations compile for
  `wasm32-unknown-unknown`, including WGPU configurations.
- The decoder loads a real TinyLlama checkpoint and streams natively.
- `muniment` has an IndexedDB backend.
- Graphshell has reopened IndexedDB in a headed browser and reports the
  browser's persistence answer honestly.
- Distillery stores, reopens, verifies, loads, and executes four pinned BERT
  embedding artifacts from 34,785,664-byte F16 BGE Micro through
  437,955,512-byte F32 E5-base inside dedicated browser workers.
- Every cold and warm BrowserWebGpu embedding is finite, unit norm, stable
  across workers, and within `1.416e-7` of an independent
  PyTorch/Transformers reference.
- Worker termination at `executing` yields no late message in the configured
  300 ms quiet window for every row.
- Frame p95 stays below 33.4 ms in every sampled phase. Forty-one isolated
  over-bound intervals remain visible, with a 175.8 ms maximum.

Still unproven:

- decoder streaming and cooperative ESP cancellation;
- GPU-memory release after worker termination;
- a first failing embedding row above the 438 MB E5-base artifact;
- a decoder artifact/first-token/throughput matrix; and
- whether isolated frame spikes remain acceptable under a configured product
  workload.

The existing Armillary actor compiling for wasm does not prove a browser thread
or worker runtime. This plan requires a headed execution receipt.

---

## 2. Ownership boundary

- **ESP** owns model parsing, tensor construction, generation, and cancellation
  points.
- **Eidetic and muniment** own manifests, integrity, bytes, and IndexedDB.
- **The browser host** owns the worker, WebGPU adapter selection, lifecycle,
  storage-persistence request, measurements, and UI responsiveness.
- **The host scheduler** will eventually consume capability measurements. The
  probe records facts and does not choose a global device policy.

The probe should be a small development surface rather than production chrome.
Its controls and results must be machine-readable so the same run can be
repeated after browser, model, ESP, or Burn changes.

---

## 3. Measurement artifact

Each run writes `browser-model-probe.json` with at least:

- build commit, browser/version, OS, adapter, driver/backend, and wasm bundle;
- model manifest id, model/component byte sizes, architecture, quantization,
  tokenizer, and loader identity;
- storage persistence state, quota/usage before and after, cold versus warm
  source, integrity result, and reopen result;
- wall time for acquisition, IndexedDB write/read, manifest resolution,
  model parse, WGPU initialization/upload, and first token;
- steady token throughput, cancellation latency, worker restart result, and
  output identity for a fixed seeded prompt;
- process/browser memory observations available on that browser, plus explicit
  `unknown` where an API cannot report GPU memory; and
- frame interval distribution for an idle animation and the same animation
  during load and generation.

Model lists, prompts, run counts, cancellation point, and frame thresholds are
configuration, not hardcoded product defaults. The report records them.

---

## 4. D2a: artifact and copy-ladder receipt

Build one headed dev page that can save a selected real model through
`ModelLibrary`, close/reload, resolve it from IndexedDB, verify every component,
and hand it to ESP.

Instrument the current path explicitly:

1. network/file source to host bytes;
2. host bytes to IndexedDB;
3. IndexedDB to `Vec<u8>` through `ModelLibrary::resolve_components`;
4. safetensors parsing and any intermediate copies; and
5. Burn tensor allocation/upload.

The current `ResolvedModel` owns complete config, tokenizer, and weight byte
vectors. For a large checkpoint this eager representation may create the limit
before model execution does. If a candidate fails before tensor construction,
record it as an artifact-pipeline ceiling rather than a model ceiling.

Stop D2a and spin out a chunked/ranged blob-reader slice if eager copies are the
dominant failure. That seam belongs in Eidetic/muniment and must still verify
content-addressed bytes before ESP consumes them. Do not optimize storage
speculatively if a small real checkpoint completes the ladder.

D2a is done when a real stored artifact survives reload, resolves with matching
hashes, reaches tensor construction, and the report accounts for every known
full-size copy.

---

## 5. D2b: worker execution and cancellation

Run model loading and generation inside a dedicated Web Worker. The main page
owns controls and rendering; the worker owns ESP and its WebGPU device. Use a
small, versioned message protocol with these observable states:

- starting, acquiring, reopening, verifying, loading, ready;
- generating, fragment, canceled, finished, failed; and
- stopping and restarted.

Required receipts:

- the page remains interactive during artifact load and generation;
- a request cancels mid-generation and no later fragments escape;
- terminating the worker releases the session from the host's point of view;
- a fresh worker reopens the same stored model without reseeding IndexedDB; and
- a fixed seed/prompt produces the expected native-versus-browser comparison,
  with differences reported rather than normalized away.

Measure frame intervals on the main page during idle, load, and generation.
This is the browser counterpart to the native shared-queue contention receipt.
The probe does not declare shared or separate devices universally; it records
what the browser exposes and what the UI pays.

---

## 6. D2c: configurable size sweep

Run an escalating configured model set. D2c has two workload phases because an
embedding graph and an autoregressive decoder expose different limits:

1. an embedding phase for artifact size, format, tensor construction, complete
   execution, and worker restart; and
2. a decoder phase for first token, steady token throughput, streaming, and
   cooperative cancellation.

Increase artifact and tensor size until one of the following occurs:

- storage quota or persistence prevents a trustworthy warm reopen;
- integrity or artifact loading fails;
- WebGPU allocation/device loss prevents execution;
- first-token latency or throughput crosses the configured usability bound;
- cancellation/restart fails; or
- frame impact crosses the configured UI bound.

For each model, perform a cold run and warm reopen. Repeat enough times to
report variation rather than one flattering sample. Chromium is the first
receipt because the storage path is already headed-proven there. Other browser
engines join only where WebGPU and required storage APIs are actually available;
unsupported is a recorded capability fact.

The result is a capability table, not one universal number. Artifact format,
quantization, browser, adapter, available storage, and copy strategy all affect
the ceiling.

The embedding phase is complete through the configured E5-base row. The
decoder phase remains open and must use the decoder provider rather than infer
decoder behavior from embedding throughput.

---

## 7. Harness and evidence rules

The 2026-08-06 storage receipt identified the absence of a browser harness. D2
must not repeat an opaque manual ritual. Provide:

- one command to build and serve the probe;
- stable page controls or a small JS control API;
- JSON export of every run; and
- a documented headed procedure for WebGPU facts that headless mode cannot
  reproduce faithfully.

Pure serialization, state-machine, and report-generation logic gets ordinary
Rust tests. The final claim still requires a real headed browser and hardware.

Keep generated wasm, model artifacts, and reports out of Git. Preserve selected
small JSON receipts under a dated documentation path when they substantiate a
decision.

---

## 8. Non-goals and stop rule

D2 does not choose a default model, ship product UI, add endpoint inference,
change the mesh, migrate Burn, implement adapters, or train a model.

Stop when the configured capability matrix is recorded and its limiting layer
is identified. Any storage-reader refactor becomes its own measured slice. A
product default is a later decision informed by these receipts.

## 9. Done conditions

- A real model is saved, reopened, verified, loaded, and executed in a headed
  browser.
- Cold and warm runs produce machine-readable reports.
- The full artifact/copy ladder and limiting layer are named.
- Worker cancellation, termination, and warm restart pass.
- UI frame impact is measured during load and generation.
- At least one success and the first configured failure/limit are recorded, or
  the configured model set all succeeds and the unmeasured upper boundary is
  stated.
- Compile, storage, headed execution, and performance claims remain distinct.

## 10. Progress

- **2026-08-09**: scoped from the completed ESP consolidation ledger. Moved D2
  out of the mesh/Burn Remote serial sequence; grounded it in the live
  IndexedDB persistence receipt and ESP wasm matrix; exposed eager
  `ResolvedModel` byte ownership as a possible false ceiling; and required a
  worker lifecycle, copy ladder, configurable sweep, and machine-readable
  headed receipt.
- **2026-08-21, MiniLM D2a/D2b receipt**: added the standalone development
  surface under `ports/distillery/probe`, including worker-capable Muniment
  IndexedDB, an async ESP BERT readback path, configurable controls, frame
  sampling, raw WebGPU error capture, and JSON export. A headed Chromium run
  saved all-MiniLM-L6-v2 through `ModelLibrary`, reopened the same manifest and
  hashes from IndexedDB, accounted for five full 90,868,376-byte host copies,
  terminated a worker at `executing`, observed no late message during the
  300 ms quiet window, and repeated the warm reopen in a fresh worker.

  Execution did not pass. CubeCL's generated max-reduction WGSL was rejected
  because its constant bitcast represents negative infinity. The returned
  384-float buffer had norm zero and missed ESP's reference fixture; its first
  eight float bit patterns decode exactly to the input token ids `101, 2023,
  2003, 1037, 7099, 6251, 1012, 102`. Repeatability therefore is not evidence
  of a correct embedding. The page reports `limited`, and the limiting layer is
  Burn/CubeCL BrowserWebGpu at the first embedding row. The raw receipt and
  interpretation are in
  [`2026-08-21_browser_model_ceiling_receipt.md`](../testing/2026-08-21_browser_model_ceiling_receipt.md).

  A separate packaging trap was isolated before that ceiling: wgpu 30 with
  wasm-bindgen 0.2.126 panics while decoding a successful null WebGPU error
  scope. The probe pins the verified 0.2.122/0.4.72/0.3.99 compatibility row
  and requires a matching CLI. That pin exposes the model failure; it does not
  fix it. D2b resumes when an upstream or narrowly justified patch produces a
  fixture-valid vector. Decoder cancellation and the size sweep stay closed
  until then.
- **2026-08-21, reduction extraction**: reduced the BrowserWebGpu failure to a
  standalone four-case Burn extrema harness without model, tokenizer, storage,
  or ESP dependencies. The released Cubek identity bitcasts literal infinity
  bits; Chromium evaluates the expression as a constant and rejects its
  non-finite `f32`. This is the WGSL half missing from CubeCL's already-landed
  constant-bitcast materialization fix for C++ dialects. A probe-local
  `cubek-reduce` patch now preserves infinity and NaN semantics by passing the
  bits through a mutable kernel local. Its native WGPU infinity and NaN suites,
  wasm build, generated-WGSL inspection, and strict Clippy gates pass. The
  post-patch headed browser and MiniLM fixture receipts remain open, so D2b's
  status has not advanced.
- **2026-08-22, post-patch headed localization**: the four-case extrema
  harness passes in Chromium 151 with finite, infinity, and NaN semantics intact
  and empty GPU error scopes. The same browser still returns tokenizer-id bits
  for the ordinary MiniLM graph, with norm zero and a 0.077806376 fixture error.
  Native release WGPU passes the exact 90.9 MB artifact and fixture.

  Feature-gated ESP traces then forced readback at fresh graph prefixes. With a
  short trace, the word lookup is finite and the embedding block becomes NaN
  after its first 384-float row. Adding awaited readbacks for position ids,
  word/position/token-type lookups, and their sums changes the result: the full
  embedding block and encoder remain finite, then pooling becomes entirely NaN.
  Empty GPU error scopes accompany both profiles. The failure moving when
  observation barriers are added rules out a stable operator-local diagnosis
  and identifies asynchronous BrowserWebGpu graph, task, or buffer lifetime as
  the current limit.

  A model-free embedding control now covers exact MiniLM table geometry, bulk
  upload pressure, queued consumers, `Embedding`/`Param`, grouped lookups, and
  their sum; all eleven headed cases pass. A BERT-width LayerNorm case is built
  but still needs its headed receipt. D2b remains open until a minimal failing
  lifetime reproducer or corrected upstream runtime row makes the browser
  MiniLM vector fixture-valid. More models and D2c stay closed because they
  cannot clarify this lower boundary.
- **2026-08-22, shared-input binary fix and MiniLM recovery**: the model-free
  graph reduced the remaining failure to binary operations whose two logical
  operands name the same GPU allocation. Scalar operations and independently
  uploaded equal tensors pass. Shared multiply fails without a WebGPU error;
  Burn's exact LayerNorm unit case and the `8 x 384` BERT-width case return the
  input unchanged.

  Tightening CubeCL's mutability count did not fix the graph, and forcing a
  distinct output did not fix it. Binding the allocation once, aliasing the
  second logical input to input zero, and writing to a distinct output passes
  every graph and embedding case. Mere carries that narrow guard in vendored
  `burn-cubecl` plus a logical-allocation identity helper in its existing
  `cubecl-runtime` patch. Burn and CubeCL main still contain the susceptible
  source path as of the audited revisions recorded in the upstream issue
  draft.

  The forcing consumer now passes in headed Chromium 151: cold and warm
  MiniLM embeddings are finite and unit norm, repeat within and across workers,
  and match ESP's native fixture within `8.940697e-8`. Integrity reopen,
  worker termination, the 300 ms quiet window, and all GPU error scopes pass.
  Frame p95 stays below the configured 33.4 ms bound, with seven isolated
  over-bound intervals and a 218.2 ms cold maximum. D2c may now open with more
  configured artifacts. Trainers still do not answer the ceiling question.
- **2026-08-22, D2c embedding matrix**: pinned and hash-verified BGE Micro v2,
  MiniLM-L6-v2, E5-small-v2, and E5-base-v2 at exact upstream revisions. BGE's
  34.8 MB F16 weights forced explicit F16-to-f32 promotion in ESP's
  safetensors loader; BF16 and other unimplemented dtypes remain rejected.
  Independent PyTorch 2.11 / Transformers 5.8 CPU references and an ESP
  NdArray harness pass every row.

  A clean `3bdb66fc` headed Chromium 151 build then passed every configured
  cold, cancellation, and warm row. The 437,955,512-byte E5-base artifact is
  the largest success. Every output is finite, unit norm, repeatable across
  executions and workers, and within `1.416e-7` of its independent reference;
  integrity reopen and all GPU error scopes pass. Persistent storage was
  requested and denied, leaving the 698 MB IndexedDB corpus best effort.
  Frame p95 remained below 33.4 ms in every phase, with 41 isolated over-bound
  intervals and a 175.8 ms maximum. The upper embedding boundary is unmeasured
  above the matrix. Decoder streaming, cooperative ESP cancellation, and GPU
  memory release remain open as the decoder phase.
