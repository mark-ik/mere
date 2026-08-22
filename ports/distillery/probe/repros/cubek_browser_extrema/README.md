# Cubek browser extrema reproducer

This is the reduction-only extraction of Distillery's first failed MiniLM
browser row. It uses Burn 0.22.0-pre.2 and Cubek 0.3.0-pre.2 to run four scalar
extrema cases through BrowserWebGpu in a dedicated worker:

- a finite maximum;
- an all-negative-infinity maximum;
- an all-positive-infinity minimum; and
- a maximum containing NaN.

The page captures WebGPU validation errors separately from returned tensor
bytes. This distinction matters because a failed dispatch can still leave a
repeatable buffer that is not a valid result.

The released Cubek source constructs the floating-point reduction identities
by bitcasting literal infinity bits. Chromium constant-evaluates that expression
and rejects the non-finite `f32`. This workspace patches `cubek-reduce` to pass
the bits through a mutable kernel local before reinterpretation while retaining
the infinity and NaN cases that motivated Cubek's identity change.

From this directory, with wasm-bindgen CLI 0.2.122 installed:

```powershell
.\run-repro.ps1 -WasmBindgen C:\path\to\wasm-bindgen.exe
```

Open the printed URL in headed Chromium and choose **Run reduction cases**.
For automation, call `window.cubekExtremaRepro.run()` and inspect both
`result` and `gpu_errors`.

The checked-in lockfile enables Mere's candidate `cubek-reduce` patch. To
reproduce the released failure, remove that one patch entry from `Cargo.toml`,
regenerate `Cargo.lock`, and rebuild; leave the `cubecl-runtime` manifest patch
in place because it is the independent wasm packaging repair required by this
prerelease row.

[`UPSTREAM_ISSUE.md`](UPSTREAM_ISSUE.md) contains an issue-ready extraction of
the failure, the generated WGSL before and after materialization, and the exact
validation boundary.
