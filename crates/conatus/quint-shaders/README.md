# quint-shaders

The resident lane's kernels in Rust, compiled to SPIR-V by rust-gpu.
`src/lib.rs` is the source of truth for what the GPU runs: repulsion,
springs, and integration, written so the force law reads the same way
it does in `quint::forces`.

**The carriage works.** `shaders/quint_shaders.spv` in `quint` is built
from this crate and committed, so a consumer needs no rust-gpu
toolchain; `quint::resident` loads it through `PASSTHROUGH_SHADERS`
where the adapter allows, and falls back to the equivalent
`shaders/resident.wgsl` where it does not (browsers have no SPIR-V
ingestion, and passthrough is a feature an adapter may lack). Both
paths are checked against `forces::repulsion_reference`, and
`the_committed_spirv_is_what_runs_where_the_adapter_allows_it` asserts
the artifact actually executes rather than passing on the fallback.

## Building

```
cargo gpu build --shader-crate . --output-dir ../quint/shaders   --auto-install-rust-toolchain   --spirv-builder-source C:/Users/mark_/Code/crates/rust-gpu
```

The source is the **local rust-gpu fork**, one commit ahead of upstream
main, and `cargo-gpu` is likewise installed from a local clone pointed
at that fork (`cargo install --path crates/cargo-gpu`, with its
`spirv-builder` dependency repointed). Both are needed because the fix
lives in `rustc_codegen_spirv-types`, which cargo-gpu links directly.
`rust-toolchain.toml` here pins the nightly rust-gpu itself pins.

## The fork: one line, and an upstream bug

rust-gpu main fails against the toolchain its own `rust-toolchain.toml`
pins. Target-spec selection reads

    if rustc_version >= Version::new(1, 97, 0)

and the pinned nightly reports `1.97.0-nightly`. semver sorts a
pre-release *before* its release, so `1.97.0-nightly < 1.97.0`, the
older spec variant is chosen, and it emits `allows-weak-linkage` —
exactly the key rustc 1.97 removed. The build dies in `rustc --print`
with *unknown field `allows-weak-linkage`*.

The fork (`Code/crates/rust-gpu`, branch `mark-ik/prerelease-version-gate`)
compares on the version triple alone, which is what every gate in that
function means. The window is one release wide: later nightlies
(`1.98.0-nightly`) already pass, because the triple comparison
dominates. **Worth reporting upstream**; when it lands, the fork
retires and the build command points back at the git source.

## What this cost, and what it did not

Three false trails, recorded so they are not walked again:

- `spirv-std` from crates.io drags a modern `libm` whose 128-bit
  integer math cannot lower to SPIR-V (439 missing-intrinsic errors).
- The published `spirv-std 0.10.0-alpha.1` fails differently: 112
  errors inside its own `vec_trunc_impls`. Use the git source.
- rust-gpu at the fork family's old rev (`05b34493`) admits
  `glam <=0.30` while current glam emits a `rust_gpu::vector`
  attribute that backend rejects. The fix was upward, not a pin
  backward.

Two kernel-side adjustments were needed for the SPIR-V target, both
sensible on their own terms: `f32::sqrt` is not in core there, so the
kernels use `spirv_std::num_traits::Float`; and the settle reduction's
atomic uses `QueueFamily` scope rather than `Device`, since everything
touching that word is one dispatch on one queue and `Device` scope
under the Vulkan memory model needs a capability this target does not
declare.

**And there is still nothing to unify.** rust-gpu is a *build-time*
toolchain and `.spv` is the artifact, so this crate compiled by the
fork and renderling's shaders compiled by 0.9 never meet: they share
only the device that loads their modules. Two rust-gpu versions is not
the hazard two wgpu versions would be.

The separate question is renderling's own shaders. Upstream renderling
still pins the old 0.9 rev (and wgpu 26, where our fork is already at
29), and its 45 committed `.spv` were last built upstream 2026-03-22,
so the fork cannot rebuild them either. That carriage stays inherited
rather than reproducible until upstream moves — fine while nobody edits
a renderling shader. The fork above would likely unblock that too, but
migrating 45 entry points from spirv-std 0.9 to 0.10 is a separate job
with no reference to follow, and worth doing only when a renderling
shader actually needs editing.
