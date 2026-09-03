# Autodiff LoRA trainer plan

**Status (2026-09-02):** in progress. Assessment complete; Mark ruled D1–D3
on 2026-09-02, each on the recommended option; Phase 1 landed 2026-09-03,
Phase 2 is under way. Follow-on to the
[distillery v0 plan](2026-08-12_distillery_v0_plan.md) (§9 trainer forcing,
and the 2026-09-02 discrete-GPU trainer entry) and the
[FLORA, Tulpa, and Standing plan](../../moothold_docs/implementation_strategy/2026-08-31_flora_tulpa_standing_plan.md).

## Scope

Replace the v0 trainer's central finite differences with real gradients from
Burn's autodiff, keeping every byte contract the stack already relies on:

```
data-owning peer
  -> autodiff adapter trainer (this plan)
  -> canonical PEFT adapter plus training/evaluation receipt   (unchanged shape)
  -> versioned aggregation provider (exact FLoRA stacking, unchanged)
  -> Tulpa adoption
  -> ESP ModelSession composition (unchanged loader)
```

In: gradient computation, the optimizer, the settings that name them, the
Distillery request that carries them, and the Djinn lane that composes them
on CPU and on the discrete GPU. Out: every alternative federated method
(FFA-LoRA, LoRA-A², FedSA, FedDPA/FedALT, FlexLoRA, Te-LoRA, RAFFT,
FedKSeed/Ferret), Burn Remote, Burn collectives, and compression. Those stay
separate providers over the trainer this plan lands, as the 2026-08-31
handoff ruled.

## Findings (verified 2026-09-02 against `origin/main` `3ce750f5`)

- **ESP's decoder is on Burn's dispatch backend, not a `B: Backend`
  generic.** `crates/intel/esp/src/infer/decoder/*` uses
  `burn::tensor::{Tensor<D>, Device}`; the backend is chosen per call site
  by the device (`Device::ndarray()`, `Device::wgpu(..)`). There is no
  `AutodiffBackend` bound anywhere in the crate.
- **Autodiff arrives by wrapping the device.** `burn-dispatch 0.22.0-pre.2`
  provides `DispatchDevice::autodiff(inner)` behind the `autodiff` feature,
  `Dispatch` implements `AutodiffBackend`, and `burn-tensor` exposes
  `Tensor::backward() -> Gradients`, `Tensor::grad(&Gradients)`,
  `require_grad`, `inner`, `from_inner`. The GPU path is
  `Device::autodiff(Device::wgpu(DiscreteGpu(0)))`. So the handoff's
  `BurnAdapterTrainer<B: AutodiffBackend>` becomes a device-carried trainer
  with the same signature family as `train_peft_lora`; nothing in ESP grows a
  backend type parameter.
- **`Param::from_tensor` panics on a composed weight; it does not merely
  over-track.** (Corrected 2026-09-03 in Phase 1.) It calls `require_grad()`,
  and burn's autodiff refuses that on any non-leaf ("Can't convert a non leaf
  tensor into a tracked tensor"); `base + A^T B^T` is exactly such a tensor,
  so `DecoderModel::from_loaded` could not build a training model at all, and
  the decoder's fields are private. The fix lives in the decoder's three
  weight-adoption helpers (`attention::adopt_param`, `Param::initialized`
  without the re-mark), which is inference-inert: nothing in ESP optimizes a
  `DecoderModel`, and `require_grad` was a no-op on every inference device.
  The base weights are additionally detached in the trainer so the claim is
  stated rather than inherited.
- **No workspace crate uses autodiff yet.** `require_grad`, `backward`,
  `Autodiff<`, and `GradientsParams` appear nowhere under `crates/`. This is
  the first use; the `burn` dependency in esp is
  `features = ["ndarray", "std"]`, so `autodiff` (and `optim`, which implies
  it) is a new feature edge. `burn-autodiff` and `burn-optim` 0.22.0-pre.2
  are already in the local registry. The workspace's vendored
  `burn-cubecl`/`burn-remote` patches are untouched by this.
- **The v0 trainer's constraints are all incidental to finite differences.**
  One target module, one shared expected token across the batch, one shared
  tokenized length, loss on the last position only, A deterministic-init and
  B zero so step zero is the base model, 2N forwards per step over N factor
  parameters. Only the last-position loss and the zero-B start are contract;
  the rest existed to keep the finite-difference bill payable.
- **The byte contract is name/shape/dtype, not trainer version.** The loader
  (`lora.rs::apply_peft_lora`) checks `adapter_config.json` against the
  manifest (`peft-{peft_version}` must equal `adapter_format_version`, rank
  and alpha must match, bias none, no PEFT variants) and reads
  `base_model.model.model.layers.{i}.self_attn.{module}.lora_{A,B}.weight`
  as F32 `[rank, in]` / `[out, rank]`. A `peft_version` of `esp-trainer-v1`
  with format version `peft-esp-trainer-v1` flows through unchanged.
- **The FLoRA stacker requires one trainer version per round.**
  `flora.rs` refuses contributions whose `adapter_format_version` or
  `peft_version` differ from the first. A round is therefore homogeneous:
  v1 adapters stack with v1, never with v0. This is the intended posture,
  not a limitation to work around.
- **Settings reach the trainer only through `TrainRequest`.**
  `ports/distillery/src/trainer.rs` embeds `LoraTrainerSettings` in
  `TrainRequest` and records it under `training_method.settings`; Djinn's
  receipts construct the request in tests and post it as the job input.
  No persisted request shape exists, so the request may change without a
  legacy reader.
- **Measured baseline to beat.** On the two-layer, eight-hidden fixture the v0
  trainer runs 40 steps at 16.6 s on CPU and 124 s on the discrete GPU
  (distillery v0 plan, 2026-09-02), with per-step launch overhead dominating
  the GPU figure. Autodiff replaces 2N forwards per step with one forward and
  one backward.

- **`Device::autodiff` is also a method, and it panics when applied twice.**
  `burn::tensor::Device::autodiff(self)` wraps; `Device::is_autodiff()` is
  the guard. The trainer accepts either an inner or an already-wrapped
  device. Phase 3 composes it over the probed discrete device.
- **burn 0.22 ships its own LoRA reparameterization**
  (`burn_core::module::param::lora::LoraAdapter`). Not used: the plan keeps
  the loader's `add_delta` as the one composition, and
  `Param::with_reparameterization` is `pub(crate)` upstream. It is the
  candidate if a later slice wants the composition to live in the module
  tree instead of being rebuilt per step.
- **`AdamConfig::init()` returns a `ModuleOptimizer` whose `step` re-marks
  each updated tensor as a fresh tracked leaf**, so the returned factor
  module feeds straight back into the next forward. The decoder is rebuilt
  around the factors every step (rope tables included); cheap on the fixture,
  and the thing to measure before a real-size run.
- **The held-out ranking tally is a coarse, non-monotone proxy on the
  fixture.** Swept at learning rate 0.2 the v1 tally reads 3, 3, 2, 6, 3, 3,
  4 of 6 at 8/12/16/20/24/30/36 steps. A rank-1 delta on a near-uniform
  eight-hidden model reorders the logit row in jumps. Every count beats the
  baseline's 0/6, which is what the receipt certifies; v0-versus-v1
  comparisons rest on loss per second and step count, not the tally.
- **CPU and GPU v1 runs agreed to six decimals** on both loss endpoints of
  the eight-step fixture. The strict-improvement-only posture of the GPU
  receipt stands as the contract regardless.

## Decisions

Each has more than one defensible answer; the recommendation is marked.

- **D1, request shape.** (a, recommended) `TrainRequest.settings` becomes an
  externally tagged `TrainerSettings` enum with `FiniteDifference(LoraTrainerSettings)`
  and `Autodiff(AutodiffLoraSettings)` arms; one resource id, one
  implementation id, and `training_method.trainer` names which arm ran. (b) A
  second resource with its own request type. (a) keeps the lane's admission,
  receipts, and Tulpa references pointing at one trainer resource; (b) keeps
  v0's request bytes frozen at the cost of a second lane entry.
- **D2, first-slice objective.** (recommended) lift the shared-expected-token
  rule (a per-case target is a gather), allow several target modules from
  q/k/v/o (the loader and manifest already carry a list), keep the
  shared-length rule for this slice and name padding-with-mask as the
  follow-on. Alternative: keep v0's exact constraints and change only the
  gradient.
- **D3, optimizer.** (recommended) Adam from `burn-optim` over a two-parameter
  LoRA `Module` per target projection, with `learning_rate`, `beta1`,
  `beta2`, `epsilon`, `weight_decay` and `steps` as explicit settings and no
  `Default`, v0's rule. Alternative: hand-rolled SGD on raw tensors, fewer
  moving parts and no `optim` feature edge, but the stack then carries its
  own optimizer.

## Phase 1: ESP autodiff trainer

Add `decoder-autodiff = ["decoder-lora", "burn/autodiff", "burn/optim"]` to
esp. Add `infer::decoder::train::autodiff` (or a sibling module) with
`AutodiffLoraSettings`, `train_peft_lora_autodiff(config, tokenizer, weights,
model_id, cases, settings, device) -> TrainedLoraAdapter`, and the version
constants `esp-trainer-v1` / `peft-esp-trainer-v1`. The training model is
built from the loaded decoder with base weights detached and the LoRA
factors as tracked parameters; the loss is mean next-token cross-entropy at
the last position through the real forward and `add_delta`; the serializer
is v0's, so the bytes differ only in values.

Done conditions:

- A gradient check: at the v0 initial point on the tiny fixture, the autodiff
  gradient of every factor parameter agrees with v0's central difference to
  a stated tolerance, on `Device::ndarray()`.
- Training strictly reduces the loss on the fixture, and the produced adapter
  loads through the unchanged `PeftLoraAdapterLoader` and passes
  `apply_peft_lora`'s checks with its own version strings.
- A v1 forcing receipt (`tests/trainer_forcing_autodiff.rs`, or a second case
  in the existing fixture) shows the held-out strict improvement, with fewer
  steps than v0's 40.
- The same test runs on `Device::autodiff(Device::wgpu(..))` behind
  `decoder-wgpu`, asserting strict improvement only, per the v0 GPU receipt's
  floating-point posture.
- `cargo clippy -p esp --features decoder-autodiff,decoder-wgpu --all-targets -- -D warnings`
  adds no finding.

## Phase 2: Distillery request and receipt

Land D1. `run_train_job` dispatches on the settings arm, records
`training_method.trainer` as `esp-trainer-v0` or `esp-trainer-v1-autodiff`,
and stamps the matching `adapter_format_version`. Add a
`trainer-autodiff = ["trainer", "esp/decoder-autodiff"]` feature (and
`trainer-gpu` continues to compose with it).

Done conditions:

- `ports/distillery/tests/trainer.rs` runs the resource end to end with the
  autodiff arm, publishing manifest, blobs, and `EvalReport` whose
  `validate_for_adapter` passes.
- The FLoRA stacker receipt stacks two v1 adapters from two participants and
  produces the same exact bytes regardless of arrival order, and refuses a
  v0/v1 mix by name.
- Package Clippy for distillery with `flora,trainer-autodiff,trainer-gpu`
  over all tests adds no finding.

## Phase 3: Djinn lane

The lane's trainer settings name the method. The CPU trainer receipt and the
discrete-GPU trainer receipt gain the autodiff arm; the GPU receipt composes
`Device::autodiff` over the probed discrete device from
`discrete_gpu_trainer_device`.

Done conditions:

- `cargo test -p djinn --features trainer --test distillery_trainer` and
  `--features trainer-gpu --test distillery_trainer_gpu` pass with the
  autodiff arm on this machine (windows-msvc, with the recorded mere-canvas
  non-incremental link workaround), and the CPU receipt is rerun on the
  Fedora ThinkPad.
- The lane refuses by name a request whose arm this build does not carry.

## Phase 4: receipts and docs

Record v0-versus-v1 wall time and step counts on the fixture for CPU and GPU
in the distillery v0 plan's progress log, update this plan's status and the
index, and note in the FLORA plan that rounds are trainer-version
homogeneous.

## Done

The autodiff trainer is the default arm in Djinn's lane configuration
examples; v0 remains available under its own arm; every receipt above is
green on the landed `main`; and the FLoRA artifact contract has not changed
a byte.

## Progress

- **2026-09-03:** Phase 1 landed in `crates/intel/esp`: `decoder-autodiff`
  feature; `infer::decoder::train_autodiff` with `AutodiffLoraSettings`,
  `train_peft_lora_autodiff`, and the `esp-trainer-v1` /
  `peft-esp-trainer-v1` stamps; the LoRA factors as a burn `Module` under
  `burn-optim` Adam; v0's serializer and config writer generalized to several
  modules and shared by both trainers, with a byte-identity test against the
  verbatim v0 writers. Gradient check against v0's own `Objective::loss`:
  tolerance `1e-4 + 0.005·|fd|` at `h = 0.01`, observed max error 7.8e-7 over
  24 parameters. Receipts: v0 `decoder-lora` suite unchanged (125 tests, the
  forcing receipt reproducing its loss, ranks, and 4/6 tally); v1 suite green
  on `Device::ndarray()` and on the discrete GPU; strict package Clippy with
  `decoder-autodiff,decoder-wgpu` over all targets clean; fmt clean. Timings
  on the fixture, debug build: v0 40 steps ≈ 10–15 s CPU; v1 12 steps 0.17 s
  CPU, 8 steps 0.63 s warm on the GPU (3.8 s cold). The v1 forcing receipt
  reads 3/6 held-out at 12 steps against a 0/6 baseline.

- **2026-09-02:** assessment complete; findings above verified against the
  code. Mark ruled D1 (tagged `TrainerSettings` enum on the one trainer
  resource), D2 (per-case targets and several target modules now; shared
  length kept, padding with a mask named as the follow-on), and D3 (Adam
  from `burn-optim` over a LoRA `Module`, every hyperparameter explicit).
