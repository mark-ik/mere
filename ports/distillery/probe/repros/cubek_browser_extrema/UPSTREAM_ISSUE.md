# Released 0.11.0-pre.2 WGSL infinity bitcast breaks BrowserWebGpu extrema reductions

## Summary

`cubek-reduce 0.3.0-pre.2` constructs floating-point max/min identities by
reinterpreting literal IEEE-754 infinity bits. CubeCL's WGSL backend emits that
reinterpretation as a constant expression. Chromium rejects the resulting
non-finite `f32` during shader creation, so every affected reduction dispatch
fails before execution.

This is the WGSL counterpart to
[`tracel-ai/cubecl#1476`](https://github.com/tracel-ai/cubecl/issues/1476).
Its C++ code-generation fix landed in
[`tracel-ai/cubecl#1477`](https://github.com/tracel-ai/cubecl/pull/1477),
but the original report explicitly classified WGSL as unaffected.

CubeCL and Cubek main have since moved to Pliron lowering. Current WGSL output
materializes named `ConstantOp` values before `ReinterpretCastOp`, which may
already avoid this exact constant-expression path. The reduced harness should
therefore be rerun against main or the next release before filing. This report
documents the released prerelease row and a possible backport; it does not claim
that current main remains affected.

## Released row

- Burn: `0.22.0-pre.2`
- Cubek: `0.3.0-pre.2`
- CubeCL: `0.11.0-pre.2`
- wgpu: `30.0.0`
- wasm-bindgen libraries and CLI: `0.2.122`
- Browser: headed Chromium 151 on Windows
- Adapter: NVIDIA Lovelace

The triggering extrema identity implementation was introduced by
[`tracel-ai/cubek#457`](https://github.com/tracel-ai/cubek/pull/457).

## Browser error

The first MiniLM embedding dispatch produced:

```text
GPUValidationError
value -inf cannot be represented as 'f32'
reduce_kernel_in_f32_in_size_4_out_f32_out_size_1_acc_f32
```

The rejected WGSL contains:

```wgsl
let val_59 = bitcast<f32>(u32(4286578688));
```

The dispatch failure left a repeatable output allocation, but the first eight
float bit patterns were the tokenizer ids rather than embedding values. This is
why the reproducer records raw GPU validation errors separately from readback.

## Minimal reproducer

`ports/distillery/probe/repros/cubek_browser_extrema` removes the model,
tokenizer, storage, and ESP layers. It runs four one-dimensional Burn reductions
inside a Web Worker:

1. finite maximum;
2. all-negative-infinity maximum;
3. all-positive-infinity minimum; and
4. maximum containing NaN.

Run `run-repro.ps1`, open the printed URL in headed Chromium, and choose
**Run reduction cases**. The stable automation entry point is
`window.cubekExtremaRepro.run()`.

The committed manifest enables the candidate local `cubek-reduce` patch. Remove
only that patch entry and regenerate the lockfile to reproduce the released
failure; retain the separate `cubecl-runtime` prerelease manifest repair needed
for wasm compilation.

## Candidate materialization

Passing the bit pattern through a mutable Cube local changes the emitted WGSL
to a runtime load before the bitcast:

```wgsl
var val_59_store: u32;
let val_59 = &val_59_store;
*val_59 = u32(4286578688);
let val_61 = *val_59;
let val_60 = bitcast<f32>(val_61);
```

This preserves positive/negative infinity identities and Cubek's NaN behavior.
A general CubeCL fix should materialize constant `Operator::Reinterpret`
operands in the WGSL backend, parallel to the C++ `ensure_lvalue` fix, rather
than requiring each caller to add a mutable local.

## Validation to date

- The standalone reproducer builds for `wasm32-unknown-unknown`.
- Cubek's native WGPU `unit_infinite_identities_f32` test passes.
- Cubek's native WGPU `unit_parallel_f32` NaN/extrema test passes.
- The logged patched WGSL contains a runtime load between the literal bits and
  `bitcast<f32>`.
- Strict Clippy passes for the patched Cubek library and reproducer source.
- A post-patch headed Chromium 151 run passes all four finite, infinity, and NaN
  cases with empty GPU error scopes. The machine-readable result is committed
  as
  [`2026-08-22_patched_iab.json`](receipts/2026-08-22_patched_iab.json).

This validates the narrow backport against the released prerelease row. Current
Cubek main still needs the same reduced harness before an upstream report can
claim that the defect survives its Pliron lowering rewrite.
