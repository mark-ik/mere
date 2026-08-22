# BrowserWebGpu silently corrupts binary ops with the same tensor as both inputs

## Summary

On Burn `0.22.0-pre.2` / CubeCL `0.11.0-pre.2`, BrowserWebGpu returns stale or
unmodified storage when a binary tensor operation receives two logical handles
to the same allocation. No WebGPU validation error is reported.

This breaks Burn LayerNorm because its variance path evaluates
`centered.clone() * centered`. The exact Burn LayerNorm unit input and an
`8 x 384` BERT-width input both return the original input unchanged in headed
Chromium 151 on Windows/NVIDIA Lovelace. Native WGPU passes.

Burn main at `2c06be351006851d42fdb6df16b976f4a82d9747` and CubeCL main at
`4057f39e7fe025323e032bab8fc7600d16414f5e` were source-audited on 2026-08-22;
the same launch and handle-count paths remain. Those main revisions were not
runtime-tested by this receipt.

## Minimal discriminator

```rust
let input = Tensor::<2>::from_data(
    TensorData::new(vec![-0.6897_f32, -2.7106, 2.2222, -1.0330], [1, 4]),
    &device,
);

let shared = input.clone() * input;
// BrowserWebGpu: incorrect.

let lhs = Tensor::<2>::from_data(data.clone(), &device);
let rhs = Tensor::<2>::from_data(data, &device);
let independent = lhs * rhs;
// BrowserWebGpu: correct.
```

Scalar multiplication also passes. The failure therefore is not generic
multiplication, upload, reduction, output allocation, model size, or queued
work.

## Observed results

- Eleven embedding controls pass, including exact MiniLM table geometry and
  model-sized upload pressure.
- `tensor.clone() * tensor` fails with maximum absolute error `7.347352`.
- Two independent, equal uploads multiply exactly.
- Burn's ten-value LayerNorm returns its input, maximum error `0.7426002`.
- BERT-width LayerNorm returns its input, maximum error `0.50517905`.
- GPU error scopes are empty.
- The exact 90,868,376-byte MiniLM artifact passes the native WGPU fixture.

The machine-readable before/after result is
[`receipts/2026-08-22_binary_alias_iab.json`](receipts/2026-08-22_binary_alias_iab.json).

## Cause and tested fix shape

`burn-cubecl/src/kernel/binary.rs::launch_binop` binds `lhs` and `rhs` as two
inputs. When both are clones of one tensor, both bindings point to the same
logical allocation. The existing output-alias branch is not the cause:
disabling in-place output selection and allocating a distinct output still
fails.

A tested fix does three things:

1. expose whether two CubeCL handles name the same logical allocation and view;
2. bind that allocation once;
3. pass the second logical tensor argument as an alias of input zero, with a
   distinct output allocation.

That form passes shared add/multiply, variance, exact LayerNorm, BERT-width
LayerNorm, and all embedding controls in headed Chromium with empty error
scopes. Mere carries the experiment as a temporary backport in
`support/patches/burn-cubecl` and `support/patches/cubecl-runtime`.

The same guard was applied to Burn's numeric, integer, and float binary
launchers. The receipt exercises the numeric path; upstream should add backend
coverage for the integer and float-family launchers as well.
