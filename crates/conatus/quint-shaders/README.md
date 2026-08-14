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

## What the latest development says (checked 2026-08-13)

**rust-gpu v0.10.0-alpha.1**, released 2026-04-17, is the version that
closes the pincer *in principle*: it pins nightly-2026-04-11 (new
enough for edition 2024), is itself updated to the 2024 edition, and
explicitly supports glam 0.30, requiring at least `0.30.8`. So the
`rust_gpu::vector` attribute that 0.9 rejects is the *new* ABI, and
glam has been chasing 0.10 rather than drifting away from it. The fix
is upward, not backward.

Bumping this crate to `spirv-std 0.10.0-alpha.1` from crates.io was
tried the same day and does not build either: 112 errors inside
spirv-std's own `vec_trunc_impls` against glam. That is alpha churn in
the published crate rather than anything about these kernels; the
matching git rev, or the first non-alpha 0.10, is the next thing to
try.

**And there is nothing to unify.** rust-gpu is a *build-time*
toolchain and `.spv` is the artifact, so a shader crate compiled by
0.10 and renderling's compiled by 0.9 never meet: they share only the
device that loads their modules. Two rust-gpu versions in the
ecosystem is not the hazard two wgpu versions would be. This crate can
move alone, and should, as soon as a 0.10 builds.

The separate, larger question is renderling's own shaders, and the
answer there is *not yet*: upstream renderling still pins the same
0.9 rev (and wgpu 26, where our fork is already at 29). Migrating its
45 entry points to spirv-std 0.10 would put the fork further ahead of
upstream on a second axis with no reference to follow. The watch
condition is upstream moving; then the fork rebases and inherits the
migration rather than authoring it.

## What would clear it

For **this crate**, in order of preference: `spirv-std` from the
rust-gpu git 0.10 branch at a rev that builds; the first non-alpha
0.10 release; or 0.9 with glam pinned below `rust_gpu::vector` *and*
that pin honoured through spirv-std's own workspace requirement, which
needs a `[patch]` on the rust-gpu checkout rather than a dependency
line here. The check is cheap to repeat: bump the version, run the
build command below, and see.

For **renderling's shaders**, wait for upstream, and watch
`schell/renderling`'s workspace `spirv-std` pin. Doing it ourselves
means a 0.9-to-0.10 API migration across 45 entry points with no
reference implementation, on a fork already carrying a wgpu bump.
Worth it only when a renderling shader actually needs editing.

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
