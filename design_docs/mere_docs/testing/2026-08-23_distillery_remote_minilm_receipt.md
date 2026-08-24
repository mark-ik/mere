# Distillery Remote MiniLM Receipt

**Date**: 2026-08-23

**Source**: clean detached Mere `176c31e8`; clean p2panda `9f2c2a01`.

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
WGPU-backed recovery. It does not measure physical GPU allocation release;
Burn Remote exposes session lifecycle rather than driver allocation counters.

The first forcing run used `burn-wgpu`'s default Fusion and autotune features.
MiniLM's fused matmul autotune exhausted every plan, then `burn-fusion` panicked
because its ordering contained two operations while the live stream contained
one. The device runner died and the client waited. The passing receipt therefore
uses plain native WGPU with Fusion and autotune disabled. That crash is a Burn
Fusion/remote-server compatibility sidequest, not a lease-adapter defect and
not acceptable cancellation behavior.
