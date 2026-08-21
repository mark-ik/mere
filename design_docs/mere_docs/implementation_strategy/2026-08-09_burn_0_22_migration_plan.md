# Burn 0.22 Migration Plan

**Date**: 2026-08-09

**Status: PRERELEASE MIGRATION EXECUTED 2026-08-20; stable closure remains
release-gated.** On the 0.22.0-pre.2 row Mark chose, both production roots
migrated (esp then quint, the plan's order), the vendored
`support/patches/cubecl-wgpu` backport retired with it exactly as the probe
predicted, and the workspace carries one `wgpu` 30.0.0 and one
`libsqlite3-sys` 0.38.2 (rusqlite behind CubeCL's autotune cache, as the probe
also predicted: the workspace is no longer sqlite-free, and that is CubeCL's
own persistence, not a crossing of mere's storage boundary).

The API shift was larger than "churn": 0.22 removes the backend type
parameter entirely. `Tensor<B, D>` is `Tensor<const D>`, every `B: Backend`
generic dies, and devices become runtime values (`Device::ndarray()`,
`Device::wgpu(DeviceKind)`) with type-erased dispatch. esp's model stacks
came out *simpler* (the generics were plumbing). Three findings worth keeping:

- **Fusion is a default of standalone `burn-wgpu`**, and quint's resident
  interop must build on the non-fused backend (`burn_wgpu::Wgpu`) or
  `from_primitive` sees a `FusionTensor` where it expects a `CubeTensor`.
  quint deps `burn-wgpu` with `default-features = false`.
- **The raw-tensor bridge survives** behind `burn/extension`:
  `Tensor::from_primitive::<burn_wgpu::Wgpu>(cube_tensor)` and
  `try_into_primitive` replace 0.21's `TensorPrimitive::Float` wrapping.
- **cubecl 0.11 launches slices, not `Array`**: kernel params become `&[T]` /
  `&mut [T]`, `ArrayArg` becomes `BufferArg`, and `SharedMemory::<f32>::new`
  becomes `Shared::<[f32]>::new_slice`.

Receipts, all on this machine's real GPU in release: quint parity 4 (repulse,
node exclusion, scalar and vector lowering), resident chunk receipt 3 and
resident 4 (the raw-buffer bridge), esp vector-kernel parity, BERT
ndarray/wgpu parity. CPU suites: esp 173, quint 63. Prior 0.21 numbers are
historical per the migration rules.

**Closure receipts 2026-08-20:** all eleven ESP feature combinations and all
five Quint feature combinations now compile for `wasm32-unknown-unknown`,
including every WGPU row and Quint's resident `field-gpu` row. The run found
two real prerelease packaging gaps: `cubecl-runtime 0.11.0-pre.2` omitted its
direct `wasm-bindgen-futures` dependency and left `cubecl-common`'s `serde` /
`hash` features desktop-only. `support/patches/cubecl-runtime` is the unchanged
published source plus the manifest corrections already made upstream in
CubeCL commits `bce4e489` and `7a2ee1c3`; delete it when a release containing
those fixes replaces this row. The same run found and fixed Quint's own missing
`getrandom/wasm_js` feature edge under `field-burn`.

The existing-device boundary is re-proven by the seven release-mode Quint
receipts inherited by this migration: the host opens the wgpu
adapter/device/queue, `ResidentClient::init` registers those handles with
Burn/CubeCL, four resident kernel receipts execute, and three chunk receipts
prove the Burn tensor and raw-kernel view resolve to the same allocation. The
current shared tree also passed a fourth chunk receipt from the concurrent
resident-patch lane; it is evidence, but not part of this migration's change
set. The earlier phrase "cross-device receipt" was imprecise: this plan requires
host-owned existing-device adoption, not remote or cross-machine execution. The
detailed command ledger is in the
[prerelease closure receipt](../testing/2026-08-20_burn_0_22_prerelease_closure.md).
The same closure passed `cargo package -p esp` from a detached clean worktree;
Cargo built and verified the extracted package. Repeat that receipt after the
stable repin because dependency packaging is part of the release gate.

Stable Burn 0.22 is still unpublished as of this recheck. Publication and the
Burn Remote adapter therefore remain closed even though the chosen prerelease
implementation and portability evidence are green.

Original doc follows.


**Status**: Release-gated on stable 0.22 only. The second reason (the sqlite
conflict) cleared on 2026-08-16, verified by probe; see below. Original note
follows.

**Original status**: Release-gated, and the gate now has a second reason. [`burn`](https://crates.io/crates/burn)
and [`burn-remote`](https://crates.io/crates/burn-remote) are still only
`0.22.0-pre.2` as of 2026-08-16; production migration waits for stable 0.22
unless Mark explicitly reopens that gate. An isolated prerelease compatibility
probe is allowed.

**2026-08-16: Mark reopened the gate, and the prerelease turned out to be
unreachable anyway.** During the stack-wide wgpu 30 unification the gate was
put to him explicitly and he chose the prerelease row. Attempting it produced a
hard resolution failure that is nothing to do with release stability:

- on `cfg(any(windows, linux, macos, android))`, `cubecl-runtime 0.11.0-pre.2`
  depends on `cubecl-environment` with the `cache` feature **non-optionally**
  (a persistent autotune cache), which pulls `rusqlite ^0.40` and thus
  `libsqlite3-sys ^0.38`;
- `p2panda-store`'s `groups` and `encryption` features, which `gemot` needs,
  force its `sqlite` feature, which pulls `sqlx` and thus
  `libsqlite3-sys >=0.30.1, <0.38` — and no released sqlx, including 0.9.0,
  reaches 0.38;
- `libsqlite3-sys` declares `links = "sqlite3"`, so exactly one may exist in a
  graph.

Version selection cannot resolve that. The workspace took **the narrow CubeCL
backport instead**: `cubecl-wgpu` is the only crate in the whole burn 0.21 /
cubecl 0.10 stack that names wgpu, so it is vendored at
`support/patches/cubecl-wgpu` and moved to wgpu 30 (three call sites). Burn
stays at stable 0.21, which is also what this plan's own stop rule wanted.

This plan's B2/B3 work is therefore still pending and still release-gated. When
stable 0.22 lands, the sqlite conflict above must be re-checked before B0
starts: it is an independent blocker that will not clear just because the
release does.

**2026-08-16 (later the same day): the sqlite blocker is CLEARED, and it did
not take a release to clear it.** The re-check this paragraph asks for was run
as an isolated prerelease probe, and the conflict is gone from our side. The
[address book muniment port](../../archive_docs/2026-08-20_completed_plans/2026-08-16_address_book_muniment_port_plan.md)
took sqlx out of the workspace entirely, so `libsqlite3-sys` has no second
claimant. Note also that the diagnosis above was partly wrong: `p2panda-store`'s
`groups`/`encryption` features were never enabled here at all. The real enablers
were `p2panda-net/address_book` and `p2panda-store/default` via p2panda-sync and
p2panda-stream.

Probe result, with `burn = "0.22.0-pre.2"` on both roots and the vendored
`cubecl-wgpu` patch disabled:

- the workspace resolves (exit 0);
- with `esp/index-burn-wgpu` and `quint/field-burn-wgpu` actually enabled, the
  graph carries **exactly one `wgpu`, 30.0.0**, and **exactly one
  `libsqlite3-sys`, 0.38.2** (from `rusqlite`, via `cubecl-environment`'s
  still-non-optional `cache` feature);
- `cubecl-wgpu` comes from the registry at `0.11.0-pre.2`, so **the vendored
  backport at `support/patches/cubecl-wgpu` is no longer needed on this row**.

So the backport retires *with* this migration, not before it: dropping the patch
while staying on burn 0.21 would put wgpu 29 back in the graph. And the
migration is real code work, not a manifest bump. `cargo check -p quint
--features field-burn-wgpu` against 0.22.0-pre.2 fails on API churn:
`burn::backend` and `burn::tensor::backend` have moved, and `Tensor`'s rank
parameter changed (`type provided when a constant was expected`). That is B2/B3.

One thing the probe changes about the sqlite story: adopting 0.22 **reintroduces
an embedded SQLite** to the graph, as `rusqlite` behind CubeCL's autotune cache.
That is CubeCL's own persistence and does not cross mere's storage boundary, but
the workspace should not be described as sqlite-free afterwards.

The probe was reverted; the tree is back on burn 0.21 with the patch in place.

**Related**:
[`../research/2026-07-04_burn_utilization_brief.md`](../research/2026-07-04_burn_utilization_brief.md),
[`2026-07-04_burn_wgpu_flip_plan.md`](2026-07-04_burn_wgpu_flip_plan.md),
[`2026-06-30_mesh_lease_scheduler_plan.md`](../../archive_docs/2026-08-09_completed_plans/2026-06-30_mesh_lease_scheduler_plan.md),
[`2026-08-09_browser_model_ceiling_probe_plan.md`](2026-08-09_browser_model_ceiling_probe_plan.md),
[`2026-08-08_esp_consolidation_plan.md`](2026-08-08_esp_consolidation_plan.md)

This plan moves Mere from Burn 0.21 to stable 0.22 without combining the
dependency migration with remote execution, model-session design, or new
product behavior. Burn Remote becomes a later adapter only after M2/M3 prove
the resource and owner-reclaim seams.

---

## 1. Current dependency boundary

The live workspace has four direct Burn dependents:

- `esp`: the production BERT, vector-kernel, and llama-family model boundary;
- `quint`: the production field/tensor lowering boundary;
- ~~`mere-embed`~~: **removed 2026-08-10 (B1)**; and
- ~~`mere-eidetic-search`~~: **removed 2026-08-10 (B1)**.

The older docs' `aether` and Sibylla/Vates inventory is stale after crate and
ESP consolidation. The actual migration has two production roots, plus two
test/example leaks.

Burn 0.22.0-pre.2 declares Rust 1.95. The checkout currently uses Rust 1.97.1,
so MSRV is not the active gate. Release stability and backend/device API churn
are.

---

## 2. Migration rules

- Preserve model math, public ESP contracts, feature names, default dependency
  floor, and host ownership.
- Migrate one production Burn boundary at a time: ESP first, Quint second.
- Keep CPU correctness anchors while WGPU APIs move.
- Re-run native, wasm, and real-device receipts; prior 0.21 numbers become
  historical rather than silently carrying forward.
- Do not enable Burn Remote default features as a side effect. Its default
  includes client, server, iroh, and websocket.
- Do not expose a Burn 0.22 type from another Mere crate merely to make a test
  compile.
- Do not publish ESP or a compatibility shim from a prerelease migration.

Any required source fork or patch must name the upstream commit, reason, removal
condition, and license. A patch is a temporary compatibility fact rather than a
new Mere-owned backend.

---

## 3. B0: stable-release audit and baseline

When stable 0.22 appears:

1. Record exact versions and feature graphs for `burn`, WGPU support, tokenizer
   dependencies, and `burn-remote`.
2. Read the official migration notes and diff the used APIs: `Backend`,
   `BackendTypes`, `NdArray`, `Wgpu`, device construction/registration, tensor
   creation/readback, module parameter loading, and safetensors paths.
3. Confirm Burn's WGPU dependency still aligns with the workspace's wgpu stack
   or document the isolation boundary.
4. Re-run the 0.21 ESP feature/target matrix, CPU suite, real-WGPU parity, Quint
   tests, and representative performance commands as the baseline.
5. Capture `cargo tree` for every direct dependent and the default ESP tree.

Items 4-5 were captured **ahead of the release** on 2026-08-10, so nothing has
to be reconstructed once APIs have moved:
[Burn 0.21 baseline](../testing/2026-08-10_burn_0_21_baseline.md). It also
records what could *not* be run — the model-backed receipts, because
`MERE_MINILM_DIR` is unset on this machine — so the 0.22 comparison does not
mistake a never-green receipt for a regression. **The migration needs the
MiniLM and TinyLlama checkpoints present**: without them the suites prove
loading and shape, not arithmetic.

Stop if stable 0.22 removes a required browser or existing-device capability.
That becomes an upstream/fork decision before any manifest-wide bump.

---

## 4. B1: reduce accidental dependency fan-out

Before changing versions, remove direct Burn dependencies that exist only so a
consumer can spell an ESP backend type.

Prefer concrete ESP-owned CPU/WGPU construction functions returning the
provider seam over public re-exports of arbitrary Burn modules. Apply them to:

- `mere-embed/tests/bert_full_pipeline.rs`; and
- `mere-eidetic-search/examples/eidetic-recall.rs`.

The exact constructor must be proven by both existing consumers before it is
promoted. If the generic backend API remains materially useful outside tests,
keep the dependency and record why; do not invent a facade solely to reduce a
dependency count.

Done when `cargo metadata` shows only intentional production Burn boundaries,
or each remaining leaf dependency has a concrete consumer justification.

**Done 2026-08-10.** `cargo metadata` now shows two direct dependents, both
production: `esp` and `quint`. There were **three** consumers naming a Burn
type, not two — the third was `mere-embed`'s own lib doctest, which compiles
under `cargo test` and held the dependency open just as firmly. Replaced with
`esp::embed::bert::{load_cpu, load_wgpu, from_bytes_cpu}` returning
`Box<dyn EmbeddingProvider>`, plus `impl EmbeddingProvider for Box<dyn
EmbeddingProvider>` — without which a boxed provider cannot satisfy a generic
bound and a runtime backend choice still needs a Burn type. `BertEmbeddingProvider<B>`
stays public and justified: the constructors take the backend's *default*
device, and a host that must register an existing `wgpu` queue needs the generic
API. Full detail and the 0.21 numbers are in the
[baseline receipt](../testing/2026-08-10_burn_0_21_baseline.md).

---

## 5. B2: migrate ESP

Migrate ESP's shared Burn dependency and all three model families together so
one published crate never contains mixed Burn generations:

- `esp::embed::bert`;
- `esp::embed::index_burn`; and
- `esp::infer::decoder`.

Required receipts:

- empty/default dependency tree remains free of Burn and tokenizers;
- every ESP feature compiles individually on native and wasm;
- combined CPU and browser-WGPU feature sets compile;
- the merged CPU unit suite and Eidetic corridor pass;
- decoder, index, and BERT CPU/WGPU parity pass on real hardware;
- the real TinyLlama checkpoint produces reference-valid output and refreshed
  throughput numbers; and
- `cargo package -p esp` verifies the extracted package without relying on
  workspace patches.

Device initialization deserves its own receipt. Verify both ESP-owned device
creation and any existing-device registration used by a host. A compile through
generic `Wgpu` types is insufficient evidence for interop or queue policy.

No ESP release occurs until Quint and workspace consumers are known to resolve
one compatible Burn graph.

---

## 6. B3: migrate Quint

Quint is an independent, legitimate Burn boundary. Migrate its field lowering
after ESP is green rather than mixing model and field failures.

Required receipts:

- scalar/vector lowering and analytic-gradient tests;
- NdArray/WGPU parity;
- native and wasm feature checks;
- real-device field execution; and
- refreshed representative performance numbers, preserving the CPU-default
  conclusion unless resident/heavier workloads change it.

Gyre and other Burn-free consumers must remain Burn-free. The closure/field
seams survive the migration unchanged.

---

## 7. B4: workspace and publication closure

After both production roots pass:

- update `mere-embed` and Eidetic example consumers;
- inspect the workspace for mixed 0.21/0.22 graphs and accidental remote or
  training features;
- run the ESP package verification from a clean commit;
- update the feature/target matrix and old Burn footprint docs; and
- publish a semver-appropriate ESP release only with explicit authorization.

The deprecated Vates and Sibylla shims receive a version bump only if their ESP
requirement must change. They contain no independent Burn ownership.

---

## 8. Optional prerelease compatibility probe

While only 0.22 prereleases exist, a detached branch/worktree may answer
bounded questions against the newest prerelease:

- how much ESP and Quint source changes;
- whether WGPU and existing-device initialization still work;
- whether `burn-remote` can mount its iroh protocol on an application-owned
  Router; and
- how the authorizer callback maps to a mesh lease reference. (Answered on
  2026-08-10 without needing the probe, by reading the 0.22.0-pre.1 source:
  see [lease-bound remote sessions](../technical_architecture/2026-08-10_lease_bound_remote_sessions.md).
  Note that **`RemoteTicket` does not exist** — this plan and the host lanes
  plan both carried that name in error.)

This probe may use a narrow temporary patch. It must not merge into main,
publish crates, claim stable compatibility, or implement a second scheduler.
Keep only a short diff/receipt if the result changes the stable migration plan.

Burn Remote execution itself is outside this migration. It starts only after:

1. M2 has a real resource registry and namespace receipt;
2. M3 has cooperative cancellation and owner reclaim;
3. stable Burn 0.22 is migrated; and
4. the remote adapter can reuse the shared murm iroh endpoint without owning
   job authorization or lease policy.

---

## 9. Non-goals and stop rules

This plan does not add LoRA adapters, `ModelSession`, training, endpoint
inference, mesh economics, or browser product defaults.

Stop on any of these conditions:

- stable 0.22 is not published;
- a required feature exists only behind incompatible WGPU generations;
- model output/parity changes without an explained upstream numerical change;
- default ESP pulls Burn or remote dependencies; or
- the migration requires ESP to own rendering/device policy.

## 10. Done conditions

- One stable Burn 0.22 generation remains in the relevant workspace graph.
- ESP and Quint pass their native, wasm, CPU/WGPU, and real-device receipts.
- Existing-device and browser feature boundaries are re-proven.
- Test/example-only Burn dependencies are removed or justified.
- ESP packages from a clean commit.
- Burn Remote remains a separately gated resource adapter.
- All refreshed performance claims name the version and hardware.

## 11. Progress

- **2026-08-09**: scoped from the completed ESP consolidation ledger. Verified
  the registry still exposes only Burn/Burn Remote `0.22.0-pre.1`, the local
  toolchain satisfies the advertised Rust requirement, and the live dependency
  surface is ESP plus Quint with two test/example leaks. Separated stable
  migration, accidental-fan-out cleanup, and an optional disposable prerelease
  probe from the later Burn Remote adapter.
- **2026-08-12**: registry recheck found Burn and Burn Remote
  `0.22.0-pre.2`, still with no stable 0.22. The release gate is unchanged.
  Remote API claims elsewhere in the lane remain explicitly sourced to pre.1
  and need a source recheck against the chosen migration version.
