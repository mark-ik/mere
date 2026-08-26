# Burn 0.22 prerelease closure receipt

**Date:** 2026-08-20

**Status:** The explicitly chosen `0.22.0-pre.2` migration remains the production
row and is green on native,
wasm compile, real WGPU, host-owned existing-device adoption, and clean
extracted-package verification. Burn `0.22.0-pre.3` is now published and has
passed the bounded source/package checks below; this is an audit, not a
production repin. Stable 0.22 is still unpublished, so publication remains
gated.

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

The production row remains `0.22.0-pre.2` until a separately authorized stable
repin. The temporary CubeCL runtime patch should be deleted at repin because
pre.3 contains the two upstream packaging fixes above. The Mere-owned
same-allocation `burn-cubecl` patch and targeted `burn-remote` lifecycle patch
still need a source rebase and fresh receipts on whichever row is selected.

## 2026-08-26 pre.3 compatibility audit

The crates.io API reports the exact prerelease packages needed by the current
roots: `burn`, `burn-remote`, and `burn-cubecl` `0.22.0-pre.3`, plus
`cubecl`, `cubecl-runtime` `0.11.0-pre.3`, and `cubek` `0.3.0-pre.3`. The
published Burn source is the `v0.22.0-pre.3` release. The package is named
`burn-pack`, not `burnpack`.

An isolated ESP source probe copied the crate, removed only unrelated local
workspace/path dependencies and feature rows, then ran:

```text
cargo check --manifest-path C:\t\esp-pre3-probe-0826\Cargo.toml \
  --features "bert-validation bert-wgpu decoder-wgpu index-burn-wgpu" -j 1
Finished `dev` profile [unoptimized + debuginfo] target(s) in 16m 08s
```

The selected ESP model/WGPU rows compiled through Burn, burn-wgpu,
burn-cubecl, cubecl-runtime, burn-ir, burn-pack, and the pre.3 CubeCL graph.
This is a bounded source-compatibility receipt. It is not a production
manifest or full feature-matrix pass, and the temporary pins were reverted.

The corresponding temporary `cargo check -p distillery --features remote
-j 1` could not reach rustc: Cargo stopped while updating the shared `genet`
checkout because another process held its packfile. Until a clean Distillery
workspace check completes, Distillery pre.3 source compatibility remains
unverified. A dependency-only probe was not completed and contributes no
receipt.

Patch disposition against the pre.3 published sources:

| Area | Result | Disposition |
| --- | --- | --- |
| `cubecl-runtime` wasm manifest fixes | Direct `wasm-bindgen-futures` and unconditional `cubecl-common` serde/hash requirements are present upstream | Remove the packaging patch at repin; rebase only Mere identity helpers if still required |
| `burn-cubecl` same-allocation binary launch | pre.3 has alias launch helpers but still chooses the mutable broadcast path before checking shared allocation | Keep and rebase the Mere patch; require headed same-allocation and LayerNorm receipts |
| `burn-remote` lifecycle | pre.3 exposes service close but `SessionManager::close` still only removes the map entry; no targeted pump drain/session reservation API is upstream | Keep and rebase the Mere lease-bound close patch |
| Remote Fusion/autotune | No pre.3 source evidence removes the existing remote allocation/ordering hazards; the pre.2 plain-WGPU result remains the supported lane | Keep Fusion/autotune out of production remote defaults; rerun only as a separately gated sidequest |

Pre.3 has concrete follow-up value for later work: the release notes include
LoRA persistent-allocation and explicit-dtype fixes, dtype preservation during
composition, and on-demand Burnpack tensor streaming. These justify a fresh
native LoRA receipt and a bounded `burn-pack` streaming probe when portable
checkpoint export becomes an active requirement. They do not change the
current ModelSession/ordinary PEFT claim or authorize a production repin.
