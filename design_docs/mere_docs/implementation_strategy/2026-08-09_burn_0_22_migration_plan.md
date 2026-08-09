# Burn 0.22 Migration Plan

**Date**: 2026-08-09

**Status**: Release-gated. [`burn`](https://crates.io/crates/burn) and
[`burn-remote`](https://crates.io/crates/burn-remote) are available only as
`0.22.0-pre.1`; production migration waits for stable 0.22 unless Mark
explicitly reopens that gate. An isolated prerelease compatibility probe is
allowed.

**Related**:
[`../research/2026-07-04_burn_utilization_brief.md`](../research/2026-07-04_burn_utilization_brief.md),
[`2026-07-04_burn_wgpu_flip_plan.md`](2026-07-04_burn_wgpu_flip_plan.md),
[`2026-06-30_mesh_lease_scheduler_plan.md`](2026-06-30_mesh_lease_scheduler_plan.md),
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
- `mere-embed`: an optional dependency used only to name a backend in an
  integration test; and
- `mere-eidetic-search`: a dev dependency used by the recall example to name
  CPU/WGPU backend types.

The older docs' `aether` and Sibylla/Vates inventory is stale after crate and
ESP consolidation. The actual migration has two production roots, plus two
test/example leaks.

Burn 0.22.0-pre.1 declares Rust 1.95. The checkout currently uses Rust 1.97.1,
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

While only `0.22.0-pre.1` exists, a detached branch/worktree may answer bounded
questions:

- how much ESP and Quint source changes;
- whether WGPU and existing-device initialization still work;
- whether `burn-remote` can mount its iroh protocol on an application-owned
  Router; and
- how `RemoteTicket` and the authorizer callback map to a mesh lease reference.

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
