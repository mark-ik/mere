# Distillery v0 Plan

**Date**: 2026-08-12  
**Status**: D0 complete; D1 resident authority lifecycle complete. Its installed
Personae/settings binding, configure/inspect binary, and read-only Cambium
surface pass an exact-source focused Cargo gate, and Turnstone admits the
surface as the contribution seam's second provider; operational host
composition is ruled, built, and receipted in §10 — the Djinn lane runs a
real mesh job from stated policy on `SystemClock` and closes clean — and
the full workspace gate is closed green, leaving only the deferrals §10
records. D2's
configured browser embedding matrix, first exact decoder row, lease-bound
remote MiniLM row, and native ModelSession/PEFT LoRA row complete. Cooperative
cancellation, explicit browser device teardown, fresh-worker recovery, exact
remote reclaim ordering, CubeCL allocator cleanup, and real adapter numerical
parity are proven. Driver-level release is proven for the supported plain
remote profile. The immutable TrainingCorpus and EvalReport artifact contract
carries the first local trainer receipt, and the trainer resource that
receipt decided now runs as a real mesh job: `esp.train.peft-lora/v1`
trains from a stored corpus's training partition, publishes the adapter
blobs, manifest, and evaluation report into the composed Eidetic store, and
commits an integer-only receipt in which the adapter strictly beats the
unchanged baseline. Browser-level physical GPU allocation release remains
unobservable through current browser APIs rather than an actionable
implementation gate.

## 1. Purpose

Distillery is Mere's model-works port. D0 proves the port by making it the first
real consumer of `mere-mesh-host`, rather than founding another scheduler or
starting with ornamental views.

The port owns product composition. Mesh owns job, lease, checkpoint, and safe
retention truth. `mere-transport` owns physical blob storage. Distillery drives
the host and decides when owner-governed maintenance runs.

## 2. D0 contract

`Distillery<B>`:

- owns a `MeshHost<B>` and drives one non-blocking supervisor tick at a time;
- returns the host's exact `Step` receipts;
- owns configurable retention behavior, with collection off by default;
- explicitly authors an owner-governed checkpoint before collecting;
- asks `MeshStore` for the safe blob set rather than reconstructing mesh truth;
- releases only this mesh's custody tags; and
- reports the resulting `RetentionEffect::BlobCollected`.

The maintenance report distinguishes candidates from tags actually released.
That distinction matters because repeated custody release is idempotent and a
different mesh or subsystem may retain the same physical content.

## 3. Findings forced by the implementation

### Shared hashes cross job boundaries

The original H2 query considered only terminal jobs in the accepted checkpoint.
That was insufficient for content-addressed storage. A job posted after that
checkpoint can reuse the same input hash, so deleting the older job's input
would also delete the newer job's input. `collectable_blobs` now compares the
checkpoint with the current board tail, protects every reference not settled
by that checkpoint, and deduplicates the release set.

### Custody is not physical ownership

The transport store is shared infrastructure. Anonymous permanent tags could
never be released, while deleting bytes by hash would erase every subsystem's
copy. The store now supports stable named custody tags and collecting memory or
disk modes. Mesh tags include the mesh id and content hash. Removing one tag
does not remove an Eidetic, other-mesh, or other-subsystem claim; garbage
collection removes physical bytes only after the last tag is gone.

## 4. Files

- `ports/distillery/src/authority.rs`: product authority and maintenance report.
- `ports/distillery/tests/authority.rs`: real joined-mesh D0 receipt.
- `crates/mesh/host/src/host.rs`: explicit checkpoint operation for a product host.
- `crates/mesh/host/src/courier.rs`: mesh-scoped transport custody.
- `crates/mesh/mesh/src/retention.rs`: checkpoint plus current-tail safety rule.
- `crates/murm/transport/src/blobs.rs`: named tags and collecting stores.

## 5. Acceptance receipt

The Distillery integration test joins a real one-host mesh over the p2panda
transport, stages an input in the transport's blob store, posts and executes
`mesh.blake3/v1`, and proves:

1. Distillery drives the host to completion.
2. Completion alone retains both input and output.
3. Explicit maintenance authors and accepts a checkpoint.
4. The maintenance report names two candidates and two released custody tags.
5. Store garbage collection removes the unowned input.

Lower-level receipts prove a shared hash remains while another subsystem's tag
exists, a post-checkpoint job protects a reused hash, the disk-backed collecting
constructor actually reclaims released bytes, and existing host blob delivery
still completes across disjoint stores.

Verified from `C:\Users\mark_\Code\repos\mere` with an isolated
`CARGO_TARGET_DIR=C:\t\distillery-v0`:

- `cargo test -p distillery`: 1 integration receipt passed.
- `cargo test -p mere-transport --lib`: 39 tests passed.
- `cargo test -p mere-mesh --lib`: 117 tests passed.
- `cargo test -p mere-mesh-host`: 10 unit and 2 real-host integration tests
  passed.
- strict all-target Clippy passed for Distillery and the three touched substrate
  crates with `--no-deps -- -D warnings`.

## 6. Stop boundary

D0 does not create the resident process or lifecycle, Cambium views, the model
manifest browser, streaming console, Burn resource migration, Burn Remote
adapter, portable remote checkpoints, tolerant comparators, or training.

## 7. D1 resident authority lifecycle

D1 in this document is Distillery-local. It does not rename the ESP
consolidation plan's D1 device-policy decision.

D1 supplies the reusable process body without hardwiring an identity profile or
an operating-system launcher.

`ResidentSettings` has no default. The embedding owner selects:

- supervisor tick cadence;
- optional maintenance cadence;
- physical blob collection cadence; and
- whether accepted maintenance releases settled mesh custody.

`ResidentStorage` opens a disk-backed collecting `BlobStore` and scopes its
custody tags to one mesh. `ResidentAuthority` rejects a host for another mesh,
owns the composed `Distillery`, p2p transport, and storage, and closes them in
that order: endpoint, joined mesh, blob-store flush.

The first restart receipt exposed a deeper lifecycle distinction: dropping a
LogSync requested actor stop but returned before its redb handle was released.
`p2panda-net::LogSync::shutdown`, `JoinedSpace::leave_and_wait`, and the
consuming mesh-host shutdown path now abort and await local job/drain tasks,
drain every topic manager, await the top-level sync actor's owning task so its
state has actually dropped, then return. Shutdown failure is carried through
the full authority chain while the remaining close operations are still
attempted. A reported clean shutdown therefore permits an immediate same-path
reopen.

The run loop emits ordered `ResidentReceipt` values for exact substrate steps,
completed maintenance, unchanged maintenance, maintenance failure, supervisor
failure, and requested stop. A maintenance refusal does not stop useful work.
That distinction is required because a checkpoint landing during a live lease
is expected to fail closed. A supervisor failure does end the run.

Cadenced maintenance uses `MeshHost::checkpoint_if_advanced`. It compares the
candidate event frontier with the latest accepted checkpoint and leaves an
empty or unchanged mesh alone. The explicit `Distillery::maintain` command
still authors whenever the owner asks.

### D1 receipt

The resident integration receipt uses a real p2panda transport, a redb-backed
`MeshStore`, and a disk-backed collecting blob store. It proves:

1. The configured tick cadence drives a V2 job to completion.
2. Scheduled maintenance releases the input and output custody tags.
3. The next maintenance turn reports an unchanged frontier without another
   checkpoint.
4. Shutdown is observed and closes the endpoint before persistent storage.
5. Reopening the redb mesh store replays the terminal job.
6. Reopening the collecting blob store preserves the custody release.

The 2026-08-20 receipt passed all three Distillery tests, including immediate
same-path redb/blob reopening, and strict Clippy for the Distillery targets and
the changed `p2panda-net` library. Shared Cargo cache contention required an
isolated manifest over the exact source files; the source and test targets were
unchanged.

## 8. Installed authority boundary

The first installed slice persists one explicit Personae `ProfileId` at
`<data-root>/distillery/settings.json`. It is a product-owned, local-only,
restart-required ordinary setting. It has no default and does not consult a
Distillery profile environment variable: an absent record is unconfigured, an
empty or unknown field is refused, and an absent selected profile fails rather
than minting a second face. The profile's mesh author derives under
`mesh::MESH_AUTHOR_SALT`; its master key supplies the transport identity. The
settings record carries neither seed nor vault secret.

Per-mesh private persistence sits at
`<data-root>/distillery/meshes/<mesh-id>/{mesh.redb,blobs}`. The reusable
`InstalledAuthority` layer opens the persisted profile, derives the author and
attestation, and binds a `ResidentAuthority` only when a caller supplies the
`MeshStore`, `HostConfig`, and `ResidentSettings` it owns. The mesh caller
therefore retains admission/retention truth; the device caller retains
resources, scheduler, conditions, and device policy. Distillery does not make
defaults for any of those facts.

`distillery-installed` is an executable bootstrap boundary, not yet a fully
operational standalone host. Its `configure` verb persists the explicit
profile; `inspect` proves that it opens from Personae and reports protection.
Starting a resident remains gated on a product composition owner supplying a
real mesh retention/admission policy and device `HostConfig`/`ResidentSettings`.
Those constructors exist through `InstalledAuthority`; the installed library
tests and configure/inspect binary now compile against the exact integrated
source graph. A CLI default here would be fabricated scheduler/device
authority.

Distillery now also homes D2's standalone browser development probe without
making it product chrome. The recovered BrowserWebGpu path now passes four
pinned embedding artifacts from 34.8 MB F16 BGE Micro through 438.0 MB F32
E5-base. Cold and warm execution pass integrity, independent numerical
reference, repeatability, GPU-error, and worker-cutoff gates. The persistent
storage request resolved `false`, so the 698 MB IndexedDB corpus remains a
reconstructible best-effort cache rather than durable product storage.

The first D2 decoder row now passes a pinned 269,060,552-byte BF16 SmolLM2
artifact in headed Chromium. Cold and warm workers reproduce the independent
Transformers and ESP NdArray token sequence exactly, stream every fragment to
the page, reopen matching IndexedDB content, stay below the configured frame
p95 bound, and report no WebGPU validation errors. The row forced Llama's
split-half rotary rule and async browser token readback into ESP. The lifecycle
receipt now cancels before the next token crosses ESP's observer boundary,
destroys the worker's tracked `GPUDevice`, terminates cleanly, and reproduces
the exact output in a fresh worker. Physical GPU-allocation release remains
unobservable. The upper embedding and decoder boundaries do not need larger
rows until a consumer asks for them.

The first Cambium surface now follows `ResidentReceipt` after the installed
authority selects its Personae profile and settings location. It renders only
the selected profile/protection, mesh-local paths, bound resident settings, and
the latest exact observer receipt; it owns no scheduler, retention, device,
job-board, or session copy. Broad job/device/session projection waits for each
authority to expose an equally truthful snapshot. The prerelease Burn 0.22
migration has executed for ESP and Quint; stable repinning and package closure
remain release-gated. The lease-bound Burn Remote source audit and targeted
session-close seam are complete, including live-request interruption,
stop-before-reclaim ordering, zero sessions, and fresh-lease recovery.

The allocator sidequest is complete through the patched CubeCL
`ComputeClient::memory_usage()` seam. Both plain-WGPU MiniLM sessions rose from
zero to 101 live allocations and 90,261,504 bytes in use, then returned exactly
to the zero baseline before their reclaim facts. Reserved bytes are recorded
but remain outside the allocator gate. A separate Windows driver receipt now
measures 604,114,944 and 637,603,840 dedicated bytes released across the two
plain-profile cycles, zero retained growth across reclaim baselines, NVIDIA GPU
0 attribution, and counter disappearance after process exit. Burn Remote now
keeps draining sessions visible until worker cleanup, propagates worker failure
through Distillery, and wakes detached writers on close.

The bounded feature matrix passes every local row and remote plain. Remote
Fusion plus autotune completes inference but retains five live allocations;
remote autotune-only becomes timing-sensitive through inference/reclaim, while
remote Fusion-only stalls during first provider load. This is an upstream
Burn/CubeCL remote-backend compatibility lane, not a Distillery lease-adapter
gate. Plain WGPU remains the supported remote profile.

Model manifest browsing, streaming console, portable remote checkpoints,
tolerant comparators, a real stacked-adapter row, and training remain separate
slices.
## 9. Training artifact boundary

The first local trainer vertical needs three immutable Eidetic artifacts:
`ModelAdapterManifest`, `TrainingCorpus`, and `EvalReport`. The first is
already the ModelSession provenance envelope. `TrainingCorpus` now separates
non-empty, strictly manifest-id-ordered training and held-out evaluation source
partitions; overlap is rejected before the artifact is stored. `EvalReport` is
a fixed-corpus baseline-versus-adapter `RecallAt` or `RankingAt` receipt using
integer passes and total cases. It rejects zero limits, impossible tallies, and
comparisons over different case counts; it also validates the adapter manifest
itself and then checks its model, adapter, and corpus links.

This is Eidetic's artifact contract only. It does not create a trainer, pick a
runtime or optimizer, assign a Mesh resource, record a lease/checkpoint, or
make Distillery infer device policy. Those facts remain respectively ESP,
Mesh, and the host scheduler's authorities.

### Next forcing task

Complete (2026-08-26): `crates/intel/esp/tests/trainer_forcing.rs` is the
local, deterministic ranking fixture. The receipt decides the first trainer
resource input/output shape:

- **Inputs**: the base `ModelManifest` ref, its tokenizer blob ref, the
  `TrainingCorpus` ref (only `training_source_engrams` may be read while
  training), and explicit hyperparameters recorded in the manifest's
  `training_method`. Nothing is defaulted or inferred from job facts.
- **Outputs**: the adapter weight blob, the adapter config blob, the
  `ModelAdapterManifest` whose `training_corpus_root` names the corpus, and
  the `EvalReport` comparing baseline and adapter on the held-out partition
  under one explicit metric.

A trainer resource is a function from the input refs to the output refs, and
the mesh job carrying it owns none of the artifact truth. That resource now
exists as `esp.train.peft-lora/v1` (`ports/distillery/src/trainer.rs`, the
`trainer` feature): the job's one input slot carries an explicit
`TrainRequest`, its committed output is the integer-only `TrainReceipt`
naming the published refs, and the artifacts land in the Eidetic store the
embedder composed the resource over. Stronger models wait for a consumer;
the Turnstone admission landed (Turnstone `9d3a7d8`), and operational
host-policy composition plus the full workspace gate continue to track in
§8.

### Progress

- **2026-08-26, installed authority/settings slice**: Distillery now stores an
  explicit Personae profile selection under its private data root and refuses
  a missing or malformed selection. `InstalledAuthority` opens that existing
  profile from Personae, derives the stable mesh author and attestation, uses
  the profile master for p2p transport, names private mesh paths, and composes
  a resident only from caller-owned mesh and device facts. The
  `distillery-installed` binary is the configure/inspect bootstrap boundary;
  it records the deliberate resident-start gate instead of hiding it behind a
  permissive device or cadence default. An isolated exact-source harness passed
  all five Distillery library tests, the installed binary check passed, and
  package Clippy passed with warnings denied against the same integrated source
  graph. The full workspace gate remains open while C0/C1 owns the primary
  checkout.
- **2026-08-26, artifact foundation**: Eidetic gained typed `TrainingCorpus`
  and `EvalReport` payloads with fixed schema references, validating
  serialization, and typed-store round trips. The corpus uses separate,
  disjoint training and evaluation partitions; the report deliberately records
  only deterministic ranking/recall counts and validates a well-formed adapter
  plus the provenance links already reserved by `ModelAdapterManifest`. No
  training, evaluation runtime, Mesh job, lease, checkpoint, or Distillery
  authority was added.
- **2026-08-26, trainer forcing fixture**: The always-run ESP integration
  receipt (`cargo test -p esp --features decoder-lora`, fixture
  `trainer_forcing.rs`) stores the tiny synthetic llama triple through the
  eidetic model corridor, saves twelve ranking cases as `OpaqueBlob` engrams,
  and materializes a canonical disjoint `TrainingCorpus`. A deterministic
  trainer — full-batch central finite differences with a backtracking line
  search over rank-1 `v_proj` LoRA factors, now shared as
  `esp::infer::decoder::train` — reads only the training partition back out
  of the store and evaluates every loss through the real decoder forward
  with the adapter loader's own delta composition. The published
  adapter (PEFT-named safetensors, `adapter_config.json`, manifest with
  `training_corpus_root` and explicit `training_method`) loads through the
  real `PeftLoraAdapterLoader` alongside the unchanged baseline; on the six
  held-out cases the expected-token rank moves from 7–8 to [3, 2, 2, 6, 2,
  4], a 0/6-versus-4/6 `RankingAt{limit: 3}` receipt (training loss 3.4603 →
  3.4581). The stored `EvalReport` round-trips, reports the strict
  improvement, and passes `validate_for_adapter` against the stored manifest.
  Verified from a clean `origin/main` worktree with an isolated target dir:
  the fixture, the full esp `decoder-lora` suite, and package Clippy with
  warnings denied.
- **2026-08-26, read-only contributed surface**: The
  `distillery.installed.v1` descriptor and erased Cambium session consume the
  installed authority's profile/path projection and only resident facts the
  caller supplies from the real resident (`ResidentSettings` plus its latest
  `ResidentReceipt`). It opens no authority and has no mutable controls. This
  is the second product half of the Knot contribution seam; Turnstone
  `9d3a7d8` now admits it through the shared registry with no
  provider-specific renderer arm, generic AccessKit projection landed, and
  the Turnstone full-shell binary builds from published sources. The surface is included in the green
  five-test exact-source Distillery library receipt; package Clippy also passes
  with warnings denied.
- **2026-08-26, trainer resource**: The forcing receipt's shape now runs as a
  real mesh job. The shared trainer moved into `esp::infer::decoder::train`
  (the fixture consumes it and reproduces its receipt exactly), and
  Distillery's new `trainer` feature registers `TrainerResource` at
  `esp.train.peft-lora/v1` through the ordinary `ResourceRegistry` seam —
  composed by its embedder over an Eidetic store and a host-selected device,
  declared `VerificationClass::ProducerOnly` because no cross-device bit
  receipt has been earned. The whole run executes on one blocking thread:
  explicit-ref loads (base manifest, tokenizer cross-check, validated
  corpus), training from the training partition only, evaluation of both
  sessions through the real `PeftLoraAdapterLoader`, then publication, with
  enforced equality between the manifest's content addresses and what the
  store landed. The integration receipt posts one staged `TrainRequest`,
  drives the host to `Step::Completed`, reads the committed integer-only
  `TrainReceipt` from the board's output blob, and finds the adapter
  manifest, blobs, and `EvalReport` in the composed store under exactly the
  receipt's refs — baseline 0/6 versus adapter 4/6 at `RankingAt{3}`, the
  fixture's numbers, via a mesh job. Verified from a clean `origin/main`
  worktree: the receipt, both distillery feature sets' tests, the full esp
  `decoder-lora` suite, and package Clippy with warnings denied.

## 10. Operational host-policy composition (assessment)

### What the gate actually is

`InstalledAuthority::bind_resident` is the composition seam and it takes no
defaults by design: the caller hands it a `MeshStore` with its retention
policy, a `HostConfig`, and `ResidentSettings`. The runtime above it is
finished — `ResidentAuthority::run_until` drives cadenced ticks, maintenance,
receipts, and ordered shutdown, and three integration receipts exercise it.
What is missing is not machinery. It is that nobody has ever supplied real
values, and four of the values a product must supply have no real
implementation anywhere in the workspace.

Of `HostConfig`'s eight fields, `HostConfig::supervised` defaults three to
something a product could ship — `ResourceRegistry::builtin()` as a floor to
extend, `SystemClock`, and `LeasePolicy`'s 60s skew — and four to documented
placeholders no product may ship:

- `courier: NoCourier` — every job must already hold its own inputs.
- `policy: DevicePolicy::permissive()` — lends everything, always, under any
  condition; the only refusal it can express is `unsupervised()`, which
  declines leased jobs entirely.
- `conditions: ObservedConditions::spare()` — a fixed snapshot (idle, 100%
  battery, on mains, 40°C, wired, 14:00) that observes nothing.
- `facts: HostFacts::cpu(4096)` — a fixed 4 GiB, no-GPU claim regardless of
  the machine, and the value `ResourceRequirements::satisfied_by` gates
  every job against.

`MeshRetentionPolicy` has no constructor, preset, or `Default` at all.

### Findings

**The device-sensing seam is unimplemented workspace-wide.** `ConditionSource`
has exactly one implementation, `ObservedConditions`, a `Mutex`-backed manual
stub whose module doc says a real host implements the trait "over whatever it
can actually observe." Nothing in Mere reads battery, thermal, network class,
or foreground activity from any operating system. `DevicePolicy` can express
rich withholding rules — idle floors, battery floors unless on mains, thermal
ceilings, network-class floors, bandwidth ceilings, quiet hours, concurrency
caps, resource allow-lists, accepted checkpoint classes — and every one of
them is inert without a source that senses. `conservative()` exists as a
plausible preset (120s idle, 40% battery, 85°C, Wifi minimum, 22:00–08:00
quiet hours, one job at a time); it is used by the remote probe binary and one
test, and never by a product.

**Delivery is proven but unadopted.** `TransportCourier::for_mesh` is
constructed exactly once in the tree, in the mesh-host `blob_delivery`
receipt that closed H1 with a disjoint-store two-host proof. No product path
wires it; every `HostConfig` in the workspace keeps `NoCourier`. This matters
only for a multi-device mesh: when the poster and the runner are the same
process, inputs are always local and `NoCourier` is honest. A single-device
operational resident therefore does not need the courier, and a multi-device
one cannot work without it. Which of those the installed product is has never
been decided.

**Retention policy has no starting point.** Five call sites hand-build the
identical literal: an arbitrary revision, `checkpoint_authority` set to the
local mesh author's own key, `promised_floor: Forever`, `privacy_ceiling:
UntilCheckpoint`, `terminal_job_payload: EraseTerminalAtCheckpoint`,
`max_skew_ms: 0`. Nothing in the tree demonstrates a checkpoint authority
distinct from the device running it, so the multi-device governance shape is
unexercised as well as unwritten.

**Nothing provisions a mesh identity.** `bind_resident` takes `mesh_id: [u8;
32]` and every caller hardcodes a test constant. An installed product has no
way to answer "which mesh is this?" — neither a derived personal mesh nor a
join from an invitation exists.

**The resident composition owner already exists, and it is not Distillery.**
`ports/djinn` is the local-first desktop resident composition: it owns the
selected Personae profile, the resident lifecycle, endpoint ownership, and an
`OwnerSettings` record whose `Option<KnotResidentSettings>` means exactly
"absent means this device host does not compose a resident Knot route" — the
shape a Distillery block would take. But Djinn has never touched `mesh_host`;
neither has Graphshell, Knot, or Signalman. Their personal-sync stack is
Stickleback-based and unrelated. Adding Distillery to Djinn therefore
introduces the mesh stack to the resident for the first time, and collides on
two resident-wide singletons: Djinn's `ResidentBlobCustody` opens a collecting
`BlobStore` and re-uses it "for every composed content lane," while
Distillery's `ResidentStorage` opens its own; and `bind_resident` builds its
own `P2pandaTransport` from the profile master key, the only production site
in the workspace that builds one. D0's own finding — custody is not physical
ownership, and mesh-scoped named tags let subsystems share one store — is the
argument that sharing is the intended shape.

**A device-scoped settings home already exists.** `pandect::device_settings_store`
persists device-scoped, local-only settings under `<data-root>/device/`, with
the doctrine device policy needs already stated: "Device policy must not
follow a graph session or silently become persona truth." Distillery's own
`InstalledSettings` is persona-scoped and `deny_unknown_fields`, which is the
wrong scope for a device's lending policy.

### The decisions, as ruled 2026-08-30

- **O-1, the composition owner — Djinn lane now.** Djinn gains a Distillery
  block in its `OwnerSettings`, absent by default. The mesh stack enters the
  resident for the first time. Assemble corrected two parts of this: Djinn
  owns no transport to share, so the lane keeps building its own, and
  sharing the blob store costs more than expected, which reopens whether it
  should be shared at all.
- **O-2, device conditions — sensed where possible, stated where not, never
  fabricated.** Djinn owns the `ConditionSource`, which is the right home for
  a signal shared by rendering, inference, and embedding: it sits above all
  three, so ESP's D1 concern about one consumer owning shared policy does not
  apply. Sensing is in-stack — `netwatch` and `wmi` are already compiled into
  the Distillery build through iroh, `windows` supplies `GetLastInputInfo`,
  and `sysinfo` plus wgpu adapter enumeration supply real `HostFacts` — so
  Windows needs no new dependency. Other platforms degrade to owner-stated
  values rather than to the fixed snapshot `ObservedConditions::spare()`
  reports today, which lends a device unconditionally by claiming facts it
  never observed.
- **O-3, mesh identity — a derived personal mesh.** The id derives from the
  Personae profile under its own salt, the way the mesh author already does.
  No invitation flow, and the identity is stable across reinstalls of the
  same profile.
- **O-4, reach — single-device first.** Poster and runner are the same
  process, so `NoCourier` is truthful rather than a silent limitation,
  checkpoint authority stays the local key, and no receipt claims a
  multi-device governance shape that has never been exercised. Wiring
  `TransportCourier::for_mesh` and answering the checkpoint-authority
  question stay together, in a later slice.

### What each decision leaves open

The original options, for the record, were:

- **O-1, the composition owner**: Distillery's own binary gains a `run` verb
  over device-scoped settings; or Djinn adds a Distillery lane and supplies
  the shared store and transport; or a library composition entry both can
  use, with Distillery's binary as its first consumer.
- **O-2, device conditions**: owner-stated static conditions recorded in
  settings (honest, small, states rather than invents); or a real OS-sensing
  `ConditionSource` (a cross-platform project of its own, and per the ESP
  consolidation plan's D1 a decision shared by render, inference, and
  embedding rather than Distillery's to make); or ship `unsupervised()` and
  decline leased work.
- **O-3, mesh identity**: a personal mesh derived from the profile; an
  explicit owner-supplied id in settings; or real provisioning with
  create/join.
- **O-4, reach**: single-device (keep `NoCourier`) or multi-device (wire
  `TransportCourier::for_mesh`, and face the checkpoint-authority question
  that no receipt has exercised).

### Assemble, before any of this is built

The rulings name three things whose shape has to be verified in the live
tree before implementation, not assumed: Djinn's actual injection points for
a fourth resident lane and whether its blob custody hands out what
`ResidentStorage` would need; whether `ResidentStorage` can adopt an injected
store without loosening the cadence agreement `ResidentAuthority::new`
enforces; and the concrete `wmi`/`netwatch` query surfaces for battery,
thermal, and interface class at the versions already in the build. A gap
found there returns here rather than being patched over in the implementation.

### Assemble, run 2026-08-30

The three verifications ran. Two returned clean with named change sites; the
third invalidated part of O-1 and raised decisions that belong back with the
ruling rather than in the implementation.

**Storage adoption is feasible, and the cadence invariant must not be
loosened.** `ResidentStorage` needs a constructor that adopts an existing
`Arc<BlobStore>` and builds the same `TransportBlobSpace::for_mesh` over it,
so mesh-scoped tags keep lanes disjoint inside one store. Shutdown is the
hazard: `ResidentStorage::shutdown` calls `BlobStore::shutdown`, which syncs
and closes the whole iroh-blobs store, and running that on a shared store
would close it under Djinn's other lanes. An adopted storage must drop its
space handle and stop there. The `blob_gc_every` agreement in
`ResidentAuthority::new` stays exactly as written: the field is a fact about
the store rather than a Distillery preference, so Djinn states its own
`gc_interval_seconds` and the assertion catches a divergence, which is the
same stated-truth-asserted-at-composition shape the surface contract's F-1
ruling established. A fourth site nobody had named:
`DistilleryInstalledSnapshotV1::from_installed` derives `blob_store_root`
from `DistilleryPaths` rather than from the store in use, so under an
adopted store the Turnstone pane would report a path the bytes do not live
at. `ResidentAuthority::storage()` and `ResidentStorage::root()` already
exist to correct it.

**The mesh identity derives with no new primitive.** Personae exposes only
`derive_keypair(salt)`, which is enough: derive under a product-owned salt
and take the public key as the 32-byte mesh id, exactly as the mesh author
already derives. No `derive_bytes` addition to the identity crate.

**Djinn owns no transport.** It never constructs a `P2pandaTransport`, iroh
endpoint, or router anywhere; the Knot lane hands a signing seed and relay
URLs to `knot-editor`, which builds its own transport internally, and
Distillery's `bind_resident` does the same from the profile master key. O-1
as recorded said Djinn supplies the shared blob custody and a transport. It
cannot supply a transport, and the Distillery lane keeps building its own.
Djinn supplies identity, settings, and policy facts.

**Sharing the blob store costs more than the assessment predicted.** Three
findings, none visible before reading the lane:

- Djinn's `ResidentBlobCustody::shutdown` unwraps the `Arc<BlobStore>` to
  close it. A Distillery lane holding a clone makes Djinn's own shutdown
  fail, so ownership-aware teardown is not a nicety; the shared path does
  not work without it.
- The two custody conventions do not reconcile. Djinn scopes bytes with
  `BlobScope` and `BlobLease` under a `mere/blob-lease/v1` tag prefix and
  gates reads through `BlobReadAuthorizer`; Distillery tags with a
  hand-rolled `mere/mesh/blob/v1` prefix through `TransportBlobSpace` and
  never touches the authorizer. They cannot collide, but mesh bytes in a
  shared store would sit outside Djinn's read authorization entirely.
- `DjinnResident` is not generic and `ResidentAuthority<B>` is, so the lane
  forces either one concrete backend for the whole resident or a generic
  parameter threaded through Djinn.

**Djinn's run loop has no shutdown signal.** `ResidentAuthority::run_until`
requires a future that resolves when it is time to stop; Djinn's loop ends
today only when one of its three broker futures returns. Knot needs no such
signal because its sync work runs in `KnotSyncHost`'s own background tasks
and the loop only ticks a settings refresh. A Distillery lane is the first
owning loop in that process and must be a fourth `select!` arm, so a
cancellation signal has to be added to `run()` before it can stop cleanly on
the way to `resident.shutdown()`.

**Sensing is reachable, but not the way O-2 recorded it.** `netwatch`
exposes only `is_up`, `name`, and `addrs` on an interface; it discards the
adapter type its own `netdev` dependency carries, and its `is_expensive`
field is hardcoded false on every platform, so it can answer neither network
class nor metered. `wmi` is `!Send` and `!Sync` by construction, initializes
COM per-thread, and `netwatch` itself routes every WMI call through
`spawn_blocking` because COM can deadlock on a tokio worker thread.

What actually covers the four signals is the `windows` crate, already in the
build at 0.62.2, with features added to our own dependency on it, which is
additive under feature unification and needs no version change:

| Signal | Source | COM |
|---|---|---|
| Battery and mains | `GetSystemPowerStatus` | no |
| Idle | `GetLastInputInfo` with `GetTickCount64` | no |
| Network class | `GetAdaptersAddresses`, reading `IfType` | no |
| Metered | WinRT `NetworkInformation` connection cost | yes |
| Thermal | `MSAcpi_ThermalZoneTemperature` over `wmi` | yes |

Thermal is the only signal that needs `wmi`, and it is the least dependable:
the ACPI class is frequently absent, stale, or access-restricted on modern
laptops, and no first-party Windows API replaces it.

**Never fabricating needs a mechanism, because `DeviceConditions` cannot say
unknown.** Every field is a plain value, `thermal_c: u16` and
`battery_pct: u8` among them, so a sensor must supply a number even for a
signal it cannot read. The distinction that survives is between a value
nobody asserted and a value the owner did: `ObservedConditions::spare()`
inventing 40 degrees and full battery is fabrication, while an owner-stated
fallback is a claim someone made. The rule that makes O-2 checkable rather
than aspirational is that a policy rule may be active only when its signal
is sensed or explicitly stated, and composition refuses a policy whose
active rules rest on neither. A device that cannot read thermals therefore
disables the thermal ceiling rather than feeding an invented temperature
into a live check.

### The storage layers, and where artifacts live

The first pass through Assemble asked only about physical job bytes and
missed the layer that matters most for what Distillery produces. Three
layers are in play, and they are not alternatives:

- `transport::BlobStore` holds physical content-addressed bytes. Mesh job
  inputs and outputs live here, custody-tagged and released by maintenance.
- `muniment::Backend` (redb) holds operations and engrams. A running Djinn
  already opens several distinct ones: one per personal graph, plus Knot's
  sync store. Distillery's `MeshStore` is another. There is no single
  canonical persona store today; it is per-lane.
- Eidetic is not a store. It is the typed-payload and schema layer over any
  muniment `Backend`, so engrams live in whichever store a caller hands it.

`TrainerResource` takes an Eidetic store handle separately from the mesh
blob space, and that is where everything the port produces lands: the
corpus, the adapter manifest, the evaluation report, and the adapter
weights as opaque blob engrams. Job bytes and artifacts have different
lifetimes — one is collectable, the other is meant to be found later — so
they are separate stores by construction, and the artifact store is the
product decision.

**Ruled 2026-08-30.** Artifacts live in a persona-scoped model library: a
muniment store under the persona's data root, distinct from both the
personal graph and any one mesh. Adapters outlive the mesh that trained
them and stay findable across lanes, without coupling Distillery's writes
to the personal-graph replica's lifecycle. A bridge that also publishes an
adapter as a graph engram, so it travels the same privacy, provenance, and
trust envelope as any other engram, remains available later and is not
required now.

**Job bytes stay Distillery's own.** Sharing Djinn's `transport::BlobStore`
buys little for ephemeral mesh-tagged bytes and costs the `Arc` unwrap that
breaks Djinn's shutdown plus the read-authorizer bypass. Djinn still owns
identity, settings, lifecycle, and shutdown ordering.

**The backend generic resolves concretely.** Both stores are `RedbBackend`
— the mesh operation log at `mesh.redb`, the model library at its own file
— so `DjinnResident` stays non-generic and no other lane is disturbed.

**Thermal is out of the first sensor.** Battery, idle, and network class
all come from the `windows` crate without COM; deferring thermal keeps COM
out of the resident entirely, and by the composition rule above a device
that does not sense thermals runs with the thermal ceiling disabled rather
than with an invented temperature.

### Action, landed 2026-09-01

The composition runs. Three lanes, each gated before the next:

- **Device lending settings** (pandect): `DeviceSettings` gained an optional
  `mesh_lending` block — the lending rules plus `stated` condition fallbacks,
  every field an owner statement with no `Default`, validated on load and
  save, old files still loading. Pandect stays mesh-free: the block is plain
  data and the consumer converts it.
- **The sensor** (`djinn::conditions`): `DeviceConditionSensor` implements
  `ConditionSource` over plain Win32 calls — `GetLastInputInfo`,
  `GetSystemPowerStatus`, `GetAdaptersAddresses` — with no COM anywhere.
  Every signal carries a `SignalProvenance` (sensed, stated, absent); absent
  signals report the fail-closed value whose direction a test verifies
  against each policy rule, and `validate_policy_coverage` refuses, at
  composition time, a policy whose enabled rules rest on absent signals. A
  positive-control assertion keeps the Win32 layer honest: on Windows at
  least one of idle, battery, or network must come back sensed.
- **The lane** (`djinn::resident_distillery`): the fourth resident lane,
  absent by default in `OwnerSettings`. Its open adopts or refuses
  Distillery's persisted profile against Djinn's resolved one, derives the
  personal mesh id under `DISTILLERY_MESH_SALT`, refuses an enabled lane on
  a device with no lending posture, converts the posture, validates
  coverage, opens the redb mesh store under the owner's retention revision
  with the derived mesh author as checkpoint authority, reads real memory
  through `GlobalMemoryStatusEx` (refusing on a build that cannot), and
  binds through the ordinary `bind_resident` — `SystemClock`, `NoCourier`
  (truthful for the single-device ruling), builtin registry, no
  `permissive()` anywhere. Djinn's run loop gained the cancellation signal
  `run_until` needed and drives the lane as a fourth `select!` arm.

The receipt (`ports/djinn/tests/distillery_lane.rs`) starts the resident
from explicit settings on `SystemClock`, posts `mesh.blake3/v1` as the
lane's own derived author, observes `Claimed`, `Started`, `Completed`
through the run loop's receipts, stops through the shutdown signal, and
closes clean in the ordered teardown, asserting the installed policy equals
the converted settings and is not `permissive()`. Companion receipts prove
the three refusals: no lending posture, an enabled thermal rule with no
stated temperature, and an absent lane composing no works at all.

Explicit deferrals, recorded rather than defaulted: the trainer resource is
not wired into Djinn (it would pull the Burn stack into the desktop
resident; it stays behind Distillery's `trainer` feature until wanted), so
the persona model library has no code expression yet; `allowed_resources`
and `accepted_checkpoints` have no settings fields yet, so they remain
mesh's own empty-means-unrestricted; thermal and bandwidth are stated-only;
the off-Windows path reports the UTC hour for `local_hour` and is
stated-only territory.

### The full workspace gate, closed 2026-09-01

The gate every earlier receipt deferred to has been run and is green. It had
never been reached before: earlier attempts spent their bounded runs in
dependency resolution without arriving at `rustc`, so every Distillery
receipt to date was an isolated exact-source harness.

`cargo check --workspace --all-targets` now finishes clean from an empty
target directory across all 101 members. Two breaks stood in the way, and
neither is visible to a per-package build:

- Distillery was the only crate restating Genet revisions by hand rather
  than following the workspace — for `genet-host-api` and for the
  `genet-scripted-dom` and `layout-dom-api` dev-dependencies. Four
  alignment commits had dragged those lines along; the move to `eff0cb6d`
  missed them, so Distillery compiled against one `genet-host-api` while
  Cambium carried another, and its own `SurfaceDescriptor` would not
  compare with itself. All three now follow the workspace, which is what
  `knot-document` already does.
- The `mesh_lending` field added to pandect's `DeviceSettings` reached
  three struct literals in Knot that a `-p pandect` run never compiles.

`cargo test --workspace --no-fail-fast` then ran 264 suites: **3664 passed,
1 failed**. The single failure is
`graphshell::native::personal_sync_host::tests::a_revocation_on_one_device_reaches_the_others`,
a multi-device replication test that polls with sleeps; it failed once
under a saturated machine and passes three times out of three in
isolation. It is load-flaky rather than a regression, and it sits in a
crate neither fix touches.

Two environment facts are worth recording, because they cost more time than
the code did. Running the full test build at cargo's default parallelism
(16 jobs on this 16-core, 31 GB machine) crashes `rustc` with
`STATUS_STACK_BUFFER_OVERRUN` while linking large debug binaries, and a
crashed compile leaves truncated rlibs that Cargo then considers fresh, so
later runs fail with unresolved externals that look like real link errors.
Recovering required wiping the target directory rather than cleaning
individual packages, since dependents keep referencing the old metadata
hash. The passing run used `-j 4` and `CARGO_PROFILE_DEV_DEBUG=line-tables-only`
by environment, changing no committed profile.

### Done conditions

An installed Distillery starts a resident on this machine from stated policy,
runs a real job to completion through cadenced ticks on `SystemClock`, emits
its receipts, and shuts down clean — with every policy fact traceable to an
explicit owner choice rather than a placeholder, and no `permissive()`,
`NoCourier`-by-omission, or fixed-snapshot condition surviving into the
product path.
