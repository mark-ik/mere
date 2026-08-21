# Burn 0.22 prerelease closure receipt

**Date:** 2026-08-20

**Status:** The explicitly chosen `0.22.0-pre.2` migration is green on native,
wasm compile, real WGPU, host-owned existing-device adoption, and clean
extracted-package verification. Stable 0.22 is still unpublished, so
publication and Burn Remote remain gated.

## Dependency repair

The first WGPU wasm check failed inside the published
`cubecl-runtime 0.11.0-pre.2`:

```text
error[E0433]: cannot find module or crate `wasm_bindgen_futures`
cubecl-runtime/src/tune/tuner.rs:485
```

The crate calls `wasm_bindgen_futures::spawn_local` on wasm but its published
manifest omitted the direct dependency. The next upstream checks also require
`cubecl-common`'s `serde` and `hash` features outside desktop targets. CubeCL
already fixed both facts in commits `bce4e489` and `7a2ee1c3`.

`support/patches/cubecl-runtime` is the registry source for exactly
`0.11.0-pre.2`. Only its Cargo manifests differ: they add the direct wasm
dependency and make the two required `cubecl-common` features unconditional.
The root patch records those commits and its deletion condition. A one-crate
Git overlay was rejected because Cargo followed the newer crate's path edges
into six newer CubeCL internals and split the prerelease graph.

The run also found a Mere-owned gap: Quint's `field-burn` feature reached
`getrandom 0.4` on wasm without activating `wasm_js`. `field-burn` now owns that
feature edge just as the WGPU extensions already did.

## Wasm compile matrix

Target: `wasm32-unknown-unknown`. Every configuration was checked separately
with `--no-default-features`.

ESP passed eleven rows:

- default and `actor`;
- `decoder` and `decoder-wgpu`;
- `index-burn` and `index-burn-wgpu`;
- `bert`, `bert-wgpu`, and `bert-validation`;
- combined CPU models: `decoder,index-burn,bert,bert-validation`;
- combined browser WGPU models:
  `decoder-wgpu,index-burn-wgpu,bert-wgpu,bert-validation`.

Quint passed five rows:

- default;
- `field-burn`;
- `field-burn-wgpu`;
- resident `field-gpu`;
- `field-rhai`.

These are compile receipts. Headed browser model execution remains the D2
browser-ceiling lane.

## Existing-device execution

Command:

```text
cargo test -p quint --release --features field-gpu \
  --test resident --test resident_chunk -- --nocapture
```

Result in the current shared tree: 8 passed, 0 failed, 0 ignored. The run did
not take either test's "no wgpu adapter" return path. Seven tests are the
existing migration surface; the fourth resident-chunk test belongs to a
concurrent stamped-patch lane and is not included in this closure commit.

- Four resident tests executed Quint-authored CubeCL kernels for force-law,
  settling, springs, and stamped position leases.
- Four resident-chunk tests proved Burn and raw views share the same CubeCL /
  wgpu allocation, exact integer planes stay exact, exported lease sizing is
  sound, and committed patches retain and restamp the allocation.

This is the migration's existing-device receipt: the test host creates the
adapter/device/queue, then Quint registers those exact handles with Burn and
CubeCL. It does not claim remote or cross-machine execution.

## Clean package receipt

From a detached clean worktree at the closure commit:

```text
cargo package -p esp
Packaged 55 files, 528.7KiB (138.6KiB compressed)
Finished `dev` profile ...
```

Cargo verified the extracted `esp 0.1.0` package rather than compiling the
workspace source in place.

## Remaining gate

The official Burn release list still ends at `v0.22.0-pre.2`. Stable repinning,
the stable-row package rerun, and Burn Remote integration remain serial work
after stable 0.22 is published. The temporary CubeCL runtime patch should be
deleted during that repin if the release contains the upstream fixes above.
