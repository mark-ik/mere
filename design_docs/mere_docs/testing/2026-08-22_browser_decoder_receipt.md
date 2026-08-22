# Browser Decoder Row Receipt

**Date**: 2026-08-22

**Result**: Distillery's first D2c decoder row passed in a clean headed browser
build. The same pinned SmolLM2 generation matches Transformers CPU, ESP
NdArray, and ESP BrowserWebGpu exactly.

## Configuration

- Clean source commit: `dd215ebdbd335d37a60042a51b2eeb1e070369a3`
- Browser: headed Chromium 151 on Windows
- Adapter report: NVIDIA Lovelace
- Backend: Burn 0.22.0-pre.2 WGPU in dedicated Web Workers
- Model: `HuggingFaceTB/SmolLM2-135M-Instruct` at revision
  `12fd25f77366fa6b3b4b768ec3050bf629380bac`
- Weights: 269,060,552-byte BF16 safetensors, promoted to f32 by ESP
- Prompt: `Mere keeps a model local.`
- Greedy cap: 8 generated tokens
- Executions per cold or warm worker: 2
- Browser receipt:
  [`2026-08-22_d2c_browser_decoder.json`](../../../ports/distillery/probe/receipts/2026-08-22_d2c_browser_decoder.json)
- ESP NdArray control:
  [`2026-08-22_d2c_native_decoder.json`](../../../ports/distillery/probe/receipts/2026-08-22_d2c_native_decoder.json)
- Independent Transformers control:
  [`2026-08-22_d2c_transformers_decoder.json`](../../../ports/distillery/probe/receipts/2026-08-22_d2c_transformers_decoder.json)

## Correctness

All three paths generated token ids
`[198, 198, 504, 1743, 314, 253, 216, 35]` and text
`"\n\nThe model is a 3"`. Both browser workers repeated the result internally,
the warm worker matched the cold output hash, and every emitted text fragment
crossed the worker boundary in order. All WebGPU error scopes were empty.

This row forced two decoder corrections. Burn's general rotary helper pairs
adjacent values, while Llama-family `rotate_half` pairs the two head-dimension
halves. ESP now owns that split-half rotary rule and tests it directly. The
browser then reached token selection but trapped on Burn's synchronous tensor
readback. ESP now has an async generation/readback path; its native parity test
selects the same ids as the synchronous path.

## Timing and frame receipt

| Worker execution | First token | Total for 8 tokens | Post-first-token rate |
| --- | ---: | ---: | ---: |
| Cold first | 364 ms | 743 ms | 18.47 tok/s |
| Cold repeat | 72 ms | 339 ms | 26.22 tok/s |
| Warm first | 279 ms | 642 ms | 19.28 tok/s |
| Warm repeat | 67 ms | 341 ms | 25.55 tok/s |

Cold acquisition, IndexedDB write, integrity resolution, and model load took
309 ms, 925 ms, 599 ms, and 401 ms respectively. Warm resolution and model
load took 576 ms and 516 ms. These are one-machine capability measurements,
not product defaults.

Idle, cold, and warm frame p95 remained at or below 6.2 ms against the
configured 33.4 ms bound. Four individual generation-phase intervals crossed
the bound; the largest was 60.7 ms. The raw receipt preserves those spikes.

## Open boundary

The stored artifact reopened with matching component hashes, but Chromium again
denied persistent-storage promotion, so this remains a reconstructible
best-effort cache. Browser and GPU memory were not exposed by the available
APIs.

This is one successful decoder row, not a decoder ceiling. Cooperative ESP
cancellation during generation and GPU-memory release after worker termination
remain unmeasured. A larger decoder row is useful when a forcing consumer needs
a higher capability bound; cancellation and teardown are the next structural
proofs.
