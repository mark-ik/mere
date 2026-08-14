# quint-shaders

The resident lane's kernels in Rust, for compilation to SPIR-V by
rust-gpu. `src/lib.rs` is the intended source of truth for what the GPU
runs: repulsion, springs, and integration, written so the force law
reads the same way it does in `quint::forces`.

**What ships today is `quint/shaders/resident.wgsl`, not a `.spv` built
from here.** The two are line-for-line equivalent, with matching entry
points and bindings, so swapping the shader module is the only change
the promotion needs once the blocker below clears. That equivalence is
checked the only way that means anything: `quint`'s resident receipt
compares the running kernel against `forces::repulsion_reference`, the
burn-free CPU anchor.

## The blocker (2026-08-13)

The rust-gpu carriage could not be completed on this machine, and the
reason is worth recording because it reaches further than this crate.

`cargo-gpu` installs and runs (from git; the crates.io `cargo-gpu
0.1.0` is a placeholder that prints "Coming Soon"). The wall is a
version pincer between the codegen backend and the ecosystem:

- **rust-gpu at the fork family's rev** (`05b34493`, the one
  `renderling` and every wing probe pin) declares
  `glam = ">=0.22, <=0.30"` in its own workspace. Current `glam
  0.30.10` satisfies that range and emits `#[rust_gpu::vector::v1]`,
  which this backend rejects: *unknown `rust_gpu` attribute, expected
  `rust_gpu::spirv`*, 41 errors before it stops. Pinning glam in the
  shader crate does not help, because spirv-std resolves through
  rust-gpu's own workspace requirement.
- **rust-gpu at cargo-gpu's default rev** (`877bd869`) installs a
  nightly too old to parse `edition = "2024"` manifests, which the
  shared crates.io registry is now full of, so resolution fails before
  compilation starts.
- Taking `spirv-std` from crates.io instead pulls a modern `libm` whose
  128-bit integer math cannot lower to SPIR-V: 439 missing-intrinsic
  errors.

**The finding that matters beyond this crate:** `renderling`'s own 45
committed `.spv` were last built upstream on 2026-03-22, and its
lockfile pins `glam 0.30.10`. So the fork cannot rebuild its own
shaders here either. The rust-gpu carriage in the wing is *inherited*,
not reproducible: fine while nobody edits a renderling shader, and a
wall the day someone does.

## What would clear it

Any one of: a rust-gpu rev whose codegen accepts current glam; a glam
version pinned below the `rust_gpu::vector` attribute *and* honoured
through spirv-std's own workspace requirement (a `[patch]` on the
rust-gpu checkout would do it); or a newer rust-gpu whose toolchain
parses edition 2024. This is upstream drift rather than anything about
these kernels, so the check is cheap to repeat later.

## Building, when it clears

```
cargo gpu build --shader-crate . --output-dir ../quint/shaders \
  --auto-install-rust-toolchain \
  --spirv-builder-source https://github.com/Rust-GPU/rust-gpu \
  --spirv-builder-version <a rev that compiles current glam>
```

Then point `quint::resident`'s shader module at the produced `.spv` via
`wgpu::util::make_spirv`, commit the artifact, and keep this crate as
the source it was built from. That is the renderling carriage: the
artifact travels, the toolchain does not.
