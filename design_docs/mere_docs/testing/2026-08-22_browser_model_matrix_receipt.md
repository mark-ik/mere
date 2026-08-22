# Browser Model Matrix Receipt

**Date**: 2026-08-22

**Result**: the configured D2c embedding matrix passed; the upper embedding
boundary is above the 438 MB E5-base row.

## Configuration

- Clean source commit: `3bdb66fce2e17aa36882b5f7327869f650cfd544`
- Browser: headed Chromium 151 on Windows
- Adapter report: NVIDIA Lovelace
- Backend: Burn 0.22.0-pre.2 WGPU in dedicated Web Workers
- Fixed input: `query: Mere keeps a model local.`
- Executions per cold or warm worker: 3
- Frame p95 bound: 33.4 ms
- Raw decision receipt:
  [`2026-08-22_d2c_browser_matrix.json`](../../../ports/distillery/probe/receipts/2026-08-22_d2c_browser_matrix.json)
- Independent native control:
  [`2026-08-22_d2c_native_matrix.json`](../../../ports/distillery/probe/receipts/2026-08-22_d2c_native_matrix.json)

## Capability table

| Model | Weights | Dtype | Width | Browser max reference error | Cold first execution | Warm first execution |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| TaylorAI/bge-micro-v2 | 34,785,664 B | F16 | 384 | 9.383075e-8 | 218 ms | 188 ms |
| sentence-transformers/all-MiniLM-L6-v2 | 90,868,376 B | F32 | 384 | 1.4156103e-7 | 262 ms | 185 ms |
| intfloat/e5-small-v2 | 133,466,304 B | F32 | 384 | 9.685755e-8 | 300 ms | 227 ms |
| intfloat/e5-base-v2 | 437,955,512 B | F32 | 768 | 4.4703484e-8 | 193 ms | 172 ms |

Every row produced finite unit-norm output with the configured width, repeated
within a worker, matched the fresh warm worker, and stayed within the `1e-4`
reference tolerance. Every GPU error scope was empty.

## Artifact and worker boundary

Each cold worker fetched config, tokenizer, and weights, saved them through
Eidetic and Muniment IndexedDB, resolved all three components, and verified
their BLAKE3 identities. Each cancellation worker reopened and uploaded the
stored artifact, reached `executing`, and was terminated. No later message
arrived in the 300 ms quiet window. Each fresh warm worker reopened the same
manifest and reproduced the cold output hash.

The eager path still names five full host-side weight copies before per-tensor
GPU upload. E5-base therefore proves this copy strategy at 437,955,512 bytes;
it does not turn structural copy accounting into peak-memory telemetry.

Persistent storage was requested and denied. The final best-effort IndexedDB
usage was 698,078,734 bytes against a reported 11,435,496,974-byte quota.

## Frame boundary

Every phase's p95 stayed below 33.4 ms. Across sixteen idle, cold,
cancellation, and warm samples, 41 individual intervals crossed the bound. The
largest was 175.8 ms during E5-base warm reopen. Product responsiveness remains
a workload decision rather than a consequence of the p95 pass.

## Open boundary

All configured embedding rows succeeded, so the first embedding limit is not
identified. A larger row such as E5-large-v2 would be useful only if a measured
failure is needed; with the current eager five-copy ladder it is also likely to
probe host linear-memory pressure before model quality matters.

This receipt does not cover decoder streaming, first-token or steady-token
throughput, cooperative ESP cancellation, or GPU-memory release after worker
termination. Those belong to the D2c decoder phase. Trainers do not answer this
browser ceiling question.
