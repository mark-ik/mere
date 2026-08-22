# Mere patch provenance

This directory is the published `cubek-reduce 0.3.0-pre.2` crate, released from
[`tracel-ai/cubek` tag `v0.3.0-pre.2`](https://github.com/tracel-ai/cubek/releases/tag/v0.3.0-pre.2)
at commit `537e703885`.

Mere changes only the floating-point extrema identity materialization in
`src/components/instructions/extrema.rs`. Cubek constructs positive and
negative infinity from literal IEEE-754 bits. Browser WGSL validation evaluates
that bitcast as a constant and rejects the resulting non-finite `f32` before
dispatch. The patch preserves Cubek's infinity and NaN semantics while making
the bit pattern pass through a mutable runtime local.

The triggering identity code arrived in
[`tracel-ai/cubek#457`](https://github.com/tracel-ai/cubek/pull/457).
CubeCL previously fixed constant-bitcast materialization for its C++ dialects
in [`tracel-ai/cubecl#1477`](https://github.com/tracel-ai/cubecl/pull/1477),
but the associated issue stated that WGSL was unaffected. Distillery's
[`headed Chromium receipt`](../../../ports/distillery/probe/repros/cubek_browser_extrema/receipts/2026-08-22_patched_iab.json)
demonstrates the missing WGSL half and validates this backport across finite,
infinity, and NaN cases.

The normalized manifest also carries an empty workspace so its upstream test
suite can be run directly while the vendored directory remains outside Mere's
ordinary workspace members.

The independent headed reproducer is
`ports/distillery/probe/repros/cubek_browser_extrema`. Remove this patch when a
released Cubek row passes all four browser cases without it.
