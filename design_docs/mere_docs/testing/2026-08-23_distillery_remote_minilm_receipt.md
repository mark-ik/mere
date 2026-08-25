# Distillery Remote MiniLM Receipt

**Date**: 2026-08-25

**Implementation**: Mere `74b55236`; p2panda `9f2c2a01` with
`mere-p2panda-net 0.7.2` published for the raw-ALPN seam.

**Clean plain rerun**: merged Mere `1aa8d4f9`, owned probe paths clean.

Distillery ran ESP's pinned `sentence-transformers/all-MiniLM-L6-v2` through
Burn Remote between two distinct application-owned p2panda/Iroh endpoints. The
lending peer served a native `burn-wgpu 0.22.0-pre.2` device; the posting peer
loaded ESP's BERT graph onto an authorized remote device under epoch 0.

The 384-float result was finite and unit norm. Maximum absolute error was
`1.4901161e-7` against ESP's native NdArray control and `1.4156103e-7` against
the existing BrowserWebGpu reference prefix, below the committed `1e-4`
tolerance.

With one live session, a 512-row MiniLM request was still in flight when the
lending device became active. The first supervisor turn returned
`AwaitingStop`; a later turn authored `Reclaimed`. The request returned a
bounded readback error rather than hanging, and the exact lease's server
session count reached zero. Restoring spare conditions granted epoch 1 under a
new lease. A new authorized session reloaded the model, reproduced every one
of the 384 first-run values exactly, and was reclaimed to zero sessions for
shutdown.

The machine-readable receipt is
[`ports/distillery/probe/receipts/2026-08-23_remote_minilm.json`](../../../ports/distillery/probe/receipts/2026-08-23_remote_minilm.json).

## Claim boundary and sidequest

This proves real model execution, numerical parity, active-request
cancellation at the session boundary, stop-before-reclaim ordering, and fresh
WGPU-backed recovery. The follow-up now measures Mere's patched native CubeCL
`ComputeClient::memory_usage()` across every server-device stream. Both model
runs raised the allocator from zero to 101 live allocations and 90,261,504
bytes in use. First reclaim and recovery reclaim each returned to zero live
allocations and zero bytes in use before reclaim was acknowledged. Reserved
bytes also happened to return from 612,368,384 to zero, but remain recorded
rather than required. This is allocator evidence, not driver VRAM telemetry.

Burn Remote now acknowledges closure only after its worker drains, syncs,
drops its interpreter, and runs backend memory cleanup. Draining sessions stay
visible; a worker panic becomes a failed resource stop rather than a false
reclaim; ordinary client close also wakes detached response writers. This
repair turned the original Fusion panic-hang into bounded, attributable
failures while keeping plain WGPU as the supported receipt.

The fresh-process matrix passes local plain, local Fusion plus autotune, local
Fusion-only, local autotune-only, and remote plain. Remote Fusion plus autotune
executes numerically but leaves five live allocations after reclaim. Remote
autotune-only is timing-sensitive through first inference and cancellation;
remote Fusion-only does not complete the first remote provider load within the
120-second bound. Both optional axes are therefore remote-unsafe in this pinned
Burn/CubeCL graph, while local success excludes MiniLM or WGPU generally. See
[`2026-08-25_remote_minilm_sidequests.json`](../../../ports/distillery/probe/receipts/2026-08-25_remote_minilm_sidequests.json).
