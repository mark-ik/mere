# Autodiff LoRA trainer plan

**Status (2026-09-02):** in progress. Assessment complete; Mark ruled D1–D3
on 2026-09-02, each on the recommended option; every phase landed 2026-09-03 and the
receipts were rerun on the rebased tree and on the Fedora ThinkPad. Complete
once the branch is on `main`; the padding-with-mask follow-on and the burn
`LoraAdapter` question stay open in Findings. Follow-on to the
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
- **`training_method.settings` was never byte-stable across the f32
  boundary.** `serde_json` widens `f32` to `f64`, so `0.9f32` publishes as
  `0.8999999761581421`; v0's settings went through the same path. A receipt
  reader compares settings as numbers, never as bytes.
- **Two v1 strings, both load-bearing.** `peft_version: "esp-trainer-v1"`
  inside `adapter_config.json` is esp's stamp, checked by the loader against
  `adapter_format_version`; `training_method.trainer: "esp-trainer-v1-autodiff"`
  is the receipt's name for the method and the request arm's serde tag.
  Neither derives from the other; both are constants.
- **A v0/v1 mix in a FLoRA round is refused twice**: first on
  `adapter_format_version`, and if the manifests are forced to agree, again
  on the config bytes' `peft_version`. The round receipt proves both.
- **The implementation id under-described the build after D1.** The id it
  carried named the finite-difference arm while answering both. Mark ruled
  2026-09-03 to rename it to a method-neutral id, accepting the wire-visible
  change; it landed in Phase 3 as `esp.train.peft-lora.esp-trainer/v2` — `/v2`
  because `TrainRequest.settings` changed shape, so a v1-era poster's bytes no
  longer parse. The resource id `esp.train.peft-lora/v1` is unchanged: what is
  asked for did not change, only what answers. The old string appeared nowhere
  outside Distillery's own constant and this document.

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
- **2026-09-03:** Phase 2 landed in `ports/distillery`: `TrainRequest.settings`
  is the externally tagged `TrainerSettings` (`esp-trainer-v0` /
  `esp-trainer-v1-autodiff`); `run_train_job` dispatches on the arm and takes
  the manifest's rank, alpha, target modules, format version, and
  `training_method` from the arm that ran; `trainer-autodiff` feature; a
  build without it reads the arm and refuses it by name at admission. Both
  arms run end to end through the mesh harness (v0 0/6 → 4/6, v1 0/6 → 3/6
  at RankingAt{3}); a FLoRA round over two real v1 adapters stacks to the
  same 680-byte rank-2 aggregate in either arrival order and a v0/v1 mix is
  refused by name. Distillery suite 22 tests green with
  `flora,trainer-autodiff,trainer-gpu`, v0-only trainer build green, strict
  package Clippy clean, fmt clean.
- **2026-09-03:** Phase 3 landed. The v1 settings type and version
  constants moved out from behind esp's `decoder-autodiff` (they ride with
  the loader under `decoder-lora`), so Distillery's request arm is typed in
  every build and `adapter_shape()` is total; the implementation id is
  `esp.train.peft-lora.esp-trainer/v2` per Mark's ruling. Djinn gained
  `trainer-autodiff`; its CPU and discrete-GPU receipts run both arms in
  sequence on one lane and assert the published manifest's
  `adapter_format_version` and `training_method.trainer` per arm, and a
  `trainer`-only build refuses a v1 request by name at admission with the
  board's committed count unchanged. The lane configuration names only the
  device; the arm arrives inside the posted request, and nothing in
  `settings.rs` or `resident_distillery.rs` changed. Through the composed
  lane on this machine (debug): v0 40 steps 12.4 s CPU / 76.8 s GPU, v1 12
  Adam steps 0.4 s CPU / 4.5 s GPU (RTX 4060 Laptop, vulkan), every arm 0/6
  → strict improvement at RankingAt{3}. Gates: esp 137, Distillery 22, Djinn
  CPU 2+2, lane 4, GPU 1; strict Clippy clean on esp and Distillery and on
  Djinn's test targets once its four pre-existing library lints are allowed;
  fmt clean. One finding on the way: removing the feature-varying alias
  exposed that Distillery's `trainer-gpu` re-exports have exactly one
  consumer, Djinn's GPU build, and only that build proves they exist.
- **2026-09-03:** Phase 4. The branch was rebased onto `origin/main`
  `129734d1` (thirteen upstream commits, overlapping this work only on
  Distillery's manifest, where a dev-dependency was added), and the
  receipts reran on the rebased tree: Distillery 22 tests including
  upstream's new `walk_fixtures`, esp 137, Djinn CPU autodiff 2/2, Djinn GPU
  1/1 (152 s on a loaded machine), strict Clippy clean on esp. Strict Clippy
  on Distillery now trips on upstream's own `walk_fixtures.rs`
  (`type_complexity`), which this branch did not write and does not fix.
  The Fedora ThinkPad (`thinkpad-l14-f`, Rust 1.97.1, AMD Renoir) reran the
  CPU receipts from the pushed branch at a30381a2 in its own worktree: the
  esp `decoder-autodiff` suite green (v1 forcing receipt 0.42 s against v0's
  18.0 s), the Djinn CPU receipt on the `trainer-autodiff` build 2/2, and on
  the `trainer`-only build the by-name refusal 2/2. Nothing in esp or Djinn
  changed between that commit and the rebased tip. The Distillery and FLORA
  plans carry their cross-notes; the index names the arm.

- **2026-09-02:** assessment complete; findings above verified against the
  code. Mark ruled D1 (tagged `TrainerSettings` enum on the one trainer
  resource), D2 (per-case targets and several target modules now; shared
  length kept, padding with a mask named as the follow-on), and D3 (Adam
  from `burn-optim` over a LoRA `Module`, every hyperparameter explicit).
