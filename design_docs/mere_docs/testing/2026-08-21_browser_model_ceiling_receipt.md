# Browser Model Ceiling Receipt: MiniLM

**Date**: 2026-08-21
**Result**: artifact and worker corridor passed; BrowserWebGpu execution failed
the numerical gate.

## Configuration

- Model: `sentence-transformers/all-MiniLM-L6-v2`
- Weights: 90,868,376 bytes
- Browser: headed Chromium 151 on Windows
- Adapter report: NVIDIA, Lovelace
- Backend: Burn 0.22.0-pre.2 WGPU in a dedicated Web Worker
- Fixed input: `This is a sample sentence.`
- Raw receipt:
  [`2026-08-21_minilm_browser_ceiling.json`](../../../ports/distillery/probe/receipts/2026-08-21_minilm_browser_ceiling.json)

## Passed boundaries

- The cold worker fetched config, tokenizer, and safetensors, saved them through
  Eidetic `ModelLibrary` and Muniment IndexedDB, and resolved matching hashes.
- A fresh warm worker reopened the same manifest without refetching the model.
- The receipt names five full 90,868,376-byte host copies before per-tensor GPU
  upload. This is structural copy accounting, not peak-memory telemetry.
- Terminating the middle worker at `executing` took less than the timer's
  resolution and produced no late message during the 300 ms quiet window.
- The main page continued to sample frames during cold load, termination, and
  warm reopen. Memory APIs unavailable to the browser are recorded as
  `unknown`; persistent storage was requested and not granted.

## Failed boundary

Both cold and warm workers reported a `GPUValidationError` while creating
CubeCL's `reduce_kernel_in_f32_in_size_4_out_f32_out_size_1_acc_f32` shader:
the generated WGSL constant bitcast represents negative infinity, which the
browser validator rejects.

The returned vector is not an embedding:

- width: 384;
- L2 norm: 0;
- maximum error across the first eight committed fixture values: 0.077806376;
- fixture within tolerance: false; and
- first eight float bit patterns decode to the tokenizer ids
  `101, 2023, 2003, 1037, 7099, 6251, 1012, 102`.

Cold/warm hashes matching only proves deterministic bad output. The limiting
layer is Burn/CubeCL BrowserWebGpu execution at the first real embedding row.
D2b remains partial; the decoder row, cooperative cancellation, and D2c size
sweep stay closed.

## Packaging finding

wgpu 30 with wasm-bindgen 0.2.126 also panics while decoding a successful null
WebGPU error-scope result. Pinning wasm-bindgen 0.2.122,
wasm-bindgen-futures 0.4.72, and js-sys/web-sys 0.3.99 with a matching CLI
removes that sibling trap and exposes the model execution failure above. The
pin is local to this standalone evidence workspace.
